//! Reusable types for npm package and dependency identifiers.

use std::collections::HashMap;
use std::{fmt, str::FromStr};
use url::Url;

/// A reason that a current npm package name is invalid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NpmPackageNameError {
    Empty,
    TooLong,
    InvalidStructure,
    InvalidComponent,
    Reserved,
}

impl fmt::Display for NpmPackageNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("package name cannot be empty"),
            Self::TooLong => formatter.write_str("package name is too long"),
            Self::InvalidStructure => formatter.write_str("package name has an invalid structure"),
            Self::InvalidComponent => formatter.write_str("package name has an invalid component"),
            Self::Reserved => formatter.write_str("package name is reserved"),
        }
    }
}

impl std::error::Error for NpmPackageNameError {}

/// An error returned while parsing an npm dependency type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NpmTypeParseError {
    InvalidPackageName {
        value: Box<str>,
        reason: NpmPackageNameError,
    },
    EmptyDependencySpecification,
    InvalidNpmAlias,
    InvalidDependencySpecification(Box<str>),
}

impl fmt::Display for NpmTypeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPackageName { reason, .. } => reason.fmt(formatter),
            Self::EmptyDependencySpecification => {
                formatter.write_str("dependency specification cannot be empty")
            }
            Self::InvalidNpmAlias => formatter.write_str("invalid npm alias"),
            Self::InvalidDependencySpecification(_) => {
                formatter.write_str("invalid dependency specification")
            }
        }
    }
}

impl std::error::Error for NpmTypeParseError {}

/// An error returned while converting a dependency collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyCollectionParseError {
    pub name: String,
    pub source: NpmTypeParseError,
}

impl fmt::Display for DependencyCollectionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid dependency {}: {}",
            self.name, self.source
        )
    }
}

impl std::error::Error for DependencyCollectionParseError {}

/// A current npm package name.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NpmPackageName(String);

impl NpmPackageName {
    /// Parses a package name using npm's current naming contract.
    pub fn parse(value: String) -> Result<Self, NpmPackageNameError> {
        if value.is_empty() {
            return Err(NpmPackageNameError::Empty);
        }

        if value.len() > 214 {
            return Err(NpmPackageNameError::TooLong);
        }

        let mut parts = value.split('/');
        let first = parts.next().expect("split always produces a first part");
        let second = parts.next();

        if parts.next().is_some()
            || (value.starts_with('@') && second.is_none())
            || (!value.starts_with('@') && second.is_some())
        {
            return Err(NpmPackageNameError::InvalidStructure);
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
            return Err(NpmPackageNameError::InvalidComponent);
        }

        if matches!(value.as_str(), "node_modules" | "favicon.ico") {
            return Err(NpmPackageNameError::Reserved);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NpmPackageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for NpmPackageName {
    type Err = NpmPackageNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value.to_owned())
    }
}

/// A dependency name using either the current npm contract or its historic
/// case-sensitive predecessor.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DependencyPackageName {
    Modern(NpmPackageName),
    Historic(Box<str>),
}

impl DependencyPackageName {
    /// Parses a dependency package name, accepting npm's historic uppercase form.
    pub fn parse(value: String) -> Result<Self, NpmTypeParseError> {
        match NpmPackageName::parse(value.clone()) {
            Ok(name) => Ok(Self::Modern(name)),
            Err(_) if is_valid_historic_dependency_package_name(&value) => {
                Ok(Self::Historic(value.into_boxed_str()))
            }
            Err(reason) => Err(NpmTypeParseError::InvalidPackageName {
                value: value.into_boxed_str(),
                reason,
            }),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Modern(name) => name.as_str(),
            Self::Historic(name) => name,
        }
    }
}

/// A dependency specification accepted by npm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DependencySpec {
    Registry(NpmVersionRange),
    Tag(String),
    File(String),
    Git(String),
    Url(Url),
    NpmAlias {
        package: DependencyPackageName,
        specification: Box<Self>,
    },
}

/// An npm registry version range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpmVersionRange(String);

impl NpmVersionRange {
    /// Returns the range exactly as it appeared in the dependency declaration.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A map of validated npm dependency names and specifications.
pub type DependencyMap = HashMap<DependencyPackageName, DependencySpec>;

/// Metadata associated with a peer dependency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerDependencyMeta {
    pub optional: bool,
}

/// A map of peer dependency metadata keyed by validated dependency names.
pub type PeerDependencyMetaMap = HashMap<DependencyPackageName, PeerDependencyMeta>;

