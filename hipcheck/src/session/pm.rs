// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]
use crate::context::Context as _;
use crate::error::Error;
use crate::error::Result;
use crate::hc_error;
use crate::http::tls::new_agent;
use crate::CheckKind;
use crate::EXIT_FAILURE;
use serde_json::Value;
use std::cmp::max;
use std::cmp::Ordering;
use std::process::exit;
use url::Host;
use url::Url;
use xml::reader::EventReader;
use xml::reader::XmlEvent;

const MAVEN: &str = CheckKind::Maven.name();
const NPM: &str = CheckKind::Npm.name();
const PYPI: &str = CheckKind::Pypi.name();

pub fn detect_and_extract(raw_package: &str, parent_command_value: String) -> Result<Url> {
	//using parent_command_value string here because we can not detect package type if/when package@version is given and not uri

	let package_trimmed = raw_package.trim(); //trimming leading/trailing white space
	match parent_command_value.as_str() {
		NPM => extract_repo_for_npm(package_trimmed),
		PYPI => extract_repo_for_pypi(package_trimmed),
		MAVEN => extract_repo_for_maven(package_trimmed),
		_ => Err(Error::msg("not a known package")),
	}
}

#[derive(Debug, Copy, Clone)]
enum PackageManager {
	Npm,

	Pypi,

	Maven,
}

/*=============================================================================
 * Key methods.
 *
 * Implements the key logic to 1) process the JSON object (`find_repo_url`) and
 * 2) score URLs in `info.project_urls` for how likely they are to be the
 * repository URL.
 *---------------------------------------------------------------------------*/

/// Performs the work of translating a relative URL to an absolute one.
fn transform_relative(rel_url: String) -> Result<Url> {
	let stripped = rel_url.strip_prefix("git@").unwrap_or(&rel_url);
	let absolute = stripped.replace(':', "/");
	let url_string = format!("https://{}", absolute);
	let final_url = Url::parse(&url_string).context("unable to parse url")?;
	Ok(final_url)
}

/// Transforms a URL string into a Url object, trimming unnecessary prefixes
/// and attempting to convert relative to absolute URLs.
fn sanitize_url(url: String) -> Result<Url> {
	// Trim unnecessary "scm:" prefix that appears on the front of some URLs.
	let trimmed_url = url.strip_prefix("scm:").unwrap_or(&url);

	// Trim "git:" prefix when "git:" is not a substring of scheme "git://".
	let cleaned_url = if trimmed_url.starts_with("git:") && !trimmed_url.starts_with("git://") {
		trimmed_url.strip_prefix("git:").unwrap_or(trimmed_url)
	} else {
		trimmed_url
	};
	// Recognize a "git@git://" url and transform it, or else parse the url normally.
	if cleaned_url.starts_with("git@") {
		transform_relative(cleaned_url.to_string())
	} else {
		let final_url = Url::parse(cleaned_url).context("unable to parse url")?;
		Ok(final_url)
	}
}

/// Parse a Maven POM document for candidate repository URLs.
fn retrieve_maven_urls(pom: &str) -> Vec<String> {
	let pom_reader = pom.as_bytes();
	let mut parser = EventReader::new(pom_reader);
	let mut urls: Vec<String> = Vec::new();
	loop {
		let e = parser.next();
		match e {
			Ok(XmlEvent::StartElement { name, .. }) => {
				if name.local_name == "connection" || name.local_name == "developerConnection" {
					match parser.next() {
						Ok(XmlEvent::Characters(s)) => {
							urls.push(s);
						}
						Err(err) => {
							println!("Error: {}", err);
						}
						_ => {}
					}
				}
			}
			Ok(XmlEvent::EndDocument) => {
				break;
			}

			Err(err) => {
				println!("Error: {}", err);
				break;
			}
			_ => {}
		}
	}
	urls
}

/// Perform the scoring of the candidate URL.
///
/// A score equal to 0 means "this is not worth considering". A score of 1 or more
/// is for URLs worth considering. The highest-scoring URL should be the best candidate
/// for a repository URL. The actual scoring system may be subject to change in future.
fn score_candidate_url(_name: &str, url: &Url) -> Score {
	let score = score_url(url);

	/* URLs which receive no points are ignored. */
	if score == 0 {
		return Score::Ignore;
	}

	Score::Candidate(score)
}

