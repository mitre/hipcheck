use serde::Deserialize;
use serde_json;
use std::{collections::HashMap, fs::File, path::Path, io::{self,Read}, str::FromStr};
use std::error::Error;
use node_semver::Version;
use thiserror::Error;

use super::package_name::NpmPackageName;

#[derive(Debug, Error)]
pub enum PackageParseError {
    #[error("File system I/O error")]
    Io(#[from] std::io::Error),

    #[error("Invalid JSON")]
    InvalidJson(#[from] serde_json::Error),

    #[error("Invalid package name '{name}': {reason}")]
    InvalidName {
        name: String,
        reason: &'static str,
    },

    #[error("The 'type' field cannot be empty")]
    EmptyType,

    #[error("Missing required field: 'version'")]
    MissingVersion,

    #[error("Invalid package version '{version}': {details}")]
    InvalidPackageVersion {
        version: String,
        details: String,
    },
}

/// structure of raw, unvalidated package.json file.
#[derive(Debug, Deserialize)]
struct RawNpmPackageJson {
    name: String,

    #[serde(default)]
    private: bool,

    #[serde(default)]
    version: Option<String>,

    #[serde(default, rename = "type")]
    type_field: String,

    #[serde(default)]
    engines: HashMap<String, String>,

    #[serde(default)]
    description: String,

    #[serde(default)]
    scripts: HashMap<String, String>,

    #[serde(default)]
    dependencies: HashMap<String, String>,

    #[serde(rename = "devDependencies")]
    #[serde(default)]
    devDependencies: HashMap<String, String>,
}

/// structure of safe, validated package.json file.
#[derive(Debug)]
pub struct NpmPackageJson {
    pub name: NpmPackageName,
    pub private: bool,
    pub version: Option<Version>, 
    pub type_field: String,
    pub engines: HashMap<String, String>,
    pub description: String,
    pub scripts: HashMap<String, String>,
    pub dependencies: HashMap<String, String>,
    pub devDependencies: HashMap<String, String>, 
}

impl NpmPackageJson {
    /// Validate and convert from RawNpmPackageJson.
    /// Returns an error if validation fails.
    pub fn from_raw(raw: RawNpmPackageJson) -> Result<Self, PackageParseError> {
        // 1. Validate package name
        let name = NpmPackageName::from_str(&raw.name).map_err(|reason| {
            PackageParseError::InvalidName {
                name: raw.name.clone(),
                reason,
            }
        })?;
       
        // 2. Validate type field string (Since serde defaults it to "", we check if empty)
        let trimmed_type = raw.type_field.trim().to_string();
        if trimmed_type.is_empty() {
            return Err(PackageParseError::EmptyType);
        }

        // 3. Conditional version evaluation
        let version = match raw.version {
            Some(v) => {
                let trimmed = v.trim();
                let parsed = Version::parse(trimmed).map_err(|e| PackageParseError::InvalidPackageVersion {
                    version: trimmed.to_string(),
                    details: e.to_string(),
                })?;
                Some(parsed)
            }
            None => {
                if raw.private {
                    None
                } else {
                    return Err(PackageParseError::MissingVersion);
                }
            }
        };

        // 4. Sanitize map inputs
        let engines = sanitize_map(raw.engines);
        let scripts = sanitize_map(raw.scripts);
        let dependencies = sanitize_map(raw.dependencies);
        let devDependencies = sanitize_map(raw.devDependencies);

        Ok(Self {
            name,
            private: raw.private,
            version,
            type_field: trimmed_type,
            description: raw.description,
            engines,
            scripts,
            dependencies,
            devDependencies
        })
    }

    /// Reads, parses, and validates a package.json file into a NpmPackageJson struct.
   pub fn parse_package_json<R: Read>(reader: R) -> Result<Self, PackageParseError> {
        let raw: RawNpmPackageJson = serde_json::from_reader(reader)?;
        let validated = Self::from_raw(raw)?; // Executes through your preferred from_raw constructor
        
        Ok(validated)
    }
}

impl TryFrom<RawNpmPackageJson> for NpmPackageJson {
    type Error = PackageParseError;

    fn try_from(raw: RawNpmPackageJson) -> Result<Self, Self::Error> {
        Self::from_raw(raw)
    }
}


/// Helper function to trim internal values in raw maps cleanly
fn sanitize_map(map: HashMap<String, String>) -> HashMap<String, String> {
    map.into_iter()
        .map(|(k, v)| (k, v.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_package_json_parser() -> Result<(), PackageParseError> {
        let json_payload = r#"{
            "name": "my-safe-package",
            "private": false,
            "version": "1.0.0",
            "type": "module",
            "devDependencies": {
                "typescript": " 5.0.0 "
            }
        }"#;

        // Cursor implements Read, acting exactly like an in-memory file
        let stream = Cursor::new(json_payload);
        let parsed = NpmPackageJson::parse_package_json(stream)?;

        assert_eq!(parsed.name.as_str(), "my-safe-package");
        assert_eq!(parsed.type_field, "module");
        // Verify whitespace sanitizer trimmed our map values
        assert_eq!(parsed.devDependencies.get("typescript").unwrap(), "5.0.0");

        Ok(())
    }
}