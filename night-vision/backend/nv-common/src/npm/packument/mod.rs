use jiff::Timestamp;
use serde_json::Value;
use semver::Version;
use std::collections::HashMap;
use std::fmt;
use std::error::Error;
use std::io::Read;
use url::Url;
mod raw;
use raw::*;

#[derive(Debug)]
pub enum PackumentParseError {
    Json(serde_json::Error),
    MissingField(&'static str),
    EmptyPackageName,
    PackageNameTooLong,
    InvalidPackageName(Box<str>),
    EmptyDependencySpecification,
    InvalidNpmAlias,
    InvalidDependencySpecification(Box<str>),
    PackageHasNoVersions,
    VersionPackageNameMismatch { version_name: Box<str>, package_name: Box<str> },
    VersionKeyMismatch { key: Box<str>, version: Box<str> },
    MissingDistTagVersion { tag: Box<str>, version: Box<str> },
    InvalidUrl { field: Box<str>, value: Box<str> },
    InvalidSha1Digest(Box<str>),
    InvalidTimestamp { field: Box<str>, value: Box<str> },
    InvalidVersion { field: Box<str>, value: Box<str> },
    EmptyPublishedVersionKey,
    InvalidHuman,
    MissingRepositoryType,
}

impl fmt::Display for PackumentParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(_) => formatter.write_str("failed to parse packument JSON"),
            Self::MissingField(field) => write!(formatter, "packument is missing required field {field}"),
            Self::EmptyPackageName => formatter.write_str("package name cannot be empty"),
            Self::PackageNameTooLong => formatter.write_str("package name is too long"),
            Self::InvalidPackageName(_) => formatter.write_str("packument contains an invalid package name"),
            Self::EmptyDependencySpecification => formatter.write_str("dependency specification cannot be empty"),
            Self::InvalidNpmAlias => formatter.write_str("packument contains an invalid npm alias"),
            Self::InvalidDependencySpecification(_) => formatter.write_str("packument contains an invalid dependency specification"),
            Self::PackageHasNoVersions => formatter.write_str("packument has no versions"),
            Self::VersionPackageNameMismatch { .. } => formatter.write_str("version package name does not match packument name"),
            Self::VersionKeyMismatch { .. } => formatter.write_str("version key does not match version metadata"),
            Self::MissingDistTagVersion { .. } => formatter.write_str("dist tag refers to a missing version"),
            Self::InvalidUrl { field, .. } => write!(formatter, "packument contains an invalid {field}"),
            Self::InvalidSha1Digest(_) => formatter.write_str("packument contains an invalid SHA-1 digest"),
            Self::InvalidTimestamp { field, .. } => write!(formatter, "packument contains an invalid {field}"),
            Self::InvalidVersion { field, .. } => write!(formatter, "packument contains an invalid {field}"),
            Self::EmptyPublishedVersionKey => formatter.write_str("packument contains an empty published version key"),
            Self::InvalidHuman => formatter.write_str("packument contains an invalid human"),
            Self::MissingRepositoryType => formatter.write_str("repository is missing its type"),
        }
    }
}

impl Error for PackumentParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for PackumentParseError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<String> for PackumentParseError {
    fn from(value: String) -> Self {
        Self::InvalidVersion {
            field: "packument value".into(),
            value: value.into_boxed_str(),
        }
    }
}

impl From<&'static str> for PackumentParseError {
    fn from(value: &'static str) -> Self {
        Self::MissingField(value)
    }
}


#[derive(Debug)]
pub struct NpmPackument {
    pub id: Option<String>,
    pub rev: Option<String>,

    pub name: NpmPackageName,

    pub modified: Option<Timestamp>,

    pub dist_tags: HashMap<String, Version>,
    pub time: Option<PackumentTimes>,
    pub users: HashMap<String, bool>,

    pub versions: HashMap<Version, NpmVersion>,

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

    pub name: NpmPackageName,
    pub version: Version,

    pub description: Option<String>,
    pub main: Option<String>,
    pub license: Option<String>,

    pub author: Option<Human>,
    pub contributors: Vec<Human>,
    pub maintainers: Vec<Human>,
    pub repository: Option<Repository>,

