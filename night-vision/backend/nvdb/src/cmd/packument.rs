use anyhow::{Context as _, Result, bail};
use camino::Utf8PathBuf;
use chrono::Utc;
use nv_common::npm::packument::parse_packument;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    process::Command,
};
use url::Url;

use crate::destructive::DestructiveOperationToken;

const CORPUS_REFRESH_STATE_FILE: &str = "../nv-common/testdata/npm/packument/corpus-refresh.json";
const CORPUS_CATALOG_FILE: &str = "../nv-common/testdata/npm/packument/corpus.json";
const CORPUS_REAL_DIRECTORY: &str = "../nv-common/testdata/npm/packument/real";
const CORPUS_CACHE_DIRECTORY: &str = "../nv-common/testdata/npm/packument/cache";
const MAX_PACKUMENT_BYTES: &str = "67108864";

pub fn command() -> clap::Command {
    clap::Command::new("packument")
        .about("Inspect npm packument parser compatibility")
        .arg_required_else_help(true)
        .subcommand(
            clap::Command::new("corpus-add")
                .about("Fetch, validate, and add a reviewed packument fixture")
                .arg(
                    clap::Arg::new("package")
                        .required(true)
                        .value_name("PACKAGE")
                        .help("Public npm package name to add"),
                )
                .arg(
                    clap::Arg::new("destructive")
                        .short('w')
                        .long("destructive")
                        .required(true)
                        .action(clap::ArgAction::SetTrue)
                        .help("Acknowledge this command updates reviewed corpus files"),
                )
                .arg(registry_argument()),
        )
        .subcommand(
            clap::Command::new("corpus-refresh")
                .about("Fetch the curated corpus and write a compatibility summary")
                .arg(
                    clap::Arg::new("destructive")
                        .short('w')
                        .long("destructive")
                        .required(true)
                        .action(clap::ArgAction::SetTrue)
                        .help("Acknowledge this command updates corpus compatibility state"),
                )
                .arg(registry_argument()),
        )
}

fn registry_argument() -> clap::Arg {
    clap::Arg::new("registry")
        .long("registry")
        .default_value("https://registry.npmjs.org/")
        .value_name("URL")
        .value_parser(clap::value_parser!(Url))
        .help("Registry base URL; useful for controlled test registries")
}

pub fn run(matches: &clap::ArgMatches) -> Result<()> {
    if let Some(add_matches) = matches.subcommand_matches("corpus-add") {
        let token = DestructiveOperationToken::new(add_matches);
        let package = add_matches
            .get_one::<String>("package")
            .expect("package is required by clap");
        let registry = add_matches
            .get_one::<Url>("registry")
            .expect("registry has a clap default");
        return add_corpus_package(registry, package, token);
    }

    if let Some(refresh_matches) = matches.subcommand_matches("corpus-refresh") {
        let token = DestructiveOperationToken::new(refresh_matches);
        let registry = refresh_matches
            .get_one::<Url>("registry")
            .expect("registry has a clap default");
        return refresh_corpus(registry, token);
    }

    Ok(())
}

fn refresh_corpus(registry: &Url, _token: DestructiveOperationToken) -> Result<()> {
    let catalog = load_catalog()?;
    let results = catalog
        .fixtures
        .iter()
        .map(|fixture| refresh_package(registry, &fixture.package))
        .collect::<Vec<_>>();
    let summary = CorpusRefreshSummary {
        schema_version: 1,
        checked_at: Utc::now().to_rfc3339(),
        registry: registry.as_str(),
        packages: results,
    };

    let state_path = corpus_refresh_state_path();
    let output = File::create(&state_path)
        .with_context(|| format!("failed to create corpus refresh state at {state_path}"))?;
    serde_json::to_writer_pretty(output, &summary)
        .context("failed to write corpus refresh summary")?;

    let accepted = summary
        .packages
        .iter()
        .filter(|package| package.status == CorpusStatus::Accepted)
        .count();
    println!(
        "packument corpus: {accepted}/{} accepted; state: {state_path}",
        summary.packages.len()
    );

    Ok(())
}

fn corpus_refresh_state_path() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_REFRESH_STATE_FILE)
}

fn corpus_catalog_path() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_CATALOG_FILE)
}

fn corpus_real_directory() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_REAL_DIRECTORY)
}

