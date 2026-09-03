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
    BundleDependencies, DependencyCollectionParseError, DependencyMap, DependencyPackageName,
    DependencySpec, NpmPackageName, NpmPackageNameError, NpmTypeParseError, PeerDependencyMeta,
    PeerDependencyMetaMap, parse_dependency_map, parse_peer_dependency_meta_map,
};

/// Largest accepted `package.json` document, in bytes.
pub const MAX_PACKAGE_JSON_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Error)]
pub enum PackageParseError {
    #[error("I/O error while reading package.json")]
    Io(#[from] std::io::Error),

    #[error("Invalid JSON")]
    InvalidJson(#[from] serde_json::Error),

    #[error("package.json exceeds the {MAX_PACKAGE_JSON_BYTES}-byte limit")]
    PackageTooLarge,

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
    DependencyReference(DependencyPackageName),
    Nested(Overrides),
}

/// The manifest section that declared a root dependency candidate.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DependencyKind {
    Dependencies,
    BundleDependencies,
    DevDependencies,
    PeerDependencies,
    OptionalDependencies,
}

/// A dependency that can seed package-source resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootDependency {
    pub name: DependencyPackageName,
    pub specification: DependencySpec,
    pub kind: DependencyKind,
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
    /// Returns every dependency specification that can seed package-source resolution.
    pub fn root_dependencies(&self) -> Vec<RootDependency> {
        let mut roots = Vec::new();
        append_root_dependencies(&mut roots, DependencyKind::Dependencies, &self.dependencies);
        append_bundle_root_dependencies(
            &mut roots,
            DependencyKind::BundleDependencies,
            &self.bundle_dependencies,
            &self.dependencies,
        );
        append_root_dependencies(
            &mut roots,
            DependencyKind::DevDependencies,
            &self.dev_dependencies,
        );
        append_root_dependencies(
            &mut roots,
            DependencyKind::PeerDependencies,
            &self.peer_dependencies,
        );
        append_root_dependencies(
            &mut roots,
            DependencyKind::OptionalDependencies,
            &self.optional_dependencies,
        );
        roots.sort_by(|left, right| {
            left.name
                .as_str()
                .cmp(right.name.as_str())
                .then(left.kind.cmp(&right.kind))
        });
        roots
    }

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
                        version: trimmed.to_owned(),
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
        let bundle_dependencies =
            parse_manifest_bundle_dependencies(raw.bundle_dependencies, &dependencies)?;
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
        let mut bytes = Vec::new();
        reader
            .take(MAX_PACKAGE_JSON_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_PACKAGE_JSON_BYTES {
            return Err(PackageParseError::PackageTooLarge);
        }

        let raw = serde_json::from_slice(&bytes)?;
        Self::from_raw(raw)
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
        .map(|(k, v)| (k, v.trim().to_owned()))
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
    dependencies: &DependencyMap,
) -> Result<BundleDependencies, PackageParseError> {
    match raw {
        Some(RawBundleDependencies::List(names)) => BundleDependencies::parse(names)
            .map_err(|error| invalid_dependency("bundleDependencies", error)),
        Some(RawBundleDependencies::LegacyBoolean(true)) => {
            Ok(BundleDependencies::all_dependencies(dependencies))
        }
        Some(RawBundleDependencies::LegacyBoolean(false)) | None => {
            Ok(BundleDependencies::default())
        }
    }
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

fn append_root_dependencies(
    roots: &mut Vec<RootDependency>,
    kind: DependencyKind,
    dependencies: &DependencyMap,
) {
    roots.extend(
        dependencies
            .iter()
            .map(|(name, specification)| RootDependency {
                name: name.clone(),
                specification: specification.clone(),
                kind,
            }),
    );
}

fn append_bundle_root_dependencies(
    roots: &mut Vec<RootDependency>,
    kind: DependencyKind,
    bundled: &BundleDependencies,
    dependencies: &DependencyMap,
) {
    roots.extend(bundled.as_slice().iter().filter_map(|name| {
        dependencies.get(name).map(|specification| RootDependency {
            name: name.clone(),
            specification: specification.clone(),
            kind,
        })
    }));
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
            Value::String(specification) => {
                if let Some(reference) = specification.strip_prefix('$') {
                    DependencyPackageName::parse(reference.to_owned())
                        .map_err(|error| PackageParseError::InvalidOverride {
                            path: entry_path.clone(),
                            details: error.to_string(),
                        })
                        .map(Override::DependencyReference)?
                } else {
                    DependencySpec::parse(specification)
                        .map_err(|error| PackageParseError::InvalidOverride {
                            path: entry_path.clone(),
                            details: error.to_string(),
                        })
                        .map(Override::Specification)?
                }
            }
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
    use proptest::prelude::*;
    use serde_json::json;
    use std::{
        fs::File,
        io::{Cursor, ErrorKind, Read},
    };

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
        assert!(parsed.bundle_dependencies.is_empty());

        Ok(())
    }

    #[test]
    fn parses_manifest_compatibility_matrix() -> Result<(), PackageParseError> {
        struct Case {
            name: &'static str,
            manifest: serde_json::Value,
            expected_name: Option<&'static str>,
            expected_private: bool,
            expected_version: Option<&'static str>,
            expected_module_kind: ModuleKind,
        }

        let cases = [
            Case {
                name: "minimal manifest",
                manifest: json!({}),
                expected_name: None,
                expected_private: false,
                expected_version: None,
                expected_module_kind: ModuleKind::CommonJs,
            },
            Case {
                name: "private CommonJS package",
                manifest: json!({
                    "name": "example-package",
                    "private": true,
                    "version": " 1.2.3 ",
                    "type": " commonjs "
                }),
                expected_name: Some("example-package"),
                expected_private: true,
                expected_version: Some("1.2.3"),
                expected_module_kind: ModuleKind::CommonJs,
            },
            Case {
                name: "scoped module package",
                manifest: json!({
                    "name": "@scope/example-package",
                    "version": "2.0.0",
                    "type": "module"
                }),
                expected_name: Some("@scope/example-package"),
                expected_private: false,
                expected_version: Some("2.0.0"),
                expected_module_kind: ModuleKind::Module,
            },
        ];

        for case in cases {
            let parsed =
                NpmPackageJson::parse_package_json(Cursor::new(case.manifest.to_string()))?;

            assert_eq!(
                parsed.name.as_ref().map(NpmPackageName::as_str),
                case.expected_name,
                "{}",
                case.name
            );
            assert_eq!(parsed.private, case.expected_private, "{}", case.name);
            assert_eq!(
                parsed.version.as_ref().map(ToString::to_string).as_deref(),
                case.expected_version,
                "{}",
                case.name
            );
            assert_eq!(
                parsed.module_kind, case.expected_module_kind,
                "{}",
                case.name
            );
        }

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
    fn expands_true_bundle_dependencies_to_declared_dependencies() -> Result<(), PackageParseError>
    {
        let json_payload = r#"{
            "dependencies": {
                "react": "^19.0.0",
                "scheduler": "^0.25.0"
            },
            "bundleDependencies": true
        }"#;

        let parsed = NpmPackageJson::parse_package_json(Cursor::new(json_payload))?;

        assert_eq!(parsed.bundle_dependencies.as_slice().len(), 2);
        assert!(
            parsed
                .bundle_dependencies
                .as_slice()
                .contains(&dependency_name("react"))
        );
        assert!(
            parsed
                .bundle_dependencies
                .as_slice()
                .contains(&dependency_name("scheduler"))
        );

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
    fn parses_override_dependency_references() -> Result<(), PackageParseError> {
        let parsed = NpmPackageJson::parse_package_json(Cursor::new(
            json!({
                "overrides": {
                    "runtime": "$development",
                    "scoped": "$@scope/runtime"
                }
            })
            .to_string(),
        ))?;

        assert_eq!(
            parsed.overrides.entries().get("runtime"),
            Some(&Override::DependencyReference(dependency_name(
                "development"
            )))
        );
        assert_eq!(
            parsed.overrides.entries().get("scoped"),
            Some(&Override::DependencyReference(dependency_name(
                "@scope/runtime"
            )))
        );

        Ok(())
    }

    #[test]
    fn parses_dependency_specification_matrix() -> Result<(), PackageParseError> {
        let specifications = [
            ("registry", "^1.0.0"),
            ("tag", "latest"),
            ("file", "file:../package"),
            ("link", "link:../package"),
            ("workspace", "workspace:^"),
            ("git", "git+https://example.com/package.git"),
            ("github-protocol", "github:owner/package"),
            ("github-shorthand", "owner/package#v1.0.0"),
            ("url", "https://example.com/package.tgz"),
            ("alias", "npm:@scope/package@^1.0.0"),
        ];
        let dependencies = specifications
            .iter()
            .map(|(name, specification)| {
                (
                    (*name).to_owned(),
                    serde_json::Value::String((*specification).to_owned()),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let parsed = NpmPackageJson::parse_package_json(Cursor::new(
            json!({ "dependencies": dependencies }).to_string(),
        ))?;

        for (name, specification) in specifications {
            assert_eq!(
                parsed.dependencies.get(&dependency_name(name)),
                Some(&dependency_spec(specification)),
                "{name}"
            );
        }

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
    fn rejects_invalid_manifest_matrix() {
        enum ExpectedError {
            InvalidJson,
            InvalidName(NpmPackageNameError),
            EmptyType,
            InvalidModuleKind(&'static str),
            InvalidVersion(&'static str),
            InvalidDependency {
                section: &'static str,
                name: &'static str,
            },
            InvalidOverride(&'static str),
        }

        struct Case {
            name: &'static str,
            manifest: serde_json::Value,
            expected: ExpectedError,
        }

        let cases = [
            Case {
                name: "non-object root",
                manifest: json!([]),
                expected: ExpectedError::InvalidJson,
            },
            Case {
                name: "reserved package name",
                manifest: json!({ "name": "node_modules" }),
                expected: ExpectedError::InvalidName(NpmPackageNameError::Reserved),
            },
            Case {
                name: "empty module kind",
                manifest: json!({ "type": "  " }),
                expected: ExpectedError::EmptyType,
            },
            Case {
                name: "unknown module kind",
                manifest: json!({ "type": "amd" }),
                expected: ExpectedError::InvalidModuleKind("amd"),
            },
            Case {
                name: "invalid version",
                manifest: json!({ "version": "not-a-version" }),
                expected: ExpectedError::InvalidVersion("not-a-version"),
            },
            Case {
                name: "invalid dev dependency",
                manifest: json!({ "devDependencies": { "@scope": "1.0.0" } }),
                expected: ExpectedError::InvalidDependency {
                    section: "devDependencies",
                    name: "@scope",
                },
            },
            Case {
                name: "invalid bundle dependency",
                manifest: json!({ "bundleDependencies": ["not valid"] }),
                expected: ExpectedError::InvalidDependency {
                    section: "bundleDependencies",
                    name: "not valid",
                },
            },
            Case {
                name: "invalid override shape",
                manifest: json!({ "overrides": { "react": true } }),
                expected: ExpectedError::InvalidOverride("$.overrides[\"react\"]"),
            },
        ];

        for case in cases {
            let error = NpmPackageJson::parse_package_json(Cursor::new(case.manifest.to_string()))
                .expect_err(case.name);

            match case.expected {
                ExpectedError::InvalidJson => {
                    assert!(
                        matches!(error, PackageParseError::InvalidJson(_)),
                        "{}",
                        case.name
                    );
                }
                ExpectedError::InvalidName(reason) => {
                    assert!(
                        matches!(
                            error,
                            PackageParseError::InvalidName { reason: actual, .. } if actual == reason
                        ),
                        "{}",
                        case.name
                    );
                }
                ExpectedError::EmptyType => {
                    assert!(
                        matches!(error, PackageParseError::EmptyType),
                        "{}",
                        case.name
                    );
                }
                ExpectedError::InvalidModuleKind(type_field) => {
                    assert!(
                        matches!(
                            error,
                            PackageParseError::InvalidModuleKind { type_field: actual } if actual == type_field
                        ),
                        "{}",
                        case.name
                    );
                }
                ExpectedError::InvalidVersion(version) => {
                    assert!(
                        matches!(
                            error,
                            PackageParseError::InvalidPackageVersion { version: actual, .. } if actual == version
                        ),
                        "{}",
                        case.name
                    );
                }
                ExpectedError::InvalidDependency { section, name } => {
                    assert!(
                        matches!(
                            error,
                            PackageParseError::InvalidDependency { section: actual_section, name: actual_name, .. }
                                if actual_section == section && actual_name == name
                        ),
                        "{}",
                        case.name
                    );
                }
                ExpectedError::InvalidOverride(path) => {
                    assert!(
                        matches!(
                            error,
                            PackageParseError::InvalidOverride { path: actual, .. } if actual == path
                        ),
                        "{}",
                        case.name
                    );
                }
            }
        }
    }

    #[test]
    fn preserves_every_dependency_class_for_later_resolution() -> Result<(), PackageParseError> {
        let parsed = NpmPackageJson::parse_package_json(Cursor::new(
            json!({
                "dependencies": { "runtime": "1.0.0", "bundled": "5.0.0" },
                "devDependencies": { "development": "2.0.0" },
                "peerDependencies": { "peer": "3.0.0" },
                "peerDependenciesMeta": { "peer": { "optional": true } },
                "optionalDependencies": { "optional": "4.0.0" },
                "bundleDependencies": ["bundled"],
            })
            .to_string(),
        ))?;

        assert_eq!(
            parsed.root_dependencies(),
            vec![
                RootDependency {
                    name: dependency_name("bundled"),
                    specification: dependency_spec("5.0.0"),
                    kind: DependencyKind::Dependencies,
                },
                RootDependency {
                    name: dependency_name("bundled"),
                    specification: dependency_spec("5.0.0"),
                    kind: DependencyKind::BundleDependencies,
                },
                RootDependency {
                    name: dependency_name("development"),
                    specification: dependency_spec("2.0.0"),
                    kind: DependencyKind::DevDependencies,
                },
                RootDependency {
                    name: dependency_name("optional"),
                    specification: dependency_spec("4.0.0"),
                    kind: DependencyKind::OptionalDependencies,
                },
                RootDependency {
                    name: dependency_name("peer"),
                    specification: dependency_spec("3.0.0"),
                    kind: DependencyKind::PeerDependencies,
                },
                RootDependency {
                    name: dependency_name("runtime"),
                    specification: dependency_spec("1.0.0"),
                    kind: DependencyKind::Dependencies,
                },
            ]
        );
        assert_eq!(
            parsed.bundle_dependencies.as_slice(),
            &[dependency_name("bundled")]
        );
        assert_eq!(
            parsed.peer_dependencies_meta.get(&dependency_name("peer")),
            Some(&PeerDependencyMeta { optional: true })
        );

        Ok(())
    }

    #[test]
    fn parses_the_complex_manifest_fixture() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = File::open(format!(
            "{}/testdata/npm/package-json/complex-manifest.json",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        let parsed = NpmPackageJson::parse_package_json(fixture)?;

        assert_eq!(
            parsed.name.as_ref().map(NpmPackageName::as_str),
            Some("@night-vision/example")
        );
        assert_eq!(parsed.module_kind, ModuleKind::Module);
        assert_eq!(parsed.dependencies.len(), 3);
        assert_eq!(parsed.dev_dependencies.len(), 1);
        assert_eq!(parsed.peer_dependencies.len(), 1);
        assert_eq!(parsed.optional_dependencies.len(), 1);
        assert_eq!(
            parsed.bundle_dependencies.as_slice(),
            &[dependency_name("runtime")]
        );
        assert_eq!(parsed.engines.get("node"), Some(&">=20".to_owned()));
        assert_eq!(parsed.scripts.get("test"), Some(&"cargo test".to_owned()));
        assert_eq!(
            parsed.workspaces,
            Workspaces::Configuration {
                packages: vec!["packages/*".to_owned()],
                nohoist: vec!["**/legacy".to_owned()],
            }
        );
        assert!(matches!(
            parsed.overrides.entries().get("runtime"),
            Some(Override::Nested(_))
        ));

        Ok(())
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
        let dev_dependencies = json!({ "typescript": "^5.0.0" });
        let bundle_dependencies = json!(["react"]);

        let manifest = NpmPackageJson::parse_package_json(Cursor::new(
            json!({
                "dependencies": dependencies,
                "devDependencies": dev_dependencies,
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
                        "devDependencies": dev_dependencies,
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
        assert_eq!(manifest.dev_dependencies, version.dev_dependencies);
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

    #[test]
    fn parser_accepts_short_reads_and_reports_reader_errors() -> Result<(), PackageParseError> {
        let parsed = NpmPackageJson::parse_package_json(ShortReader {
            bytes: br#"{ "dependencies": { "runtime": "1.0.0" } }"#,
            position: 0,
            maximum_chunk_size: 1,
        })?;
        assert_eq!(parsed.root_dependencies().len(), 1);

        let error = NpmPackageJson::parse_package_json(FailingReader).unwrap_err();
        assert!(matches!(error, PackageParseError::Io(error) if error.kind() == ErrorKind::Other));

        Ok(())
    }

    #[test]
    fn rejects_manifests_larger_than_the_document_limit() {
        let maximum_size =
            usize::try_from(MAX_PACKAGE_JSON_BYTES).expect("document limit fits in usize");
        let oversized_size = maximum_size
            .checked_add(1)
            .expect("one byte over document limit fits in usize");
        let oversized = vec![b' '; oversized_size];
        let error = NpmPackageJson::parse_package_json(Cursor::new(oversized)).unwrap_err();

        assert!(matches!(error, PackageParseError::PackageTooLarge));
    }

    #[test]
    fn document_limit_is_one_mebibyte() {
        assert_eq!(MAX_PACKAGE_JSON_BYTES, 1_048_576);
    }

    #[test]
    fn accepts_a_manifest_at_the_document_limit() -> Result<(), PackageParseError> {
        let wrapper = r#"{"description":""}"#;
        let maximum_size =
            usize::try_from(MAX_PACKAGE_JSON_BYTES).expect("document limit fits in usize");
        let description_length = maximum_size
            .checked_sub(wrapper.len())
            .expect("wrapper fits within document limit");
        let manifest = format!(r#"{{"description":"{}"}}"#, "a".repeat(description_length));

        let parsed = NpmPackageJson::parse_package_json(Cursor::new(manifest))?;

        assert_eq!(parsed.description.len(), description_length);

        Ok(())
    }

    proptest! {
        #[test]
        fn parser_never_panics_for_arbitrary_bytes(input in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let _ = NpmPackageJson::parse_package_json(Cursor::new(input));
        }

        #[test]
        fn parses_generated_valid_dependency_collections(
            package in "[a-z][a-z0-9-]{0,12}",
            dependency in "[a-z][a-z0-9-]{0,12}",
            major in 0_u16..100,
        ) {
            let manifest = json!({
                "name": package,
                "version": format!("{major}.0.0"),
                "dependencies": { dependency.clone(): format!("^{major}.0.0") },
                "bundleDependencies": true,
            });

            let parsed = NpmPackageJson::parse_package_json(Cursor::new(manifest.to_string()))?;
            let dependency = dependency_name(&dependency);

            prop_assert!(parsed.dependencies.contains_key(&dependency));
            prop_assert_eq!(parsed.bundle_dependencies.as_slice(), &[dependency]);
        }

        #[test]
        fn unknown_fields_do_not_change_resolvable_dependencies(
            unknown_key in "x[a-z][a-z0-9-]{0,12}",
            unknown_value in ".{0,80}",
        ) {
            let base = json!({ "dependencies": { "runtime": "^1.0.0" } });
            let mut with_unknown_field = base.as_object().expect("object literal").clone();
            with_unknown_field.insert(unknown_key, serde_json::Value::String(unknown_value));

            let base = NpmPackageJson::parse_package_json(Cursor::new(base.to_string()))?;
            let with_unknown_field = NpmPackageJson::parse_package_json(Cursor::new(
                serde_json::Value::Object(with_unknown_field).to_string(),
            ))?;

            prop_assert_eq!(base.root_dependencies(), with_unknown_field.root_dependencies());
        }
    }

    struct ShortReader {
        bytes: &'static [u8],
        position: usize,
        maximum_chunk_size: usize,
    }

    impl Read for ShortReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let remaining = &self.bytes[self.position..];
            let length = remaining
                .len()
                .min(self.maximum_chunk_size)
                .min(buffer.len());
            buffer[..length].copy_from_slice(&remaining[..length]);
            self.position = self
                .position
                .checked_add(length)
                .expect("read position cannot overflow");
            Ok(length)
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("reader failed"))
        }
    }

    fn dependency_name(value: &str) -> super::super::types::DependencyPackageName {
        super::super::types::DependencyPackageName::parse(value.to_owned())
            .expect("valid dependency name")
    }

    fn dependency_spec(value: &str) -> DependencySpec {
        DependencySpec::parse(value.to_owned()).expect("valid dependency specification")
    }
}