    pub dependencies: HashMap<NpmPackageName, DependencySpec>,
    pub accept_dependencies: HashMap<NpmPackageName, DependencySpec>,
    pub dev_dependencies: HashMap<NpmPackageName, DependencySpec>,
    pub peer_dependencies: HashMap<NpmPackageName, DependencySpec>,
    pub peer_dependencies_meta: HashMap<NpmPackageName, PeerDependencyMeta>,
    pub optional_dependencies: HashMap<NpmPackageName, DependencySpec>,
    pub bundle_dependencies: Vec<NpmPackageName>,

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
pub struct PackumentTimes {
    pub created: Timestamp,
    pub modified: Timestamp,
    pub versions: HashMap<PublishedVersionKey, Timestamp>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PublishedVersionKey(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NpmPackageName(String);

impl NpmPackageName {
    fn parse(value: String) -> Result<Self, PackumentParseError> {
        if value.is_empty() || value.len() > 214 {
            return Err("Package name must be between 1 and 214 characters".into());
        }

        let mut parts = value.split('/');
        let first = parts.next().expect("split always produces a first part");
        let second = parts.next();

        if parts.next().is_some()
            || (value.starts_with('@') && second.is_none())
            || (!value.starts_with('@') && second.is_some())
        {
            return Err(PackumentParseError::InvalidPackageName(value.into_boxed_str()));
        }

        let valid = if value.starts_with('@') {
            second.is_some_and(|second| {
                is_valid_package_name_component(first.strip_prefix('@').unwrap_or(first))
                    && is_valid_package_name_component(second)
            })
        } else {
            second.is_none() && is_valid_package_name_component(first)
        };

        if !valid {
            return Err(PackumentParseError::InvalidPackageName(value.into_boxed_str()));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DependencySpec {
    Registry(NpmVersionRange),
    Tag(String),
    File(String),
    Git(String),
    Url(Url),
    NpmAlias {
        package: NpmPackageName,
        specification: Box<DependencySpec>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpmVersionRange(String);

impl DependencySpec {
    fn parse(value: String) -> Result<Self, PackumentParseError> {
        if value.is_empty() {
            return Err("Dependency specification cannot be empty".into());
        }

        if let Some(alias) = value.strip_prefix("npm:") {
            let (package, specification) = alias
                .rsplit_once('@')
                .filter(|(package, specification)| !package.is_empty() && !specification.is_empty())
                .ok_or("Invalid npm alias dependency specification")?;
            return Ok(Self::NpmAlias {
                package: NpmPackageName::parse(package.to_owned())?,
                specification: Box::new(Self::parse(specification.to_owned())?),
            });
        }

        if value.starts_with("file:") || value.starts_with("link:") || value.starts_with("workspace:") {
            return Ok(Self::File(value));
        }

        if value.starts_with("git+") || value.starts_with("git://") || value.starts_with("github:") {
            return Ok(Self::Git(value));
        }

        if let Ok(url) = Url::parse(&value) {
            return Ok(Self::Url(url));
        }

        if value.starts_with(['=', '~', '^', '>', '<', '*', 'v'])
            || value
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
        {
            return Ok(Self::Registry(NpmVersionRange(value)));
        }

        if value.chars().any(char::is_whitespace) {
            return Err(PackumentParseError::InvalidDependencySpecification(value.into_boxed_str()));
        }

        Ok(Self::Tag(value))
    }
}

fn is_valid_package_name_component(component: &str) -> bool {
    !component.is_empty()
        && !component.starts_with(['.', '_'])
        && component.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.')
        })
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
    pub tarball: Url,
    pub shasum: Sha1Digest,
    pub integrity: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sha1Digest(String);

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
    fn from_raw(raw: RawNpmPackument) -> Result<Self, PackumentParseError> {
        let name = NpmPackageName::parse(raw.name.ok_or("Missing required field: name")?)?;

        let raw_dist_tags = raw.dist_tags.ok_or("Missing required field: dist-tags")?;

        let raw_versions = raw.versions.ok_or("Missing required field: versions")?;

        if raw_versions.is_empty() {
            return Err("Package has no versions".into());
        }

        let homepage = match raw.homepage {
            Some(url) => Some(validate_url(url, "homepage URL")?),
            None => None,
        };

        let mut versions = HashMap::new();

        for (key, raw_version) in raw_versions {
            let version_name =
                NpmPackageName::parse(raw_version.name.ok_or("Missing required field: version.name")?)?;

            if version_name != name {
                return Err(PackumentParseError::VersionPackageNameMismatch {
                    version_name: version_name.as_str().into(),
                    package_name: name.as_str().into(),
                });
            }

            let version_key = parse_version(key, "version key")?;
            let version = parse_version(
                raw_version
                    .version
                    .ok_or("Missing required field: version.version")?,
                "version.version",
            )?;

            if version_key != version {
                return Err(PackumentParseError::VersionKeyMismatch {
                    key: version_key.to_string().into_boxed_str(),
                    version: version.to_string().into_boxed_str(),
                });
            }

            let dist = raw_version.dist.ok_or("Missing required field: dist")?;

            let tarball = parse_url(
                dist.tarball.ok_or("Missing required field: dist.tarball")?,
                "tarball URL",
            )?;

            let npm_version = NpmVersion {
                id: raw_version.id,
                node_version: raw_version.node_version,
                npm_version: raw_version.npm_version,
                npm_user: convert_person(raw_version.npm_user)?,

                name: version_name,
                version: version,

                description: raw_version.description,
                main: raw_version.main,
                license: raw_version.license,

                author: convert_person(raw_version.author)?,

                contributors: convert_people(raw_version.contributors.unwrap_or_default())?,

                maintainers: convert_people(raw_version.maintainers.unwrap_or_default())?,

                repository: convert_repository(raw_version.repository)?,

                dependencies: convert_dependency_map(raw_version.dependencies.unwrap_or_default())?,

                accept_dependencies: convert_dependency_map(
                    raw_version.accept_dependencies.unwrap_or_default(),
                )?,

                dev_dependencies: convert_dependency_map(raw_version.dev_dependencies.unwrap_or_default())?,

                peer_dependencies: convert_dependency_map(raw_version.peer_dependencies.unwrap_or_default())?,

                peer_dependencies_meta: convert_peer_dependencies_meta(
                    raw_version.peer_dependencies_meta.unwrap_or_default(),
                )?,

                optional_dependencies: convert_dependency_map(
                    raw_version.optional_dependencies.unwrap_or_default(),
                )?,

                bundle_dependencies: raw_version
                    .bundle_dependencies
                    .unwrap_or_default()
                    .into_iter()
                    .map(NpmPackageName::parse)
                    .collect::<Result<_, _>>()?,

                bin: convert_bin(raw_version.bin),

                directories: raw_version.directories.unwrap_or_default(),

                engines: raw_version.engines.unwrap_or_default(),

                has_shrinkwrap: raw_version.has_shrinkwrap,

                has_install_script: raw_version.has_install_script,

                funding: convert_funding(raw_version.funding)?,

                cpu: raw_version.cpu.unwrap_or_default(),

                os: raw_version.os.unwrap_or_default(),

                dist: NpmDist {
                    tarball,
                    shasum: parse_sha1_digest(
                        dist.shasum.ok_or("Missing required field: dist.shasum")?,
                    )?,
                    integrity: dist.integrity,
                },

                deprecated: raw_version.deprecated,

                extra: raw_version.extra,
            };

            versions.insert(version_key, npm_version);
        }

        let mut dist_tags = HashMap::new();

        for (tag, version) in raw_dist_tags {
            let version = parse_version(version, "dist-tag version")?;

            if !versions.contains_key(&version) {
                return Err(PackumentParseError::MissingDistTagVersion { tag: tag.into_boxed_str(), version: version.to_string().into_boxed_str() });
            }

            dist_tags.insert(tag, version);
        }

        Ok(Self {
            id: raw.id,
            rev: raw.rev,

            name,

            modified: raw
                .modified
                .map(|value| parse_timestamp(value, "modified"))
                .transpose()?,

            dist_tags,

            time: convert_packument_times(raw.time)?,

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

fn validate_url(url: String, field: &str) -> Result<String, PackumentParseError> {
    parse_url(url, field).map(|url| url.to_string())
}

fn parse_url(url: String, field: &str) -> Result<Url, PackumentParseError> {
    Url::parse(&url)
        .map_err(|_| PackumentParseError::InvalidUrl { field: field.into(), value: url.into_boxed_str() })
}

fn parse_sha1_digest(value: String) -> Result<Sha1Digest, PackumentParseError> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PackumentParseError::InvalidSha1Digest(value.into_boxed_str()));
    }

    Ok(Sha1Digest(value))
}

fn convert_funding(raw: Option<RawFunding>) -> Result<Option<Funding>, PackumentParseError> {
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

fn convert_person(raw: Option<RawHuman>) -> Result<Option<Human>, PackumentParseError> {
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

fn convert_people(raw: Vec<RawHuman>) -> Result<Vec<Human>, PackumentParseError> {
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

fn convert_repository(raw: Option<RawRepository>) -> Result<Option<Repository>, PackumentParseError> {
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
            map.insert(String::new(), path);
            map
        }

        Some(RawBin::Map(map)) => map,

        None => HashMap::new(),
    }
}

fn convert_peer_dependencies_meta(
    raw: HashMap<String, RawPeerDependencyMeta>,
) -> Result<HashMap<NpmPackageName, PeerDependencyMeta>, PackumentParseError> {
    raw.into_iter()
        .map(|(name, meta)| {
            Ok((
                NpmPackageName::parse(name)?,
                PeerDependencyMeta {
                    optional: meta.optional.unwrap_or(false),
                },
            ))
        })
        .collect()
}

fn convert_dependency_map(
    raw: HashMap<String, String>,
) -> Result<HashMap<NpmPackageName, DependencySpec>, PackumentParseError> {
    raw.into_iter()
        .map(|(name, specification)| {
            Ok((
                NpmPackageName::parse(name)?,
                DependencySpec::parse(specification)?,
            ))
        })
        .collect()
}

fn convert_packument_times(
    raw: Option<HashMap<String, String>>,
) -> Result<Option<PackumentTimes>, PackumentParseError> {
    let Some(mut raw) = raw else {
        return Ok(None);
    };

    let created = parse_timestamp(
        raw.remove("created")
            .ok_or("Packument time is missing created")?,
        "time.created",
    )?;
    let modified = parse_timestamp(
        raw.remove("modified")
            .ok_or("Packument time is missing modified")?,
        "time.modified",
    )?;
    let versions = raw
        .into_iter()
        .map(|(version, timestamp)| {
            Ok((
                parse_published_version_key(version)?,
                parse_timestamp(timestamp, "time version timestamp")?,
            ))
        })
        .collect::<Result<HashMap<_, _>, PackumentParseError>>()?;

    Ok(Some(PackumentTimes {
        created,
        modified,
        versions,
    }))
}

fn parse_timestamp(value: String, field: &str) -> Result<Timestamp, PackumentParseError> {
    value
        .parse()
        .map_err(|_| PackumentParseError::InvalidTimestamp { field: field.into(), value: value.into_boxed_str() })
}

fn parse_published_version_key(value: String) -> Result<PublishedVersionKey, PackumentParseError> {
    if value.is_empty() {
        return Err(PackumentParseError::EmptyPublishedVersionKey);
    }

    Ok(PublishedVersionKey(value))
}

fn parse_version(value: String, field: &str) -> Result<Version, PackumentParseError> {
    Version::parse(&value)
        .map_err(|_| PackumentParseError::InvalidVersion { field: field.into(), value: value.into_boxed_str() })
}

pub fn parse_packument<R: Read>(reader: R) -> Result<NpmPackument, PackumentParseError> {
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
        Ok(parse_packument(file)?)
    }

    fn semantic_version(value: &str) -> Version {
        Version::parse(value).expect("test version should parse")
    }

    #[test]
    fn parses_full_packument_fixture() {
        let package = load_fixture("full-packument.json").unwrap();

        assert_eq!(package.name.as_str(), "example");
        assert!(package.versions.contains_key(&semantic_version("1.0.0")));

        let version = package.versions.get(&semantic_version("1.0.0")).unwrap();

        assert_eq!(version.version, semantic_version("1.0.0"));
        assert_eq!(
            version.dependencies.get(&NpmPackageName::parse("serde".to_owned()).unwrap()),
            Some(&DependencySpec::Registry(NpmVersionRange("^1.0.0".to_owned())))
        );
    }

    #[test]
    fn parses_abbreviated_packument_fixture() {
        let package = load_fixture("abbreviated-packument.json").unwrap();

        assert_eq!(package.name.as_str(), "minimal-package");
        assert!(package.versions.contains_key(&semantic_version("2.0.0")));
    }

    #[test]
    fn abbreviated_packument_allows_missing_metadata() {
        let package = load_fixture("abbreviated-packument.json").unwrap();

        assert!(package.description.is_none());
        assert!(package.author.is_none());
        assert!(package.repository.is_none());

        let version = package.versions.get(&semantic_version("2.0.0")).unwrap();

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

        assert!(matches!(
            result.unwrap_err(),
            PackumentParseError::InvalidVersion { .. }
        ));
    }

    #[test]
    fn parses_scoped_package_names() -> Result<(), Box<dyn Error>> {
        let package = load_fixture("scoped-package.json")?;

        assert_eq!(package.name.as_str(), "@scope/package");

        Ok(())
    }

    #[test]
    fn rejects_invalid_package_names() {
        for package_name in ["", "UPPERCASE", "@scope", "@/package", "two/slashes/here"] {
            let result = NpmPackageName::parse(package_name.to_owned());
            assert!(result.is_err(), "{package_name} should be rejected");
        }
    }

    #[test]
    fn parses_express_full_packument() {
        let package = load_fixture("express.json").unwrap();

        assert_eq!(package.name.as_str(), "express");

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

        assert_eq!(version.name.as_str(), "express");
        assert_eq!(version.version, *latest_version);

        assert!(
            !version.dist.tarball.as_str().is_empty(),
            "express should have a tarball URL"
        );

        assert!(
            !version.dist.shasum.0.is_empty(),
            "express should have a shasum"
        );
    }
}