fn corpus_cache_directory() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_CACHE_DIRECTORY)
}

fn load_catalog() -> Result<CorpusCatalog> {
    let catalog_path = corpus_catalog_path();
    serde_json::from_reader(
        File::open(&catalog_path)
            .with_context(|| format!("failed to read corpus catalog at {catalog_path}"))?,
    )
    .context("failed to parse corpus catalog")
}

fn add_corpus_package(
    registry: &Url,
    package: &str,
    _token: DestructiveOperationToken,
) -> Result<()> {
    let registry_path = registry_path_for_package(package)?;
    let endpoint = registry
        .join(&registry_path)
        .with_context(|| format!("failed to build registry URL for {package}"))?;
    let body = fetch_packument(registry, package, &registry_path)?;
    let parsed = parse_packument(body.as_slice())
        .with_context(|| format!("registry response for {package} is not a supported packument"))?;

    if parsed.name.as_str() != package {
        bail!("registry response package name does not match requested package {package}");
    }

    let catalog_path = corpus_catalog_path();
    let mut catalog = load_catalog()?;
    if catalog
        .fixtures
        .iter()
        .any(|fixture| fixture.package == package)
    {
        bail!("corpus already contains package {package}");
    }

    let fixture = format!("{}.json", fixture_stem(package)?);
    let fixture_path = corpus_real_directory().join(&fixture);
    if fixture_path.exists() {
        bail!("corpus fixture already exists at {fixture_path}");
    }

    let cache_path = corpus_cache_directory().join(&fixture);
    fs::create_dir_all(cache_path.parent().expect("cache path has a parent"))
        .with_context(|| format!("failed to create corpus cache directory for {cache_path}"))?;
    fs::write(&cache_path, &body)
        .with_context(|| format!("failed to write untracked packument cache at {cache_path}"))?;
    fs::write(&fixture_path, minimize_packument_fixture(&body)?)
        .with_context(|| format!("failed to write corpus fixture at {fixture_path}"))?;
    catalog.fixtures.push(CorpusFixture {
        package: package.to_owned(),
        fixture: format!("real/{fixture}"),
        source: endpoint.to_string(),
        captured_at: Utc::now().date_naive().to_string(),
    });
    let catalog_file = File::create(&catalog_path)
        .with_context(|| format!("failed to update corpus catalog at {catalog_path}"))?;
    serde_json::to_writer_pretty(catalog_file, &catalog)
        .context("failed to write corpus catalog")?;

    println!("added packument corpus fixture: {package} ({fixture_path})");
    Ok(())
}

fn minimize_packument_fixture(body: &[u8]) -> Result<Vec<u8>> {
    let packument: serde_json::Value = serde_json::from_slice(body)
        .context("failed to parse fetched packument JSON for fixture minimization")?;
    let name = packument
        .get("name")
        .cloned()
        .context("fetched packument is missing name")?;
    let dist_tags = packument
        .get("dist-tags")
        .and_then(serde_json::Value::as_object)
        .context("fetched packument is missing dist-tags")?;
    let versions = packument
        .get("versions")
        .and_then(serde_json::Value::as_object)
        .context("fetched packument is missing versions")?;

    let mut selected_versions = serde_json::Map::new();
    for version in dist_tags.values() {
        let version = version
            .as_str()
            .context("packument dist tag does not name a version")?;
        let metadata = versions
            .get(version)
            .with_context(|| format!("packument dist tag points to missing version {version}"))?;
        selected_versions.insert(version.to_owned(), metadata.clone());
    }

    serde_json::to_vec_pretty(&serde_json::json!({
        "name": name,
        "dist-tags": dist_tags,
        "versions": selected_versions,
    }))
    .context("failed to serialize minimized packument fixture")
}

fn registry_path_for_package(package: &str) -> Result<String> {
    let stem = fixture_stem(package)?;
    if let Some(scoped) = package.strip_prefix('@') {
        let (scope, name) = scoped.split_once('/').expect("validated scoped package");
        return Ok(format!("%40{scope}%2F{name}"));
    }

    Ok(stem)
}

