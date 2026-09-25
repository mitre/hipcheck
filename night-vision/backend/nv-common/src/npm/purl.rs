//! Parsing for concrete npm Package URLs.

use super::types::NpmPackageName;
use percent_encoding::percent_decode_str;
use std::{fmt, str::FromStr as _};

/// A validated concrete npm package release PURL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpmPackagePurl {
    pub name: NpmPackageName,
    pub version: String,
}

impl NpmPackagePurl {
    pub fn parse(value: &str) -> Result<Self, NpmPackagePurlError> {
        let package_and_version = value
            .strip_prefix("pkg:npm/")
            .ok_or(NpmPackagePurlError::WrongType)?;
        if package_and_version.contains(['?', '#']) {
            return Err(NpmPackagePurlError::QualifiersOrSubpath);
        }
        let (encoded_name, encoded_version) = package_and_version
            .rsplit_once('@')
            .ok_or(NpmPackagePurlError::MissingVersion)?;
        let name = decode(encoded_name, "package name")?;
        let name =
            NpmPackageName::from_str(&name).map_err(|_| NpmPackagePurlError::InvalidPackageName)?;
        let version = decode(encoded_version, "version")?;
        if version.is_empty() || version.contains(['/', '@']) {
            return Err(NpmPackagePurlError::InvalidVersion);
        }
        Ok(Self { name, version })
    }
}

fn decode(value: &str, component: &'static str) -> Result<String, NpmPackagePurlError> {
    if !valid_percent_encoding(value) {
        return Err(NpmPackagePurlError::InvalidPercentEncoding(component));
    }
    percent_decode_str(value)
        .decode_utf8()
        .map(std::borrow::Cow::into_owned)
        .map_err(|_| NpmPackagePurlError::InvalidUtf8(component))
}

fn valid_percent_encoding(value: &str) -> bool {
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let (Some(first), Some(second)) = (bytes.next(), bytes.next()) else {
                return false;
            };
            if !first.is_ascii_hexdigit() || !second.is_ascii_hexdigit() {
                return false;
            }
        }
    }
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NpmPackagePurlError {
    WrongType,
    QualifiersOrSubpath,
    MissingVersion,
    InvalidPackageName,
    InvalidVersion,
    InvalidPercentEncoding(&'static str),
    InvalidUtf8(&'static str),
}

impl fmt::Display for NpmPackagePurlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongType => write!(f, "PURL must use the pkg:npm type"),
            Self::QualifiersOrSubpath => write!(f, "PURL must not include qualifiers or a subpath"),
            Self::MissingVersion => write!(f, "PURL must include a package version"),
            Self::InvalidPackageName => write!(f, "PURL has an invalid npm package name"),
            Self::InvalidVersion => write!(f, "PURL must include one non-empty package version"),
            Self::InvalidPercentEncoding(component) => {
                write!(f, "PURL {component} has invalid percent encoding")
            }
            Self::InvalidUtf8(component) => write!(f, "PURL {component} is not valid UTF-8"),
        }
    }
}

impl std::error::Error for NpmPackagePurlError {}

#[cfg(test)]
mod tests {
    use super::NpmPackagePurl;

    #[test]
    fn parses_scoped_package_purls() {
        let parsed = NpmPackagePurl::parse("pkg:npm/%40types/node@22.1.0").expect("PURL parses");
        assert_eq!(parsed.name.as_str(), "@types/node");
        assert_eq!(parsed.version, "22.1.0");
    }
}
