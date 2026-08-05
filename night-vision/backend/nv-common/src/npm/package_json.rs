use node_semver::Version;
use serde::Deserialize;
use serde_json;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    io::Read,
};
use thiserror::Error;

use super::types::{
    BundleDependencies, DependencyCollectionParseError, DependencyMap, DependencySpec,
    NpmPackageName, NpmPackageNameError, NpmTypeParseError, PeerDependencyMeta,
    PeerDependencyMetaMap, parse_dependency_map, parse_peer_dependency_meta_map,
};

#[derive(Debug, Error)]
pub enum PackageParseError {
    #[error("File system I/O error")]
    Io(#[from] std::io::Error),

    #[error("Invalid JSON")]
    InvalidJson(#[from] serde_json::Error),

    #[error("Invalid package name '{name}': {reason}")]
    InvalidName {
        name: String,
        reason: NpmPackageNameError,
    },

    #[error("The 'type' field cannot be empty")]
    EmptyType,

    #[error("Invalid package type '{type_field}'; expected 'commonjs' or 'module'")]
    InvalidModuleKind { type_field: String },

    #[error("Invalid package version '{version}': {details}")]
    InvalidPackageVersion { version: String, details: String },

    #[error("Invalid dependency in '{section}' for '{name}': {source}")]
    InvalidDependency {
        section: &'static str,
        name: String,
        source: NpmTypeParseError,
    },

    #[error("Invalid override at '{path}': {details}")]
    InvalidOverride { path: String, details: String },
}

/// Structure of a raw, unvalidated `package.json` file.
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
    peer_dependencies_meta: HashMap<String, RawPeerDependencyMeta>,

    #[serde(rename = "optionalDependencies")]
    #[serde(default)]
    optional_dependencies: HashMap<String, String>,

    #[serde(rename = "bundleDependencies")]
    bundle_dependencies: Option<RawBundleDependencies>,

    #[serde(default)]
    overrides: Value,

    workspaces: Option<RawWorkspaces>,
}

#[derive(Debug, Deserialize)]
struct RawPeerDependencyMeta {
    optional: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBundleDependencies {
    List(Vec<String>),
    LegacyBoolean(bool),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawWorkspaces {
    Packages(Vec<String>),
    Configuration {
        packages: Vec<String>,
        #[serde(default)]
        nohoist: Vec<String>,
    },
}

/// The module system selected by a package's `type` field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleKind {
    CommonJs,
    Module,
}

/// Typed npm overrides declared by a package manifest.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct Overrides(BTreeMap<String, Override>);

/// A dependency override or a nested override scope.
#[derive(Debug, Eq, PartialEq)]
pub enum Override {
    Specification(DependencySpec),
    Nested(Overrides),
}

impl Overrides {
    pub fn entries(&self) -> &BTreeMap<String, Override> {
        &self.0
    }
}

/// npm workspace declarations in either supported manifest form.
#[derive(Debug, Eq, PartialEq)]
pub enum Workspaces {
    Packages(Vec<String>),
    Configuration {
        packages: Vec<String>,
        nohoist: Vec<String>,
    },
}

impl Default for Workspaces {
    fn default() -> Self {
        Self::Packages(Vec::new())
    }
}

/// Structure of a safe, validated `package.json` file.
#[derive(Debug)]
pub struct NpmPackageJson {
    pub name: Option<NpmPackageName>,
    pub private: bool,
    pub version: Option<Version>,
    pub module_kind: ModuleKind,
    pub engines: HashMap<String, String>,
    pub description: String,
    pub scripts: HashMap<String, String>,
    pub dependencies: DependencyMap,
    pub dev_dependencies: DependencyMap,
    pub peer_dependencies: DependencyMap,
    pub peer_dependencies_meta: PeerDependencyMetaMap,
    pub optional_dependencies: DependencyMap,
    pub bundle_dependencies: BundleDependencies,
    pub overrides: Overrides,
    pub workspaces: Workspaces,
}

impl NpmPackageJson {
    /// Validate and convert from RawNpmPackageJson.
    /// Returns an error if validation fails.
    fn from_raw(raw: RawNpmPackageJson) -> Result<Self, PackageParseError> {
        let name = raw
            .name
            .map(|name| {
                NpmPackageName::parse(name.clone())
                    .map_err(|reason| PackageParseError::InvalidName { name, reason })
            })
            .transpose()?;

        let module_kind = match raw.type_field {
            Some(type_field) => {
                let type_field = type_field.trim();
                if type_field.is_empty() {
                    return Err(PackageParseError::EmptyType);
                }
                match type_field {
                    "commonjs" => ModuleKind::CommonJs,
                    "module" => ModuleKind::Module,
                    _ => {
                        return Err(PackageParseError::InvalidModuleKind {
                            type_field: type_field.to_owned(),
                        });
                    }
                }
            }
            None => ModuleKind::CommonJs,
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
        let dependencies = parse_manifest_dependency_map("dependencies", raw.dependencies)?;
        let dev_dependencies =
            parse_manifest_dependency_map("devDependencies", raw.dev_dependencies)?;
        let peer_dependencies =
            parse_manifest_dependency_map("peerDependencies", raw.peer_dependencies)?;
        let peer_dependencies_meta =
            parse_manifest_peer_dependency_meta(raw.peer_dependencies_meta)?;
        let optional_dependencies =
            parse_manifest_dependency_map("optionalDependencies", raw.optional_dependencies)?;
        let bundle_dependencies = parse_manifest_bundle_dependencies(raw.bundle_dependencies)?;
        let overrides = parse_overrides(raw.overrides, "$.overrides")?;
        let workspaces = raw.workspaces.map(Workspaces::from).unwrap_or_default();

        Ok(Self {
            name,
            private: raw.private,
            version,
            module_kind,
            description: raw.description,
            engines,
            scripts,
            dependencies,
            dev_dependencies,
            peer_dependencies,
            peer_dependencies_meta,
            optional_dependencies,
            bundle_dependencies,
            overrides,
            workspaces,
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

fn parse_manifest_dependency_map(
    section: &'static str,
    map: HashMap<String, String>,
) -> Result<DependencyMap, PackageParseError> {
    parse_dependency_map(sanitize_map(map)).map_err(|error| invalid_dependency(section, error))
}

fn parse_manifest_peer_dependency_meta(
    map: HashMap<String, RawPeerDependencyMeta>,
) -> Result<PeerDependencyMetaMap, PackageParseError> {
    let map = map
        .into_iter()
        .map(|(name, meta)| {
            (
                name,
                PeerDependencyMeta {
                    optional: meta.optional.unwrap_or(false),
                },
            )
        })
        .collect();

    parse_peer_dependency_meta_map(map)
        .map_err(|error| invalid_dependency("peerDependenciesMeta", error))
}

fn parse_manifest_bundle_dependencies(
    raw: Option<RawBundleDependencies>,
) -> Result<BundleDependencies, PackageParseError> {
    let names = match raw {
        Some(RawBundleDependencies::List(names)) => names,
        Some(RawBundleDependencies::LegacyBoolean(value)) => {
            let _ = value;
            Vec::new()
        }
        None => Vec::new(),
    };

    BundleDependencies::parse(names)
        .map_err(|error| invalid_dependency("bundleDependencies", error))
}

fn invalid_dependency(
    section: &'static str,
    error: DependencyCollectionParseError,
) -> PackageParseError {
    PackageParseError::InvalidDependency {
        section,
        name: error.name,
        source: error.source,
    }
}

fn parse_overrides(value: Value, path: &str) -> Result<Overrides, PackageParseError> {
    let Value::Object(overrides) = value else {
        if value.is_null() {
            return Ok(Overrides::default());
        }
        return Err(PackageParseError::InvalidOverride {
            path: path.to_owned(),
            details: "overrides must be an object".to_owned(),
        });
    };

    let mut parsed = BTreeMap::new();
    for (key, value) in overrides {
        let entry_path = format!(
            "{path}[{}]",
            serde_json::to_string(&key).expect("string serializes")
        );
        let override_value = match value {
            Value::String(specification) => DependencySpec::parse(specification)
                .map_err(|error| PackageParseError::InvalidOverride {
                    path: entry_path.clone(),
                    details: error.to_string(),
                })
                .map(Override::Specification)?,
            Value::Object(_) => Override::Nested(parse_overrides(value, &entry_path)?),
            _ => {
                return Err(PackageParseError::InvalidOverride {
                    path: entry_path,
                    details: "an override must be a dependency specification or object".to_owned(),
                });
            }
        };
        parsed.insert(key, override_value);
    }

    Ok(Overrides(parsed))
}

impl From<RawWorkspaces> for Workspaces {
    fn from(raw: RawWorkspaces) -> Self {
        match raw {
            RawWorkspaces::Packages(packages) => Self::Packages(packages),
            RawWorkspaces::Configuration { packages, nohoist } => {
                Self::Configuration { packages, nohoist }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npm::packument::parse_packument;
    use serde_json::json;
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
        assert_eq!(parsed.module_kind, ModuleKind::Module);
        // Verify whitespace sanitizer trimmed our map values
        assert_eq!(
            parsed.dev_dependencies.get(&dependency_name("typescript")),
            Some(&dependency_spec("5.0.0"))
        );
        assert_eq!(
            parsed.peer_dependencies.get(&dependency_name("react")),
            Some(&dependency_spec("^19.0.0"))
        );
        assert_eq!(
            parsed
                .optional_dependencies
                .get(&dependency_name("fsevents")),
            Some(&dependency_spec("~2.3.3"))
        );
        assert_eq!(
            parsed.peer_dependencies_meta.get(&dependency_name("react")),
            Some(&PeerDependencyMeta { optional: true })
        );
        assert_eq!(
            parsed.bundle_dependencies.as_slice(),
            &[dependency_name("react")]
        );
        assert_eq!(
            parsed.overrides.entries().get("react"),
            Some(&Override::Specification(dependency_spec("19.0.0")))
        );
        assert_eq!(
            parsed.workspaces,
            Workspaces::Configuration {
                packages: vec!["packages/*".to_owned()],
                nohoist: Vec::new(),
            }
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
        assert_eq!(parsed.module_kind, ModuleKind::CommonJs);
        assert_eq!(
            parsed.dependencies.get(&dependency_name("react")),
            Some(&dependency_spec("^19.0.0"))
        );

        Ok(())
    }

    #[test]
    fn parses_an_explicit_commonjs_type_field() -> Result<(), PackageParseError> {
        let json_payload = r#"{
            "type": " commonjs "
        }"#;

        let parsed = NpmPackageJson::parse_package_json(Cursor::new(json_payload))?;

        assert_eq!(parsed.module_kind, ModuleKind::CommonJs);

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

    #[test]
    fn rejects_an_unknown_type_field() {
        let json_payload = r#"{
            "type": "definitely-not-a-node-type"
        }"#;

        let error = NpmPackageJson::parse_package_json(Cursor::new(json_payload)).unwrap_err();

        assert!(matches!(
            error,
            PackageParseError::InvalidModuleKind { type_field }
                if type_field == "definitely-not-a-node-type"
        ));
    }

    #[test]
    fn accepts_workspace_list_and_legacy_bundle_dependencies() -> Result<(), PackageParseError> {
        let json_payload = r#"{
            "workspaces": ["packages/*"],
            "bundleDependencies": false
        }"#;

        let parsed = NpmPackageJson::parse_package_json(Cursor::new(json_payload))?;

        assert_eq!(
            parsed.workspaces,
            Workspaces::Packages(vec!["packages/*".to_owned()])
        );
        assert!(parsed.bundle_dependencies.is_empty());

        Ok(())
    }

    #[test]
    fn parses_nested_overrides() -> Result<(), PackageParseError> {
        let json_payload = r#"{
            "overrides": {
                "react": {
                    "scheduler": "^0.25.0"
                }
            }
        }"#;

        let parsed = NpmPackageJson::parse_package_json(Cursor::new(json_payload))?;
        let Override::Nested(react) = parsed.overrides.entries().get("react").unwrap() else {
            panic!("react override should be nested");
        };

        assert_eq!(
            react.entries().get("scheduler"),
            Some(&Override::Specification(dependency_spec("^0.25.0")))
        );

        Ok(())
    }

    #[test]
    fn reports_the_dependency_section_and_key_for_invalid_entries() {
        let json_payload = r#"{
            "dependencies": {
                "@scope": "^1.0.0"
            }
        }"#;

        let error = NpmPackageJson::parse_package_json(Cursor::new(json_payload)).unwrap_err();

        assert!(matches!(
            error,
            PackageParseError::InvalidDependency { section, name, .. }
                if section == "dependencies" && name == "@scope"
        ));
    }

    #[test]
    fn shares_typed_dependency_results_with_packuments() -> Result<(), Box<dyn std::error::Error>> {
        let dependencies = json!({
            "react": "^19.0.0",
            "alias": "npm:Deferred@0.1.0",
            "local": "file:../local"
        });
        let peer_dependencies = json!({ "react": "^19.0.0" });
        let peer_dependencies_meta = json!({ "react": { "optional": true } });
        let optional_dependencies = json!({ "fsevents": "~2.3.3" });
        let bundle_dependencies = json!(["react"]);

        let manifest = NpmPackageJson::parse_package_json(Cursor::new(
            json!({
                "dependencies": dependencies,
                "peerDependencies": peer_dependencies,
                "peerDependenciesMeta": peer_dependencies_meta,
                "optionalDependencies": optional_dependencies,
                "bundleDependencies": bundle_dependencies,
            })
            .to_string(),
        ))?;

        let packument = parse_packument(
            json!({
                "name": "example",
                "dist-tags": { "latest": "1.0.0" },
                "versions": {
                    "1.0.0": {
                        "name": "example",
                        "version": "1.0.0",
                        "dependencies": dependencies,
                        "peerDependencies": peer_dependencies,
                        "peerDependenciesMeta": peer_dependencies_meta,
                        "optionalDependencies": optional_dependencies,
                        "bundleDependencies": bundle_dependencies,
                        "dist": {
                            "tarball": "https://registry.npmjs.org/example/-/example-1.0.0.tgz",
                            "shasum": "0123456789abcdef0123456789abcdef01234567"
                        }
                    }
                }
            })
            .to_string()
            .as_bytes(),
        )?;
        let version = packument.versions.values().next().unwrap();

        assert_eq!(manifest.dependencies, version.dependencies);
        assert_eq!(manifest.peer_dependencies, version.peer_dependencies);
        assert_eq!(
            manifest.peer_dependencies_meta,
            version.peer_dependencies_meta
        );
        assert_eq!(
            manifest.optional_dependencies,
            version.optional_dependencies
        );
        assert_eq!(manifest.bundle_dependencies, version.bundle_dependencies);

        Ok(())
    }

    fn dependency_name(value: &str) -> super::super::types::DependencyPackageName {
        super::super::types::DependencyPackageName::parse(value.to_owned())
            .expect("valid dependency name")
    }

    fn dependency_spec(value: &str) -> DependencySpec {
        DependencySpec::parse(value.to_owned()).expect("valid dependency specification")
    }
}