fn score_url(url: &Url) -> u64 {
	let mut score: u64 = 0;

	/* Known repository hosts are worth consideration. Add other useful hosts
	 * here as needed. */
	if let Some("github.com" | "gitlab.com") = url.host_str() {
		score += 1;
	}

	/* URLs using the git:// protocol are prioritized. */
	if url.scheme() == "git" {
		score += 2;
	}

	/* URLs specifically identifying a git repository are prioritized. */
	if url.path().contains(".git") {
		score += 2;
	}

	score
}

/// Find the best repository URL.
fn find_repo_url(json: &Value) -> Result<Url> {
	json
		// Get the 'project_urls' field.
		.pointer("/info/project_urls")
		.ok_or_else(|| hc_error!("missing 'project_urls'"))?
		// Convert it to a `serde_json::Map`
		.as_object()
		.ok_or_else(|| hc_error!("'project_urls' is not a map"))?
		// Iterate over it with `impl Iterator<Item = (&String, &Value)>`
		.iter()
		// Convert into `impl Iterator<Item = CandidateUrl<'_>>`, discarding
		// any entries with invalid URLs.
		.filter_map(CandidateUrl::new)
		// Score the URLs, converting into
		// `impl Iterator<Item = ScoredCandidateUrl<'_>>`, discarding
		// entries which are not possible repository URLs.
		.filter_map(ScoredCandidateUrl::score)
		// Pick out the highest-scoring URL.
		.max()
		// Discard the score and name, keeping only the URL itself.
		.map(ScoredCandidateUrl::into_url)
		// If nothing, report the error.
		.ok_or_else(|| hc_error!("no repository URL found"))
}

/*=============================================================================
 * Supporting types.
 *
 * These types support the intermediate work needed to find the best URL.
 *---------------------------------------------------------------------------*/

/// The score assigned to a possible URL.
enum Score {
	/// This URL _might_ be the repository URL, with a higher number indicating
	/// higher likelihood.
	Candidate(u64),

	/// This URL is _not_ likely to be the repository URL, and should be
	/// ignored.
	Ignore,
}

impl Score {
	/// Convert `Score` into `Option<u64>`.
	fn ok(self) -> Option<u64> {
		match self {
			Score::Candidate(score) => Some(score),
			Score::Ignore => None,
		}
	}
}

/// Represents a URL which _might_ be the repository URL.
struct CandidateUrl<'s> {
	name: &'s str,
	url: Url,
}

impl<'s> CandidateUrl<'s> {
	/// Make a new candidate, parsing the `URL` and failing if invalid.
	fn new((name, raw_url): (&'s String, &Value)) -> Option<Self> {
		Url::parse(raw_url.to_string().trim_matches('"'))
			.map(|url| CandidateUrl { name, url })
			.ok()
	}
}

/// Represents a URL which _might_ be the repository URL, with a score based
/// on how likely it is to be the right one.
struct ScoredCandidateUrl<'s> {
	score: u64,
	candidate_url: CandidateUrl<'s>,
}

impl<'s> ScoredCandidateUrl<'s> {
	/// Score the `candidate_url`, failing if it's not worth considering as
	/// a candidate.
	fn score(candidate_url: CandidateUrl<'s>) -> Option<Self> {
		score_candidate_url(candidate_url.name, &candidate_url.url)
			.ok()
			.map(|score| ScoredCandidateUrl {
				score,
				candidate_url,
			})
	}

	/// Extract the inner `Url`.
	fn into_url(self) -> Url {
		self.candidate_url.url
	}
}

/*=============================================================================
 * Trait impls
 *
 * These make it so we can call `Iterator::max` with an
 * `impl Iterator<Item = ScoredCandidateUrl<'_>>`, which requires `Item: Ord`.
 *
 * All the impls just delegate to the impl on the `score` field.
 *---------------------------------------------------------------------------*/

impl<'s> PartialEq for ScoredCandidateUrl<'s> {
	fn eq(&self, other: &ScoredCandidateUrl<'s>) -> bool {
		self.score.eq(&other.score)
	}
}

