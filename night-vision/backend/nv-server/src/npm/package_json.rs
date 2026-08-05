use node_semver::Version;
use serde::Deserialize;
use serde_json;
use serde_json::Value;
use std::{collections::HashMap, io::Read, str::FromStr};
use thiserror::Error;

use super::package_name::NpmPackageName;

#[derive(Debug, Error)]
pub enum PackageParseError {
    #[error("File system I/O error")]
    Io(#[from] std::io::Error),

    #[error("Invalid JSON")]
    InvalidJson(#[from] serde_json::Error),

    #[error("Invalid package name '{name}': {reason}")]
    InvalidName { name: String, reason: &'static str },

    #[error("The 'type' field cannot be empty")]
    EmptyType,

    #[error("Invalid package version '{version}': {details}")]
    InvalidPackageVersion { version: String, details: String },
}

/// structure of raw, unvalidated package.json file.
#[derive(Debug, Deserialize)]
struct RawNpmPackageJson {
    name: Option<String>,

    #[serde(default)]
    private: bool,

    #[serde(default)]
    version: Option<String>,

    #[serde(rename = "type")]
    type_field: Option<String>,

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
    dev_dependencies: HashMap<String, String>,

    #[serde(rename = "peerDependencies")]
    #[serde(default)]
    peer_dependencies: HashMap<String, String>,

    #[serde(rename = "peerDependenciesMeta")]
    #[serde(default)]
    peer_dependencies_meta: Value,

    #[serde(rename = "optionalDependencies")]
    #[serde(default)]
    optional_dependencies: HashMap<String, String>,

    #[serde(rename = "bundleDependencies")]
    #[serde(default)]
    bundle_dependencies: Value,

    #[serde(default)]
    overrides: Value,

    #[serde(default)]
    workspaces: Value,
}

/// structure of safe, validated package.json file.
#[derive(Debug)]
pub struct NpmPackageJson {
    pub name: Option<NpmPackageName>,
    pub private: bool,
    pub version: Option<Version>,
    pub type_field: String,
    pub engines: HashMap<String, String>,
    pub description: String,
    pub scripts: HashMap<String, String>,
    pub dependencies: HashMap<String, String>,
    pub dev_dependencies: HashMap<String, String>,
    pub peer_dependencies: HashMap<String, String>,
    pub peer_dependencies_meta: Value,
    pub optional_dependencies: HashMap<String, String>,
    pub bundle_dependencies: Value,
    pub overrides: Value,
    pub workspaces: Value,
}

impl NpmPackageJson {
    /// Validate and convert from RawNpmPackageJson.
    /// Returns an error if validation fails.
    fn from_raw(raw: RawNpmPackageJson) -> Result<Self, PackageParseError> {
        let name = raw
            .name
            .map(|name| {
                NpmPackageName::from_str(&name)
                    .map_err(|reason| PackageParseError::InvalidName { name, reason })
            })
            .transpose()?;

        let type_field = match raw.type_field {
            Some(type_field) => {
                let type_field = type_field.trim().to_string();
                if type_field.is_empty() {
                    return Err(PackageParseError::EmptyType);
                }
                type_field
            }
            None => "commonjs".to_owned(),
        };

        let version = match raw.version {
            Some(v) => {
                let trimmed = v.trim();
                let parsed = Version::parse(trimmed).map_err(|e| {
                    PackageParseError::InvalidPackageVersion {
                        version: trimmed.to_string(),
                        details: e.to_string(),
                    }
                })?;
                Some(parsed)
            }
            None => None,
        };

        let engines = sanitize_map(raw.engines);
        let scripts = sanitize_map(raw.scripts);
        let dependencies = sanitize_map(raw.dependencies);
        let dev_dependencies = sanitize_map(raw.dev_dependencies);
        let peer_dependencies = sanitize_map(raw.peer_dependencies);
        let optional_dependencies = sanitize_map(raw.optional_dependencies);

        Ok(Self {
            name,
            private: raw.private,
            version,
            type_field,
            description: raw.description,
            engines,
            scripts,
            dependencies,
            dev_dependencies,
            peer_dependencies,
            peer_dependencies_meta: raw.peer_dependencies_meta,
            optional_dependencies,
            bundle_dependencies: raw.bundle_dependencies,
            overrides: raw.overrides,
            workspaces: raw.workspaces,
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
            },
            "peerDependencies": {
                "react": " ^19.0.0 "
            },
            "peerDependenciesMeta": {
                "react": { "optional": true }
            },
            "optionalDependencies": {
                "fsevents": " ~2.3.3 "
            },
            "bundleDependencies": ["react"],
            "overrides": {
                "react": "19.0.0"
            },
            "workspaces": {
                "packages": ["packages/*"]
            }
        }"#;

        // Cursor implements Read, acting exactly like an in-memory file
        let stream = Cursor::new(json_payload);
        let parsed = NpmPackageJson::parse_package_json(stream)?;

        assert_eq!(parsed.name.as_ref().unwrap().as_str(), "my-safe-package");
        assert_eq!(parsed.type_field, "module");
        // Verify whitespace sanitizer trimmed our map values
        assert_eq!(parsed.dev_dependencies.get("typescript").unwrap(), "5.0.0");
        assert_eq!(parsed.peer_dependencies.get("react").unwrap(), "^19.0.0");
        assert_eq!(
            parsed.optional_dependencies.get("fsevents").unwrap(),
            "~2.3.3"
        );
        assert_eq!(parsed.peer_dependencies_meta["react"]["optional"], true);
        assert_eq!(parsed.bundle_dependencies, serde_json::json!(["react"]));
        assert_eq!(parsed.overrides["react"], "19.0.0");
        assert_eq!(
            parsed.workspaces["packages"],
            serde_json::json!(["packages/*"])
        );

        Ok(())
    }

    #[test]
    fn parses_dependency_only_package_json() -> Result<(), PackageParseError> {
        let json_payload = r#"{
            "dependencies": {
                "react": " ^19.0.0 "
            }
        }"#;

        let parsed = NpmPackageJson::parse_package_json(Cursor::new(json_payload))?;

        assert!(parsed.name.is_none());
        assert!(parsed.version.is_none());
        assert_eq!(parsed.type_field, "commonjs");
        assert_eq!(parsed.dependencies.get("react").unwrap(), "^19.0.0");

        Ok(())
    }

    #[test]
    fn rejects_an_explicitly_empty_type_field() {
        let json_payload = r#"{
            "type": "   "
        }"#;

        let error = NpmPackageJson::parse_package_json(Cursor::new(json_payload)).unwrap_err();

        assert!(matches!(error, PackageParseError::EmptyType));
    }
}