/// The validated package names bundled with an npm package.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BundleDependencies(Vec<DependencyPackageName>);

impl BundleDependencies {
    /// Parses the package names in an npm `bundleDependencies` list.
    pub fn parse(names: Vec<String>) -> Result<Self, DependencyCollectionParseError> {
        names
            .into_iter()
            .map(parse_dependency_name)
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    /// Creates a bundle list containing every declared dependency.
    pub fn all_dependencies(dependencies: &DependencyMap) -> Self {
        Self(dependencies.keys().cloned().collect())
    }

    pub fn as_slice(&self) -> &[DependencyPackageName] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Parses a raw dependency map into validated names and specifications.
pub fn parse_dependency_map(
    raw: HashMap<String, String>,
) -> Result<DependencyMap, DependencyCollectionParseError> {
    raw.into_iter()
        .map(|(name, specification)| {
            let parsed_name = parse_dependency_name(name)?;
            let parsed_specification = DependencySpec::parse(specification).map_err(|source| {
                DependencyCollectionParseError {
                    name: parsed_name.as_str().to_owned(),
                    source,
                }
            })?;
            Ok((parsed_name, parsed_specification))
        })
        .collect()
}

/// Parses raw peer dependency metadata keyed by npm dependency names.
pub fn parse_peer_dependency_meta_map(
    raw: HashMap<String, PeerDependencyMeta>,
) -> Result<PeerDependencyMetaMap, DependencyCollectionParseError> {
    raw.into_iter()
        .map(|(name, meta)| Ok((parse_dependency_name(name)?, meta)))
        .collect()
}

fn parse_dependency_name(
    name: String,
) -> Result<DependencyPackageName, DependencyCollectionParseError> {
    DependencyPackageName::parse(name.clone())
        .map_err(|source| DependencyCollectionParseError { name, source })
}

impl DependencySpec {
    /// Parses a dependency specification using npm's supported source forms.
    pub fn parse(value: String) -> Result<Self, NpmTypeParseError> {
        if value.is_empty() {
            return Err(NpmTypeParseError::EmptyDependencySpecification);
        }

        if let Some(alias) = value.strip_prefix("npm:") {
            let (package, specification) = alias
                .rsplit_once('@')
                .filter(|(package, specification)| !package.is_empty() && !specification.is_empty())
                .ok_or(NpmTypeParseError::InvalidNpmAlias)?;
            return Ok(Self::NpmAlias {
                package: DependencyPackageName::parse(package.to_owned())?,
                specification: Box::new(Self::parse(specification.to_owned())?),
            });
        }

        if value.starts_with("file:")
            || value.starts_with("link:")
            || value.starts_with("workspace:")
        {
            return Ok(Self::File(value));
        }

        if value.starts_with("git+") || value.starts_with("git://") || value.starts_with("github:")
        {
            return Ok(Self::Git(value));
        }

        if is_github_shorthand(&value) {
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
            return Err(NpmTypeParseError::InvalidDependencySpecification(
                value.into_boxed_str(),
            ));
        }

        Ok(Self::Tag(value))
    }
}

fn is_valid_package_name_component(component: &str) -> bool {
    !component.is_empty()
        && !component.starts_with(['.', '_'])
        && component.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

fn is_github_shorthand(value: &str) -> bool {
    let repository = value
        .split_once('#')
        .map_or(value, |(repository, _)| repository);
    let mut components = repository.split('/');
    let Some(owner) = components.next() else {
        return false;
    };
    let Some(name) = components.next() else {
        return false;
    };

    components.next().is_none()
        && !owner.is_empty()
        && !name.is_empty()
        && owner
            .bytes()
            .chain(name.bytes())
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_valid_historic_dependency_package_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 214 || !value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return false;
    }

    let mut parts = value.split('/');
    let first = parts
        .next()
        .expect("nonempty package has a first component");
    let second = parts.next();
    if parts.next().is_some()
        || (value.starts_with('@') && second.is_none())
        || (!value.starts_with('@') && second.is_some())
    {
        return false;
    }

    let valid_component = |component: &str| {
        !component.is_empty()
            && !component.starts_with(['.', '_'])
            && component
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };

    if value.starts_with('@') {
        second.is_some_and(|second| {
            valid_component(first.strip_prefix('@').unwrap_or(first)) && valid_component(second)
        })
    } else {
        valid_component(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_current_package_names() {
        for name in [
            "package",
            "package-name",
            "package_name",
            "package.name",
            "a1",
            "0package",
            "@scope/package",
            "@scope/package-name",
            "@scope_2/package.name",
            "@a/b",
            "@npm/cli",
            "http",
            "stream",
            "fs",
            "path",
        ] {
            let parsed = NpmPackageName::parse(name.to_owned()).expect("valid package name");
            assert_eq!(parsed.as_str(), name);
        }
    }

    #[test]
    fn exposes_standard_string_conversion_for_current_package_names() {
        let name: NpmPackageName = "package".parse().expect("valid package name");

        assert_eq!(name.to_string(), "package");
    }

    #[test]
    fn rejects_invalid_current_package_names() {
        for name in [
            "",
            " package",
            "package ",
            "package\tname",
            ".package",
            "_package",
            "UPPERCASE",
            "package/child",
            "package/child/grandchild",
            "package/",
            "/package",
            "@scope",
            "@",
            "@scope/",
            "@/package",
            "@scope/package/child",
            "@scope//package",
            "@@scope/package",
            "@scope/@package",
            "@scope/._package",
            "package~name",
            "package!name",
            "package*name",
            "package(name)",
            "package'name",
            "package?name",
            "package%name",
            "package:name",
            "package\\name",
            "café",
            "node_modules",
            "favicon.ico",
        ] {
            assert!(
                NpmPackageName::parse(name.to_owned()).is_err(),
                "{name:?} should be rejected"
            );
        }
    }

    #[test]
    fn enforces_the_npm_package_name_length_limit() {
        NpmPackageName::parse("a".repeat(214)).expect("214-character package name should be valid");
        assert_eq!(
            NpmPackageName::parse("a".repeat(215)),
            Err(NpmPackageNameError::TooLong)
        );
    }

    #[test]
    fn accepts_historic_names_only_for_dependencies() {
        assert_eq!(
            NpmPackageName::parse("Deferred".to_owned()),
            Err(NpmPackageNameError::InvalidComponent)
        );
        assert!(matches!(
            DependencyPackageName::parse("@scope/package".to_owned()),
            Ok(DependencyPackageName::Modern(_))
        ));
        assert!(matches!(
            DependencyPackageName::parse("Deferred".to_owned()),
            Ok(DependencyPackageName::Historic(_))
        ));
    }

    #[test]
    fn parses_dependency_specification_forms() {
        let cases = [
            ("^1.0.0", "registry range"),
            ("latest", "registry tag"),
            ("file:../package", "file path"),
            ("link:../package", "linked path"),
            ("workspace:^", "workspace range"),
            ("git+https://example.com/package.git", "git URL"),
            ("github:owner/package", "GitHub protocol shorthand"),
            ("owner/package#v1.0.0", "GitHub user/repository shorthand"),
            (
                "owner/package.name",
                "GitHub shorthand with a dotted repository name",
            ),
            ("https://example.com/package.tgz", "tarball URL"),
            ("npm:@scope/package@^1.0.0", "scoped npm alias"),
        ];

        for (value, description) in cases {
            assert!(
                DependencySpec::parse(value.to_owned()).is_ok(),
                "{description}: {value}"
            );
        }
    }

    #[test]
    fn converts_dependency_collections_to_shared_types() {
        let dependencies = parse_dependency_map(HashMap::from([
            ("react".to_owned(), "^19.0.0".to_owned()),
            ("legacy".to_owned(), "npm:Deferred@0.1.0".to_owned()),
        ]))
        .expect("valid dependency map");

        assert_eq!(dependencies.len(), 2);
        assert_eq!(
            dependencies.get(&DependencyPackageName::parse("react".to_owned()).unwrap()),
            Some(&DependencySpec::parse("^19.0.0".to_owned()).unwrap())
        );

        let peer_metadata = parse_peer_dependency_meta_map(HashMap::from([(
            "react".to_owned(),
            PeerDependencyMeta { optional: true },
        )]))
        .expect("valid peer dependency metadata");
        assert_eq!(peer_metadata.len(), 1);

        let bundled =
            BundleDependencies::parse(vec!["react".to_owned()]).expect("valid bundled dependency");
        assert_eq!(bundled.as_slice().len(), 1);
    }

    #[test]
    fn reports_the_failing_dependency_key() {
        let error =
            parse_dependency_map(HashMap::from([("@scope".to_owned(), "^1.0.0".to_owned())]))
                .expect_err("malformed dependency name should fail");

        assert_eq!(error.name, "@scope");
    }
}