impl<'s> Eq for ScoredCandidateUrl<'s> {}

impl<'s> PartialOrd for ScoredCandidateUrl<'s> {
	fn partial_cmp(&self, other: &ScoredCandidateUrl<'s>) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl<'s> Ord for ScoredCandidateUrl<'s> {
	fn cmp(&self, other: &ScoredCandidateUrl<'s>) -> Ordering {
		self.score.cmp(&other.score)
	}
}

//============================

impl PackageManager {
	fn detect(url: &Url) -> Result<PackageManager> {
		match url.host() {
			Some(Host::Domain("registry.npmjs.org")) => Ok(PackageManager::Npm),
			Some(Host::Domain(
				"pypi.io" | "pypi.org" | "pypi.python.org" | "files.pythonhosted.org",
			)) => Ok(PackageManager::Pypi),
			Some(Host::Domain("repo.maven.apache.org")) => Ok(PackageManager::Maven),
			_ => Err(Error::msg("not a known package manager URL")),
		}
	}
}
fn extract_package_version(raw_package: &str) -> Result<(String, String)> {
	// Get the package and version from package argument in form package@version because it has @ symbol in it
	let mut package_and_version = raw_package.split('@');
	let package_value = match package_and_version.next() {
		Some(package) => Ok(package),
		_ => Err(Error::msg("unable to get package from package@version")),
	};
	Ok((
		package_value.unwrap().to_string(), //this wont panic because we check for it above
		package_and_version
			.next()
			.unwrap_or("no version")
			.to_string(), //we check for this in match so we can format url correctly
	))
}

fn extract_package_version_from_url(url: Url) -> Result<(String, String)> {
	//Get package and version from the URL, npm and pypi only
	//Note maven urls are too complex to work with the npm and pypi url parsing model below
	let package_type = match PackageManager::detect(&url) {
		Ok(PackageManager::Npm) => NPM,
		Ok(PackageManager::Pypi) => PYPI,
		_ => "no package found for package url",
	};

	// Get the package and version from the URL
	if package_type.contains(NPM) {
		//npm gets the first two segments
		let (package, version) = url
			.path_segments()
			.map(|mut i| (i.next(), i.next()))
			.ok_or_else(|| hc_error!("can't detect package name"))?;
		Ok((
			package.unwrap().to_string(), //this will graceful error if empty because of the mapping above I believe
			version.unwrap_or("no version").to_string(),
		))
	} else if package_type.contains(PYPI) {
		//pypi gets the second and third segments
		let mut path_segments = url
			.path_segments()
			.ok_or_else(|| hc_error!("Unable to get path"))?;
		let _project = path_segments.next();
		let package_value = match path_segments.next() {
			Some(package) => Ok(package),
			_ => Err(Error::msg("unable to get package from uri")),
		};
		let version = path_segments.next();
		Ok((
			package_value.unwrap().to_string(), //this will graceful error if empty because of panic checking above
			version.unwrap_or("no version").to_string(), //we check for this in match so we can format url correctly
		))
	} else {
		Err(Error::msg("not a known package manager URL"))
	}
}

fn extract_repo_for_npm(raw_package: &str) -> Result<Url> {
	// Get the package and version from passed in package value in url format or package@version
	let (package, version) = match Url::parse(raw_package) {
		Ok(url_parsed) => extract_package_version_from_url(url_parsed).unwrap(),
		_ => extract_package_version(raw_package).unwrap(),
	};

	// Construct the registry URL.
	let package = error_if_empty(Some(&package), "no repository given for npm package");
	let version = warn_if_empty(
		Some(&version),
		"no version given for npm package; getting URL for latest version",
	);
	let registry = match version {
		"no version" => format!("https://registry.npmjs.org/{}", package),
		_ => format!("https://registry.npmjs.org/{}/{}", package, version),
	};

	// Make an HTTP request to that URL.
	let response = new_agent()?
		.get(&registry)
		.call()
		.context("request to npm API failed, make sure the package name is correct as well as the project version")?;

	// Parse the response as JSON.
	let json: Value = {
		let intermediate = response
			.into_string()
			.context("can't parse npm API response")?;
		serde_json::from_str(&intermediate).context("npm API response isn't valid JSON")?
	};

	// Get the repository URL from the JSON object.
	let repository = {
		let raw_repository = json
			.get("repository")
			.ok_or_else(|| hc_error!("no repository field for package"))?
			.get("url")
			.ok_or_else(|| hc_error!("no repository url field for package"))?
			.to_string();

		let raw_repository = raw_repository.trim_matches('"');

		let raw_repository = raw_repository
			.strip_prefix("git+")
			.unwrap_or(raw_repository);

		Url::parse(raw_repository).context("invalid repository URL from npm API")?
	};

	Ok(repository)
}

fn extract_repo_for_pypi(raw_package: &str) -> Result<Url> {
	// Get the package and version from passed in package value in url format or package@version
	let (package, version) = match Url::parse(raw_package) {
		Ok(url_parsed) => extract_package_version_from_url(url_parsed).unwrap(),
		_ => extract_package_version(raw_package).unwrap(),
	};
	let package = error_if_empty(Some(&package), "no repository given for python package");

	// Construct the registry URL.
	let registry = match version.as_str() {
		"no version" => format!("https://pypi.org/pypi/{}/json", package),
		_ => format!("https://pypi.org/pypi/{}/{}/json", package, version),
	};

	// Make an HTTP request to that URL.
	let response = new_agent()?
		.get(&registry)
		.call()
		.context("request to PYPI API failed, make sure the project name is correct (case matters) as well as the project version")?;

	// Parse the response as JSON.
	let json: Value = {
		let intermediate = response
			.into_string()
			.context("can't parse PYPI API response")?;
		serde_json::from_str(&intermediate).context("PYPI API response isn't valid JSON")?
	};

	//checking for panic on repo url find
	let repository = match find_repo_url(&json) {
		Ok(repository) => Ok(repository),
		_ => {
			return Err(Error::msg(
				"Unable to get git repository URL from python package",
			))
		}
	};
	let repo = repository.clone().unwrap();
	if let Some("github.com" | "gitlab.com") = repo.host_str() {
		pop_url_segments(repo)
	} else {
		repository
	}
}

fn extract_repo_for_maven(url: &str) -> Result<Url> {
	// Make an HTTP request to that URL to get the POM file.

	let response = new_agent()?
		.get(url)
		.call()
		.context("request to Maven API failed")?;

	let intermediate: String = response
		.into_string()
		.context("can't parse Maven POM response")?;

	let url_list: Vec<String> = retrieve_maven_urls(&intermediate);
	let best_scoring_url = url_list
		.into_iter()
		.filter_map(|s| sanitize_url(s).ok())
		.max_by(|a, b| score_url(a).cmp(&score_url(b)));

	best_scoring_url.ok_or_else(|| hc_error!("no valid repository URL found"))
}

/// Remove unnecessary path segments from tail end of repository URL.
fn pop_url_segments(mut repo: Url) -> Result<Url> {
	let times_to_pop = max(
		repo.path_segments()
			.ok_or_else(|| hc_error!("Unable to get path"))?
			.count() - 2,
		0,
	);
	for _ in 0..times_to_pop {
		repo.path_segments_mut()
			.map_err(|_| hc_error!("No path found in URL"))?
			.pop();
	}
	Ok(repo)
}

/// Print an error message if the string is empty or `None`.
fn error_if_empty<'out, 'inp: 'out>(s: Option<&'inp str>, msg: &'static str) -> &'out str {
	if is_none_or_empty(s) {
		eprintln!("error: {}", msg);
		exit(EXIT_FAILURE);
	} else {
		s.unwrap()
	}
}

/// Print a warning if the string is empty or `None`.
fn warn_if_empty<'out, 'inp: 'out>(s: Option<&'inp str>, msg: &'static str) -> &'out str {
	if is_none_or_empty(s) {
		eprintln!("warning: {}", msg);
		""
	} else {
		s.unwrap()
	}
}

fn is_none_or_empty(s: Option<&str>) -> bool {
	match s {
		Some(s) => s.is_empty(),
		None => true,
	}
}

#[cfg(test)]
mod tests {
	// Note this useful idiom: importing names from outer (for mod tests) scope.
	use super::*;
	use serde_json::json;
	use url::Url;