fn fixture_stem(package: &str) -> Result<String> {
    if package.is_empty() || package.len() > 214 {
        bail!("invalid npm package name {package:?}");
    }

    let scoped = package.starts_with('@');
    let package = package.strip_prefix('@').unwrap_or(package);
    let mut parts = package.split('/');
    let first = parts
        .next()
        .expect("nonempty package has a first component");
    let second = parts.next();
    if parts.next().is_some() || (scoped && second.is_none()) || (!scoped && second.is_some()) {
        bail!("invalid npm package name {package:?}");
    }

    let valid_component = |component: &str| {
        !component.is_empty()
            && !component.starts_with(['.', '_'])
            && component.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.')
            })
    };
    if !valid_component(first) || second.is_some_and(|component| !valid_component(component)) {
        bail!("invalid npm package name {package:?}");
    }

    Ok(second.map_or_else(|| first.to_owned(), |second| format!("{first}--{second}")))
}

fn refresh_package(registry: &Url, package: &str) -> CorpusRefreshPackage {
    let registry_path = match registry_path_for_package(package) {
        Ok(path) => path,
        Err(error) => {
            return CorpusRefreshPackage {
                name: package.to_owned(),
                status: CorpusStatus::Rejected,
                versions: None,
                error: Some(error.to_string()),
            };
        }
    };

    match fetch_packument(registry, package, &registry_path) {
        Ok(body) => match parse_packument(body.as_slice()) {
            Ok(packument) => CorpusRefreshPackage {
                name: package.to_owned(),
                status: CorpusStatus::Accepted,
                versions: Some(packument.versions.len()),
                error: None,
            },
            Err(error) => CorpusRefreshPackage {
                name: package.to_owned(),
                status: CorpusStatus::Rejected,
                versions: None,
                error: Some(error.to_string()),
            },
        },
        Err(error) => CorpusRefreshPackage {
            name: package.to_owned(),
            status: CorpusStatus::FetchFailed,
            versions: None,
            error: Some(error.to_string()),
        },
    }
}

fn fetch_packument(registry: &Url, package: &str, registry_path: &str) -> Result<Vec<u8>> {
    let endpoint = registry
        .join(registry_path)
        .with_context(|| format!("failed to build registry URL for {package}"))?;
    let response = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--max-filesize",
            MAX_PACKUMENT_BYTES,
            "--header",
            "Accept: application/json",
        ])
        .arg(endpoint.as_str())
        .output()
        .context("failed to run curl; install curl to refresh the packument corpus")?;

    if !response.status.success() {
        bail!(
            "curl failed for {}: {}",
            package,
            String::from_utf8_lossy(&response.stderr).trim()
        );
    }

    Ok(response.stdout)
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusCatalog {
    schema_version: u8,
    fixtures: Vec<CorpusFixture>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusFixture {
    package: String,
    fixture: String,
    source: String,
    captured_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusRefreshSummary<'a> {
    schema_version: u8,
    checked_at: String,
    registry: &'a str,
    packages: Vec<CorpusRefreshPackage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusRefreshPackage {
    name: String,
    status: CorpusStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    versions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CorpusStatus {
    Accepted,
    Rejected,
    FetchFailed,
}

#[cfg(test)]
mod tests {
    use super::{command, corpus_refresh_state_path, fixture_stem, load_catalog};
    use camino::Utf8PathBuf;

    #[test]
    fn corpus_refresh_requires_destructive_flag() {
        let error = command()
            .try_get_matches_from(["packument", "corpus-refresh"])
            .expect_err("missing destructive flag should fail");

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn corpus_refresh_writes_the_fixed_project_state_file() {
        assert_eq!(
            corpus_refresh_state_path(),
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../nv-common/testdata/npm/packument/corpus-refresh.json")
        );
    }

    #[test]
    fn corpus_add_requires_destructive_flag() {
        let error = command()
            .try_get_matches_from(["packument", "corpus-add", "example"])
            .expect_err("missing corpus-add options should fail");

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn fixture_stems_preserve_scopes_without_path_separators() {
        assert_eq!(fixture_stem("example").unwrap(), "example");
        assert_eq!(fixture_stem("@scope/package").unwrap(), "scope--package");
        fixture_stem("../escape").expect_err("invalid package name should fail");
    }

    #[test]
    fn corpus_catalog_contains_diverse_public_packages() {
        let catalog = load_catalog().expect("corpus catalog should parse");
        let names = catalog
            .fixtures
            .iter()
            .map(|fixture| fixture.package.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"express"));
        assert!(names.contains(&"@babel/core"));
        assert!(names.contains(&"@types/node"));
    }
}
