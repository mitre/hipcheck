//! Reusable types for npm package and dependency identifiers.

use std::fmt;
use url::Url;

/// An error returned while parsing an npm package or dependency type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NpmTypeParseError {
    EmptyPackageName,
    PackageNameTooLong,
    InvalidPackageName(Box<str>),
    EmptyDependencySpecification,
    InvalidNpmAlias,
    InvalidDependencySpecification(Box<str>),
}

impl fmt::Display for NpmTypeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPackageName => formatter.write_str("package name cannot be empty"),
            Self::PackageNameTooLong => formatter.write_str("package name is too long"),
            Self::InvalidPackageName(_) => formatter.write_str("invalid package name"),
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

/// A current npm package name.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NpmPackageName(String);

impl NpmPackageName {
    /// Parses a package name using npm's current naming contract.
    pub fn parse(value: String) -> Result<Self, NpmTypeParseError> {
        if value.is_empty() {
            return Err(NpmTypeParseError::EmptyPackageName);
        }

        if value.len() > 214 {
            return Err(NpmTypeParseError::PackageNameTooLong);
        }

        let mut parts = value.split('/');
        let first = parts.next().expect("split always produces a first part");
        let second = parts.next();

        if parts.next().is_some()
            || (value.starts_with('@') && second.is_none())
            || (!value.starts_with('@') && second.is_some())
        {
            return Err(NpmTypeParseError::InvalidPackageName(
                value.into_boxed_str(),
            ));
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
            return Err(NpmTypeParseError::InvalidPackageName(
                value.into_boxed_str(),
            ));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
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
            Err(error) => Err(error),
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
    fn parses_current_and_historic_dependency_names() {
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
        for value in [
            "^1.0.0",
            "latest",
            "file:../package",
            "git+https://example.com/package.git",
            "https://example.com/package.tgz",
            "npm:package@^1.0.0",
        ] {
            assert!(DependencySpec::parse(value.to_owned()).is_ok(), "{value}");
        }
    }
}
