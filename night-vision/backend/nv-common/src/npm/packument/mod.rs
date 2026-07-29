use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use url::Url;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RawNpmPackument {
    #[serde(rename = "_id")]
    id: Option<String>,

    #[serde(rename = "_rev")]
    rev: Option<String>,

    name: Option<String>,

    #[serde(rename = "dist-tags")]
    dist_tags: Option<HashMap<String, String>>,

    modified: Option<String>,

    time: Option<HashMap<String, String>>,

    #[serde(default)]
    versions: Option<HashMap<String, RawNpmVersion>>,

    author: Option<RawHuman>,
    bugs: Option<Value>,

    #[serde(default)]
    contributors: Option<Vec<RawHuman>>,

    description: Option<String>,
    homepage: Option<String>,
    keywords: Option<Vec<String>>,
    license: Option<String>,
    maintainers: Option<Vec<RawHuman>>,
    readme: Option<String>,

    #[serde(rename = "readmeFilename")]
    readme_filename: Option<String>,

    repository: Option<RawRepository>,

    users: Option<HashMap<String, bool>>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RawNpmVersion {
    #[serde(rename = "_id")]
    id: Option<String>,

    #[serde(rename = "_nodeVersion")]
    node_version: Option<String>,

    #[serde(rename = "_npmVersion")]
    npm_version: Option<String>,

    #[serde(rename = "_npmUser")]
    npm_user: Option<RawHuman>,

    name: Option<String>,
    version: Option<String>,

    description: Option<String>,
    main: Option<String>,
    license: Option<String>,

    author: Option<RawHuman>,
    contributors: Option<Vec<RawHuman>>,
    maintainers: Option<Vec<RawHuman>>,
    repository: Option<RawRepository>,

    dependencies: Option<HashMap<String, String>>,

    #[serde(rename = "devDependencies")]
    dev_dependencies: Option<HashMap<String, String>>,

    #[serde(rename = "peerDependencies")]
    peer_dependencies: Option<HashMap<String, String>>,

    #[serde(rename = "optionalDependencies")]
    optional_dependencies: Option<HashMap<String, String>>,

    #[serde(rename = "bundleDependencies")]
    bundle_dependencies: Option<Vec<String>>,

    #[serde(rename = "peerDependenciesMeta")]
    peer_dependencies_meta: Option<HashMap<String, RawPeerDependencyMeta>>,

    dist: Option<RawNpmDist>,

    engines: Option<HashMap<String, String>>,

    deprecated: Option<String>,

    #[serde(rename = "acceptDependencies")]
    accept_dependencies: Option<HashMap<String, String>>,

    bin: Option<RawBin>,

    directories: Option<HashMap<String, String>>,

    #[serde(rename = "_hasShrinkwrap")]
    has_shrinkwrap: Option<bool>,

    #[serde(default, rename = "hasInstallScript")]
    has_install_script: Option<bool>,

    funding: Option<RawFunding>,

    cpu: Option<Vec<String>>,

    os: Option<Vec<String>>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawFunding {
    Url(String),

    Object {
        url: String,

        #[serde(rename = "type")]
        type_field: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBin {
    String(String),
    Map(HashMap<String, String>),
}

#[derive(Debug, Deserialize)]
struct RawPeerDependencyMeta {
    optional: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawHuman {
    String(String),

    Object {
        name: Option<String>,
        email: Option<String>,
        url: Option<String>,
    },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RawRepository {
    #[serde(rename = "type")]
    type_field: Option<String>,
    url: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RawNpmDist {
    tarball: Option<String>,
    shasum: Option<String>,
    integrity: Option<String>,
    #[serde(rename = "fileCount")]
    file_count: Option<u64>,

    #[serde(rename = "unpackedSize")]
    unpacked_size: Option<u64>,

    #[serde(rename = "npm-signature")]
    npm_signature: Option<String>,
}

#[derive(Debug)]
pub struct NpmPackument {
    pub id: Option<String>,
    pub rev: Option<String>,

    pub name: String,

    pub dist_tags: HashMap<String, String>,
    pub time: HashMap<String, String>,
    pub users: HashMap<String, bool>,

    pub versions: HashMap<String, NpmVersion>,

    pub author: Option<Human>,
    pub bugs: Option<Value>,
    pub contributors: Vec<Human>,

    pub description: Option<String>,
    pub homepage: Option<String>,
    pub keywords: Vec<String>,
    pub license: Option<String>,

    pub maintainers: Vec<Human>,

    pub readme: Option<String>,
    pub readme_filename: Option<String>,

    pub repository: Option<Repository>,
    pub extra: HashMap<String, Value>,
}

#[derive(Debug)]
pub struct NpmVersion {
    pub id: Option<String>,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub npm_user: Option<Human>,

    pub name: String,
    pub version: String,

    pub description: Option<String>,
    pub main: Option<String>,
    pub license: Option<String>,

    pub author: Option<Human>,
    pub contributors: Vec<Human>,
    pub maintainers: Vec<Human>,
    pub repository: Option<Repository>,

    pub dependencies: HashMap<String, String>,
    pub accept_dependencies: HashMap<String, String>,
    pub dev_dependencies: HashMap<String, String>,
    pub peer_dependencies: HashMap<String, String>,
    pub peer_dependencies_meta: HashMap<String, PeerDependencyMeta>,
    pub optional_dependencies: HashMap<String, String>,
    pub bundle_dependencies: Vec<String>,

    pub bin: HashMap<String, String>,
    pub directories: HashMap<String, String>,

    pub engines: HashMap<String, String>,

    pub has_shrinkwrap: Option<bool>,
    pub has_install_script: Option<bool>,

    pub funding: Option<Funding>,

    pub cpu: Vec<String>,
    pub os: Vec<String>,

    pub dist: NpmDist,

    pub deprecated: Option<String>,

    pub extra: HashMap<String, Value>,
}

#[derive(Debug)]
pub struct Funding {
    pub url: String,
    pub type_field: Option<String>,
}

#[derive(Debug)]
pub struct PeerDependencyMeta {
    pub optional: bool,
}

#[derive(Debug)]
pub struct NpmDist {
    pub tarball: String,
    pub shasum: Option<String>,
    pub integrity: Option<String>,
}

/*
    A human requires either a name or an email.
    The parser will fail without having at
    least one field filled out.
*/
#[derive(Debug)]
pub struct Human {
    pub name: Option<String>,
    pub email: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug)]
pub struct Repository {
    pub type_field: String,
    pub url: String,
}

impl NpmPackument {
    fn from_raw(raw: RawNpmPackument) -> Result<Self, String> {
        let name = raw.name.ok_or("Missing required field: name")?;

        if name.trim().is_empty() {
            return Err("Package name cannot be empty".into());
        }

        let dist_tags = raw.dist_tags.ok_or("Missing required field: dist-tags")?;

        let raw_versions = raw.versions.ok_or("Missing required field: versions")?;

        if raw_versions.is_empty() {
            return Err("Package has no versions".into());
        }

        let homepage = match raw.homepage {
            Some(url) => Some(validate_url(url, "homepage URL")?),
            None => None,
        };

        let mut versions = HashMap::new();

        for (key, version) in raw_versions {
            let version_name = version.name.ok_or("Missing required field: version.name")?;

            if version_name.trim().is_empty() {
                return Err("Version name cannot be empty".into());
            }

            let version_string = version
                .version
                .ok_or("Missing required field: version.version")?;

            if !is_valid_semver(&version_string) {
                return Err(format!("Invalid version: {}", version_string));
            }

            let dist = version.dist.ok_or("Missing required field: dist")?;

            let tarball = validate_url(
                dist.tarball.ok_or("Missing required field: dist.tarball")?,
                "tarball URL",
            )?;

            let npm_version = NpmVersion {
                id: version.id,
                node_version: version.node_version,
                npm_version: version.npm_version,
                npm_user: convert_person(version.npm_user)?,

                name: version_name,
                version: version_string,

                description: version.description,
                main: version.main,
                license: version.license,

                author: convert_person(version.author)?,

                contributors: convert_people(version.contributors.unwrap_or_default())?,

                maintainers: convert_people(version.maintainers.unwrap_or_default())?,

                repository: convert_repository(version.repository)?,

                dependencies: version.dependencies.unwrap_or_default(),

                accept_dependencies: version.accept_dependencies.unwrap_or_default(),

                dev_dependencies: version.dev_dependencies.unwrap_or_default(),

                peer_dependencies: version.peer_dependencies.unwrap_or_default(),

                peer_dependencies_meta: convert_peer_dependencies_meta(
                    version.peer_dependencies_meta.unwrap_or_default(),
                ),

                optional_dependencies: version.optional_dependencies.unwrap_or_default(),

                bundle_dependencies: version.bundle_dependencies.unwrap_or_default(),

                bin: convert_bin(version.bin),

                directories: version.directories.unwrap_or_default(),

                engines: version.engines.unwrap_or_default(),

                has_shrinkwrap: version.has_shrinkwrap,

                has_install_script: version.has_install_script,

                funding: convert_funding(version.funding)?,

                cpu: version.cpu.unwrap_or_default(),

                os: version.os.unwrap_or_default(),

                dist: NpmDist {
                    tarball,
                    shasum: dist.shasum,
                    integrity: dist.integrity,
                },

                deprecated: version.deprecated,

                extra: version.extra,
            };

            versions.insert(key, npm_version);
        }

        Ok(Self {
            id: raw.id,
            rev: raw.rev,

            name,

            dist_tags,

            time: raw.time.unwrap_or_default(),

            users: raw.users.unwrap_or_default(),

            versions,

            author: convert_person(raw.author)?,

            bugs: raw.bugs,

            contributors: convert_people(raw.contributors.unwrap_or_default())?,

            description: raw.description,

            homepage,

            keywords: raw.keywords.unwrap_or_default(),

            license: raw.license,

            maintainers: convert_people(raw.maintainers.unwrap_or_default())?,

            readme: raw.readme,

            readme_filename: raw.readme_filename,

            repository: convert_repository(raw.repository)?,

            extra: raw.extra,
        })
    }
}

fn validate_url(url: String, field: &str) -> Result<String, String> {
    Url::parse(&url).map_err(|_| format!("Invalid {field}: {url}"))?;
    Ok(url)
}

fn convert_funding(raw: Option<RawFunding>) -> Result<Option<Funding>, String> {
    match raw {
        Some(RawFunding::Url(url)) => Ok(Some(Funding {
            url: validate_url(url, "funding URL")?,
            type_field: None,
        })),

        Some(RawFunding::Object { url, type_field }) => Ok(Some(Funding {
            url: validate_url(url, "funding URL")?,
            type_field,
        })),

        None => Ok(None),
    }
}

fn convert_person(raw: Option<RawHuman>) -> Result<Option<Human>, String> {
    match raw {
        Some(RawHuman::String(name)) => Ok(Some(Human {
            name: Some(name),
            email: None,
            url: None,
        })),

        Some(RawHuman::Object { name, email, url }) => {
            if name.is_none() && email.is_none() {
                return Err("Human must have either a name or an email".into());
            }

            Ok(Some(Human { name, email, url }))
        }

        None => Ok(None),
    }
}

fn convert_people(raw: Vec<RawHuman>) -> Result<Vec<Human>, String> {
    raw.into_iter()
        .map(|person| match person {
            RawHuman::String(name) => Ok(Human {
                name: Some(name),
                email: None,
                url: None,
            }),

            RawHuman::Object { name, email, url } => {
                if name.is_none() && email.is_none() {
                    return Err("Human must have either a name or an email".into());
                }

                Ok(Human { name, email, url })
            }
        })
        .collect()
}

fn convert_repository(raw: Option<RawRepository>) -> Result<Option<Repository>, String> {
    match raw {
        Some(repo) => {
            let repo_type = repo.type_field.ok_or("Repository missing type")?;
            let url = validate_url(repo.url.ok_or("Repository missing url")?, "repository URL")?;

            Ok(Some(Repository {
                type_field: repo_type,
                url,
            }))
        }
        None => Ok(None),
    }
}

fn convert_bin(raw: Option<RawBin>) -> HashMap<String, String> {
    match raw {
        Some(RawBin::String(path)) => {
            let mut map = HashMap::new();
            map.insert("".into(), path);
            map
        }

        Some(RawBin::Map(map)) => map,

        None => HashMap::new(),
    }
}

fn convert_peer_dependencies_meta(
    raw: HashMap<String, RawPeerDependencyMeta>,
) -> HashMap<String, PeerDependencyMeta> {
    raw.into_iter()
        .map(|(name, meta)| {
            (
                name,
                PeerDependencyMeta {
                    optional: meta.optional.unwrap_or(false),
                },
            )
        })
        .collect()
}

fn is_valid_semver(version: &str) -> bool {
    semver::Version::parse(version).is_ok()
}

pub fn parse_packument<R: Read>(reader: R) -> Result<NpmPackument, Box<dyn Error>> {
    let raw: RawNpmPackument = serde_json::from_reader(reader)?;

    let validated = NpmPackument::from_raw(raw)?;

    Ok(validated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn load_fixture(name: &str) -> Result<NpmPackument, Box<dyn Error>> {
        let path = format!(
            "{}/testdata/npm/packument/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let file = File::open(&path)?;
        parse_packument(file)
    }

    #[test]
    fn parses_full_packument_fixture() {
        let package = load_fixture("full-packument.json").unwrap();

        assert_eq!(package.name, "example");
        assert!(package.versions.contains_key("1.0.0"));

        let version = package.versions.get("1.0.0").unwrap();

        assert_eq!(version.version, "1.0.0");
        assert_eq!(
            version.dependencies.get("serde"),
            Some(&"^1.0.0".to_string())
        );
    }

    #[test]
    fn parses_abbreviated_packument_fixture() {
        let package = load_fixture("abbreviated-packument.json").unwrap();

        assert_eq!(package.name, "minimal-package");
        assert!(package.versions.contains_key("2.0.0"));
    }

    #[test]
    fn abbreviated_packument_allows_missing_metadata() {
        let package = load_fixture("abbreviated-packument.json").unwrap();

        assert!(package.description.is_none());
        assert!(package.author.is_none());
        assert!(package.repository.is_none());

        let version = package.versions.get("2.0.0").unwrap();

        assert!(version.dependencies.is_empty());
        assert!(version.dev_dependencies.is_empty());
    }

    #[test]
    fn missing_optional_metadata_does_not_fail_parse() {
        let package = load_fixture("missing-optional-fields.json").unwrap();

        assert!(package.description.is_none());
        assert!(package.homepage.is_none());
        assert!(package.repository.is_none());
    }

    #[test]
    fn malformed_version_metadata_returns_error() {
        let file = File::open(format!(
            "{}/testdata/npm/packument/malformed-version.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();

        let result = parse_packument(file);

        assert!(result.is_err());

        let error = result.unwrap_err().to_string();

        assert!(
            error.contains("Invalid version"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn parses_scoped_package_names() -> Result<(), Box<dyn Error>> {
        let package = load_fixture("scoped-package.json")?;

        assert_eq!(package.name, "@scope/package");

        Ok(())
    }

    #[test]
    fn parses_express_full_packument() {
        let package = load_fixture("express.json").unwrap();

        assert_eq!(package.name, "express");

        assert!(
            !package.versions.is_empty(),
            "express should contain published versions"
        );

        let latest_version = package
            .dist_tags
            .get("latest")
            .expect("express should have a latest dist tag");

        assert!(
            package.versions.contains_key(latest_version),
            "latest version should exist in versions"
        );

        let version = package.versions.get(latest_version).unwrap();

        assert_eq!(version.name, "express");
        assert_eq!(version.version, *latest_version);

        assert!(
            !version.dist.tarball.is_empty(),
            "express should have a tarball URL"
        );

        assert!(
            version.dist.shasum.is_some(),
            "express should have a shasum"
        );
    }
}
