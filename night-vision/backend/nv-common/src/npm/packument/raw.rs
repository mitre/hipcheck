use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub(super) struct RawNpmPackument {
    #[serde(rename = "_id")]
    pub(super) id: Option<String>,
    #[serde(rename = "_rev")]
    pub(super) rev: Option<String>,
    pub(super) name: Option<String>,
    #[serde(rename = "dist-tags")]
    pub(super) dist_tags: Option<HashMap<String, String>>,
    pub(super) modified: Option<String>,
    pub(super) time: Option<HashMap<String, String>>,
    #[serde(default)]
    pub(super) versions: Option<HashMap<String, RawNpmVersion>>,
    pub(super) author: Option<RawHuman>,
    pub(super) bugs: Option<Value>,
    #[serde(default)]
    pub(super) contributors: Option<Vec<RawHuman>>,
    pub(super) description: Option<String>,
    pub(super) homepage: Option<String>,
    pub(super) keywords: Option<Vec<String>>,
    pub(super) license: Option<String>,
    pub(super) maintainers: Option<Vec<RawHuman>>,
    pub(super) readme: Option<String>,
    #[serde(rename = "readmeFilename")]
    pub(super) readme_filename: Option<String>,
    pub(super) repository: Option<RawRepository>,
    pub(super) users: Option<HashMap<String, bool>>,
    #[serde(flatten)]
    pub(super) extra: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawNpmVersion {
    #[serde(rename = "_id")]
    pub(super) id: Option<String>,
    #[serde(rename = "_nodeVersion")]
    pub(super) node_version: Option<String>,
    #[serde(rename = "_npmVersion")]
    pub(super) npm_version: Option<String>,
    #[serde(rename = "_npmUser")]
    pub(super) npm_user: Option<RawHuman>,
    pub(super) name: Option<String>,
    pub(super) version: Option<String>,
    pub(super) description: Option<String>,
    pub(super) main: Option<String>,
    pub(super) license: Option<String>,
    pub(super) author: Option<RawHuman>,
    pub(super) contributors: Option<Vec<RawHuman>>,
    pub(super) maintainers: Option<Vec<RawHuman>>,
    pub(super) repository: Option<RawRepository>,
    pub(super) dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub(super) dev_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "peerDependencies")]
    pub(super) peer_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "optionalDependencies")]
    pub(super) optional_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "bundleDependencies")]
    pub(super) bundle_dependencies: Option<RawBundleDependencies>,
    #[serde(rename = "peerDependenciesMeta")]
    pub(super) peer_dependencies_meta: Option<HashMap<String, RawPeerDependencyMeta>>,
    pub(super) dist: Option<RawNpmDist>,
    pub(super) engines: Option<RawEngines>,
    pub(super) deprecated: Option<RawDeprecated>,
    #[serde(rename = "acceptDependencies")]
    pub(super) accept_dependencies: Option<HashMap<String, String>>,
    pub(super) bin: Option<RawBin>,
    pub(super) directories: Option<HashMap<String, String>>,
    #[serde(rename = "_hasShrinkwrap")]
    pub(super) has_shrinkwrap: Option<bool>,
    #[serde(default, rename = "hasInstallScript")]
    pub(super) has_install_script: Option<bool>,
    pub(super) funding: Option<RawFunding>,
    pub(super) cpu: Option<Vec<String>>,
    pub(super) os: Option<Vec<String>>,
    #[serde(flatten)]
    pub(super) extra: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawFunding {
    Url(String),
    Object {
        url: String,
        #[serde(rename = "type")]
        type_field: Option<String>,
    },
    Array(Vec<RawFundingEntry>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawFundingEntry {
    Url(String),
    Object {
        url: String,
        #[serde(rename = "type")]
        type_field: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawBin {
    String(String),
    Map(HashMap<String, String>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawEngines {
    Map(HashMap<String, String>),
    LegacyList(Vec<String>),
    LegacyString(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawDeprecated {
    Message(String),
    LegacyBoolean(bool),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawBundleDependencies {
    List(Vec<String>),
    LegacyBoolean(bool),
}

#[derive(Debug, Deserialize)]
pub(super) struct RawPeerDependencyMeta {
    pub(super) optional: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawHuman {
    String(String),
    Object {
        name: Option<String>,
        email: Option<String>,
        url: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
pub(super) struct RawRepository {
    #[serde(rename = "type")]
    pub(super) type_field: Option<String>,
    pub(super) url: Option<String>,
}

#[expect(
    dead_code,
    reason = "the raw representation retains registry fields that are not exposed"
)]
#[derive(Debug, Deserialize)]
pub(super) struct RawNpmDist {
    pub(super) tarball: Option<String>,
    pub(super) shasum: Option<String>,
    pub(super) integrity: Option<String>,
    #[serde(rename = "fileCount")]
    pub(super) file_count: Option<u64>,
    #[serde(rename = "unpackedSize")]
    pub(super) unpacked_size: Option<u64>,
    #[serde(rename = "npm-signature")]
    pub(super) npm_signature: Option<String>,
}