	#[test]
	fn test_extract_repo_for_pypi() {
		let link = "https://pypi.org/project/certifi/2021.5.30";
		let link2 = "https://github.com/certifi/python-certifi";
		let pypi_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_pypi(link).unwrap(), pypi_git);
	}

	#[test]
	fn test_extract_repo_for_pypi_2() {
		let link = "https://pypi.org/project/urllib3/1.26.6";
		let link2 = "https://github.com/urllib3/urllib3";
		let pypi_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_pypi(link).unwrap(), pypi_git);
	}

	#[test]
	fn test_extract_repo_for_pypi_3() {
		let link = "https://pypi.org/project/Flask/2.1.1";
		let link2 = "https://github.com/pallets/flask";
		let pypi_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_pypi(link).unwrap(), pypi_git);
	}

	#[test]
	fn test_extract_repo_for_pypi_4() {
		let link = "Flask@2.1.1";
		let link2 = "https://github.com/pallets/flask";
		let pypi_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_pypi(link).unwrap(), pypi_git);
	}

	#[test]
	fn test_extract_repo_for_pypi_5() {
		let link = "Flask";
		let link2 = "https://github.com/pallets/flask";
		let pypi_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_pypi(link).unwrap(), pypi_git);
	}

	#[test]
	fn test_extract_repo_for_pypi_6() {
		let link = "flask";
		let link2 = "https://github.com/pallets/flask";
		let pypi_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_pypi(link).unwrap(), pypi_git);
	}

	#[test]
	fn test_extract_repo_for_pypi_7() {
		//should fail
		let link = "Flaskx";
		let link2 = "https://github.com/pallets/flask";
		let pypi_git = Url::parse(link2).unwrap();
		println!("{}", extract_repo_for_pypi(link).unwrap().as_str());
		assert_ne!(extract_repo_for_pypi(link).unwrap(), pypi_git);
	}

	#[test]
	fn test_extract_repo_for_npm() {
		let link = "https://registry.npmjs.org/lodash/";
		let link2 = "https://github.com/lodash/lodash.git";
		let npm_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_npm(link).unwrap(), npm_git);
	}

	#[test]
	fn test_extract_repo_for_npm_2() {
		let link = "https://registry.npmjs.org/chalk/";
		let link2 = "https://github.com/chalk/chalk.git";
		let npm_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_npm(link).unwrap(), npm_git);
	}

	#[test]
	fn test_extract_repo_for_npm_3() {
		let link = "https://registry.npmjs.org/node-ipc/9.2.1";
		let link2 = "https://github.com/RIAEvangelist/node-ipc.git";
		let npm_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_npm(link).unwrap(), npm_git);
	}

	#[test]
	fn test_extract_repo_for_npm_4() {
		let package = "node-ipc@9.2.1";
		let link2 = "https://github.com/RIAEvangelist/node-ipc.git";
		let npm_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_npm(package).unwrap(), npm_git);
	}

	#[test]
	fn test_extract_repo_for_npm_5() {
		let package = "node-ipc";
		let link2 = "https://github.com/RIAEvangelist/node-ipc.git";
		let npm_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_npm(package).unwrap(), npm_git);
	}

	#[test]
	/// Tests scm:git: prefix removal case.
	fn test_extract_repo_for_maven_2() {
		let maven_url = "https://repo.maven.apache.org/maven2/com/fasterxml/jackson/core/jackson-databind/2.12.4/jackson-databind-2.12.4.pom";
		let link2 = "https://github.com/FasterXML/jackson-databind.git";
		let maven_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_maven(maven_url).unwrap(), maven_git);
	}

	#[test]
	/// Tests scm:git: prefix removal case on https:// scheme URL.
	fn test_extract_repo_for_maven_3() {
		let maven_url = "https://repo.maven.apache.org/maven2/joda-time/joda-time/2.10.10/joda-time-2.10.10.pom";
		let link2 = "https://github.com/JodaOrg/joda-time.git";
		let maven_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_maven(maven_url).unwrap(), maven_git);
	}

	#[test]
	/// Test scm:git: prefix removal case on git:// scheme URL.
	fn test_extract_repo_for_maven_4() {
		let maven_url = "https://repo.maven.apache.org/maven2/org/springframework/boot/spring-boot-autoconfigure/2.5.3/spring-boot-autoconfigure-2.5.3.pom";
		let link2 = "git://github.com/spring-projects/spring-boot.git";
		let maven_git = Url::parse(link2).unwrap();
		assert_eq!(extract_repo_for_maven(maven_url).unwrap(), maven_git);
	}

	#[test]
	#[should_panic(expected = "not a known package manager URL")]
	fn bad_npm_url() {
		let link = "https:/www.google.com/";
		let npm_url = Url::parse(link).unwrap();

		PackageManager::detect(&npm_url).unwrap();
	}

	#[test]
	fn repo_for_numpy() {
		let json = json!({
			"info": {
				"project_urls": {
					"Bug Tracker": "https://github.com/numpy/numpy/issues",
					"Documentation": "https://numpy.org/doc/1.21",
					"Download": "https://pypi.python.org/pypi/numpy",
					"Homepage": "https://www.numpy.org",
					"Source Code": "https://github.com/numpy/numpy"
				},
			}
		});
		let actual = find_repo_url(&json).unwrap();
		let expected = Url::parse("https://github.com/numpy/numpy").unwrap();
		assert_eq!(actual, expected);
	}

	#[test]
	fn repo_for_pandas() {
		let json = json!({
			"info": {
				"project_urls": {
					"Bug Tracker": "https://github.com/pandas-dev/pandas/issues",
					"Documentation": "https://pandas.pydata.org/pandas-docs/stable",
					"Homepage": "https://pandas.pydata.org",
					"Source Code": "https://github.com/pandas-dev/pandas",
				},
			}
		});
		let actual = find_repo_url(&json).unwrap();
		let expected = Url::parse("https://github.com/pandas-dev/pandas").unwrap();
		assert_eq!(actual, expected);
	}

	#[test]
	#[should_panic(expected = "no repository URL found")]
	fn repo_for_nodetree() {
		let json = json!({
			"info": {
				"project_urls": {
					"Download": "http://pypi.python.org/pypi/NodeTree/0.3",
					"Homepage": "http://www.nodetree.org/"
				}
			},
		});
		find_repo_url(&json).unwrap();
	}

	#[test]
	fn repo_for_keras() {
		let json = json!({
			"info": {
				"project_urls": {
					"Download": "https://github.com/keras-team/keras/tarball/2.4.3",
					"Homepage": "https://github.com/keras-team/keras",
				},
			}
		});
		let actual = find_repo_url(&json).unwrap();
		let expected = Url::parse("https://github.com/keras-team/keras").unwrap();
		assert_eq!(actual, expected);
	}

	#[test]
	fn repo_for_flake8() {
		let json = json!({
			"info" : {
				"project_urls": {
					"Homepage": "https://gitlab.com/pycqa/flake8",
					"Documentation": "http://flake8.pycqa.org/en/latest/index.html#quickstart"
				}
			}
		});
		let actual = find_repo_url(&json).unwrap();
		let expected = Url::parse("https://gitlab.com/pycqa/flake8").unwrap();
		assert_eq!(actual, expected);
	}

	#[test]
	fn test_maven_url_retrieval() {
		let test_xml = r##"
			<project xmlns="http://maven.apache.org/POM/4.0.0">
				<scm>
					<url>https://github.com/webcomponents/webcomponentsjs</url>
					<connection>https://github.com/webcomponents/webcomponentsjs.git</connection>
					<developerConnection>https://github.com/webcomponents/webcomponentsjs.git</developerConnection>
				</scm>
				<developers>
					<developer>
						<url>http://webjars.org</url>
					</developer>
				</developers>
			</project>
		"##;
		let actual = retrieve_maven_urls(&String::from(test_xml));
		let expected = vec![
			"https://github.com/webcomponents/webcomponentsjs.git",
			"https://github.com/webcomponents/webcomponentsjs.git",
		];
		assert_eq!(actual, expected);
	}
}
