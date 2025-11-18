// SPDX-License-Identifier: Apache-2.0

use crate::{
	string_newtype_parse_kdl_node,
	task::manifest::{
		kdl::{ParseKdlNode, ToKdlNode, extract_data},
		util::agent,
	},
};
use anyhow::{Result, anyhow};
use kdl::{KdlDocument, KdlNode, KdlValue};
use regex::Regex;
use std::{
	cmp::Ordering,
	fmt::Display,
	hash::{Hash, Hasher},
	str::FromStr,
	sync::LazyLock,
};

static VERSION_REGEX: LazyLock<Regex> =
	LazyLock::new(|| Regex::new("[v]?([0-9]+).([0-9]+).([0-9]+)").unwrap());

pub fn parse_plugin_version(version_str: &str) -> Option<PluginVersion> {
	VERSION_REGEX.captures(version_str).map(|m| {
		PluginVersion(
			m.get(1).unwrap().as_str().parse::<u8>().unwrap(),
			m.get(2).unwrap().as_str().parse::<u8>().unwrap(),
			m.get(3).unwrap().as_str().parse::<u8>().unwrap(),
		)
	})
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct PluginVersion(pub u8, pub u8, pub u8);

impl ParseKdlNode for PluginVersion {
	fn kdl_key() -> &'static str {
		"version"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		let raw_version = node.entries().first()?.value().as_string()?;
		parse_plugin_version(raw_version)
	}
}

impl Display for PluginVersion {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "{}.{}.{}", self.0, self.1, self.2)
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Arch(pub String);
string_newtype_parse_kdl_node!(Arch, "arch");

// NOTE: the implementation in this crate was largely derived from RFD #0004

impl ParseKdlNode for url::Url {
	fn kdl_key() -> &'static str {
		"url"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		let raw_url = node.entries().first()?.value().as_string()?;
		url::Url::from_str(raw_url).ok()
	}
}

/// Contains all of the hash algorithms supported inside of the plugin download manifest
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HashAlgorithm {
	Sha256,
	Blake3,
}

impl Display for HashAlgorithm {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			HashAlgorithm::Sha256 => write!(f, "SHA256"),
			HashAlgorithm::Blake3 => write!(f, "BLAKE3"),
		}
	}
}

impl TryFrom<&str> for HashAlgorithm {
	type Error = anyhow::Error;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value {
			"SHA256" => Ok(HashAlgorithm::Sha256),
			"BLAKE3" => Ok(HashAlgorithm::Blake3),
			_ => Err(anyhow!("Invalid hash algorithm specified: '{}'", value)),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BufferedDigest {
	Resolved(String),
	Remote(url::Url),
}
impl BufferedDigest {
	#[allow(unused)] // Will be used in a later PR
	fn resolve(self) -> anyhow::Result<String> {
		use BufferedDigest::*;
		match self {
			Resolved(s) => Ok(s),
			Remote(u) => {
				let agent = agent::agent()?;
				let raw_hash_str = agent.get(u.as_ref()).call()?.into_string()?;
				let Some(hash_str) = raw_hash_str.split_whitespace().next() else {
					return Err(anyhow!("malformed sha256 file at {}", u));
				};
				Ok(hash_str.to_owned())
			}
		}
	}
}
impl From<String> for BufferedDigest {
	fn from(value: String) -> Self {
		BufferedDigest::Resolved(value)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashWithDigest {
	/// hash algorithm used
	pub hash_algorithm: HashAlgorithm,
	/// expected hash of artifact when hashed with the hash_algorithm specified
	pub digest: BufferedDigest,
}

impl HashWithDigest {
	pub fn new(hash_algorithm: HashAlgorithm, digest: BufferedDigest) -> Self {
		Self {
			hash_algorithm,
			digest,
		}
	}
}

impl ParseKdlNode for HashWithDigest {
	fn kdl_key() -> &'static str {
		"hash"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		// Per RFD #0004, the hash algorithm is of type String
		let specified_algorithm = node.get("alg")?.as_string()?;
		let hash_algorithm = HashAlgorithm::try_from(specified_algorithm).ok()?;
		// Per RFD #0004, the digest is of type String
		let digest = node.get("digest")?.as_string()?.to_string();
		Some(HashWithDigest::new(
			hash_algorithm,
			BufferedDigest::Resolved(digest),
		))
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveFormat {
	/// archived with tar and compressed with the XZ algorithm
	TarXz,
	/// archived with tar and compressed with the Gzip algorithm
	TarGz,
	/// archived with tar and compressed with the zstd algorithm
	TarZst,
	/// archived with tar, not compressed
	Tar,
	/// archived and compressed with zip
	Zip,
}

impl Display for ArchiveFormat {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ArchiveFormat::TarXz => write!(f, "tar.xz"),
			ArchiveFormat::TarGz => write!(f, "tar.gz"),
			ArchiveFormat::TarZst => write!(f, "tar.zst"),
			ArchiveFormat::Tar => write!(f, "tar"),
			ArchiveFormat::Zip => write!(f, "zip"),
		}
	}
}

impl TryFrom<&str> for ArchiveFormat {
	type Error = anyhow::Error;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value {
			"tar.xz" => Ok(ArchiveFormat::TarXz),
			"tar.gz" => Ok(ArchiveFormat::TarGz),
			"tar.zst" => Ok(ArchiveFormat::TarZst),
			"tar" => Ok(ArchiveFormat::Tar),
			"zip" => Ok(ArchiveFormat::Zip),
			_ => Err(anyhow!("Invalid compression format specified")),
		}
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Compress {
	/// compression algorithm used for the downloaded archive
	pub format: ArchiveFormat,
}

impl Compress {
	#[cfg(test)]
	pub fn new(archive_format: ArchiveFormat) -> Self {
		Self {
			format: archive_format,
		}
	}
}

impl ParseKdlNode for Compress {
	fn kdl_key() -> &'static str {
		"compress"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		// Per RFD #0004, the format is of type String
		let specified_format = node.get("format")?.as_string()?;
		let format = ArchiveFormat::try_from(specified_format).ok()?;
		Some(Compress { format })
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Size {
	/// size of the downloaded artifact, in bytes
	pub bytes: u64,
}

impl Size {
	pub fn new(bytes: u64) -> Self {
		Self { bytes }
	}
}

impl ParseKdlNode for Size {
	fn kdl_key() -> &'static str {
		"size"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let specified_size = node.get("bytes")?;
		let bytes = match specified_size {
			// Negative size and a size of 0 do not make sense
			KdlValue::Integer(bytes) => {
				let bytes = *bytes;
				if bytes.is_positive() {
					bytes as u64
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(Size { bytes })
	}
}

/// Represents one entry in a download manifest file, as spelled out in RFD #0004
/// Example entry:
/// ```
///plugin version="0.1.0" arch="aarch64-apple-darwin" {
///  url "https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz"
///  hash alg="SHA256" digest="b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b"
///  compress format="tar.xz"
///  size bytes=2_869_896
///}
///```
#[derive(Debug, Clone, Eq)]
pub struct DownloadManifestEntry {
	/// A `SemVer` version of the plugin. Not a version requirement as in the plugin manifest file,
	/// but only a specific concrete version
	pub version: PluginVersion,
	/// The target architecture for a plugin
	pub arch: Arch,
	/// The URL of the archive file to download containing the plugin executable artifact and
	/// plugin manifest.
	pub url: url::Url,
	/// Contains info about what algorithm was used to hash the archive and what the expected
	/// digest is
	pub hash: HashWithDigest,
	/// Defines how to handle decompressing the downloaded plugin archive
	pub compress: Compress,
	/// Describes the size of the downloaded artifact, used to validate the download was
	/// successful, makes it more difficult for an attacker to distribute malformed artifacts
	pub size: Size,
}

impl PartialEq for DownloadManifestEntry {
	fn eq(&self, other: &Self) -> bool {
		self.arch == other.arch && self.version == other.version
	}
}

impl Ord for DownloadManifestEntry {
	fn cmp(&self, other: &Self) -> Ordering {
		match self.version.cmp(&other.version) {
			Ordering::Equal => self.arch.cmp(&other.arch),
			o => o,
		}
	}
}

impl PartialOrd for DownloadManifestEntry {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

// Makes it so we can HashSet and enforce unique (version, arch) pairs
impl Hash for DownloadManifestEntry {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.version.hash(state);
		self.arch.hash(state);
	}
}

impl ParseKdlNode for DownloadManifestEntry {
	fn kdl_key() -> &'static str {
		"plugin"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		// Per RFD #0004, arch is of type String
		let arch = Arch(node.get("arch")?.as_string()?.to_string());
		let version = parse_plugin_version(node.get("version")?.as_string()?)?;

		// there should be one child for each plugin and it should contain the url, hash, compress

		// and size information
		let nodes = node.children()?.nodes();

		// extract the url, hash, compress and size from the child
		let url: url::Url = extract_data(nodes)?;
		let hash: HashWithDigest = extract_data(nodes)?;
		let compress: Compress = extract_data(nodes)?;
		let size: Size = extract_data(nodes)?;

		Some(Self {
			version,
			arch,
			url,
			hash,
			compress,
			size,
		})
	}
}

impl ToKdlNode for DownloadManifestEntry {
	fn to_kdl_node(&self) -> Result<KdlNode> {
		let mut parent = KdlNode::new("plugin");
		parent.insert("version", self.version.to_string());
		parent.insert("arch", self.arch.0.clone());

		let mut children = KdlDocument::new();
		let children_nodes = children.nodes_mut();

		let mut url = KdlNode::new("url");
		url.insert(0, self.url.to_string());
		children_nodes.push(url);

		let mut hash = KdlNode::new("hash");
		hash.insert("alg", self.hash.hash_algorithm.to_string());
		let resolved_hash = self.hash.digest.clone().resolve()?;
		hash.insert("digest", resolved_hash);
		children_nodes.push(hash);

		let mut compress = KdlNode::new("compress");
		compress.insert("format", self.compress.format.to_string());
		children_nodes.push(compress);

		let mut size = KdlNode::new("size");
		size.insert("bytes", KdlValue::Integer(self.size.bytes as i128));
		children_nodes.push(size);

		parent.set_children(children);
		Ok(parent)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadManifest {
	pub entries: Vec<DownloadManifestEntry>,
}

impl DownloadManifest {
	#[cfg(test)]
	pub fn iter(&self) -> impl Iterator<Item = &DownloadManifestEntry> {
		self.entries.iter()
	}

	#[cfg(test)]
	pub fn len(&self) -> usize {
		self.entries.len()
	}
}

impl FromStr for DownloadManifest {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let document = KdlDocument::from_str(s)
			.map_err(|e| anyhow!("Error parsing download manifest file: {}", e))?;
		let mut entries = vec![];
		for node in document.nodes() {
			if let Some(entry) = DownloadManifestEntry::parse_node(node) {
				entries.push(entry);
			} else {
				return Err(anyhow!("Error parsing download manifest entry: {}", node));
			}
		}
		Ok(Self { entries })
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use std::str::FromStr;
	use url::Url;

	#[test]
	fn test_parsing_hash_algorithm() {
		let digest = "b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b";

		// test SHA256 parsing
		let node =
			KdlNode::from_str(format!(r#"  hash alg="SHA256" digest="{}""#, digest).as_str())
				.unwrap();
		let parsed_hash_with_digest = HashWithDigest::parse_node(&node).unwrap();
		let expected_hash_with_digest =
			HashWithDigest::new(HashAlgorithm::Sha256, digest.to_string().into());
		assert_eq!(parsed_hash_with_digest, expected_hash_with_digest);

		// test BLAKE3 parsing
		let node =
			KdlNode::from_str(format!(r#"  hash alg="BLAKE3" digest="{}""#, digest).as_str())
				.unwrap();
		let parsed_hash_with_digest = HashWithDigest::parse_node(&node).unwrap();
		let expected_hash_with_digest =
			HashWithDigest::new(HashAlgorithm::Blake3, digest.to_string().into());
		assert_eq!(parsed_hash_with_digest, expected_hash_with_digest);

		// ensure invalid hash algorithms do not pass
		let node = KdlNode::from_str(format!(r#"  hash alg="SHA1" digest="{}""#, digest).as_str())
			.unwrap();
		assert!(HashWithDigest::parse_node(&node).is_none());
		let node = KdlNode::from_str(format!(r#"  hash alg="BLAKE" digest="{}""#, digest).as_str())
			.unwrap();
		assert!(HashWithDigest::parse_node(&node).is_none());
	}

	#[test]
	fn test_parsing_compression_algorithm() {
		let formats = [
			("tar.xz", ArchiveFormat::TarXz),
			("tar.gz", ArchiveFormat::TarGz),
			("tar.zst", ArchiveFormat::TarZst),
			("tar", ArchiveFormat::Tar),
			("zip", ArchiveFormat::Zip),
		];
		for (value, format) in formats {
			let node =
				KdlNode::from_str(format!(r#"compress format="{}""#, value).as_str()).unwrap();
			assert_eq!(Compress::parse_node(&node).unwrap(), Compress { format });
		}
	}

	#[test]
	fn test_parsing_size() {
		// test normal number
		let node = KdlNode::from_str("size bytes=1234").unwrap();
		assert_eq!(Size::parse_node(&node).unwrap(), Size { bytes: 1234 });
		let node = KdlNode::from_str("size bytes=1").unwrap();
		assert_eq!(Size::parse_node(&node).unwrap(), Size { bytes: 1 });

		// test parsing number with _ inside
		let node = KdlNode::from_str("size bytes=1_234_567").unwrap();
		assert_eq!(Size::parse_node(&node).unwrap(), Size { bytes: 1234567 });

		// test that negative number does not work
		let node = KdlNode::from_str("size bytes=-1234").unwrap();
		assert!(Size::parse_node(&node).is_none());

		// ensure 0 does not work
		let node = KdlNode::from_str("size bytes=0").unwrap();
		assert!(Size::parse_node(&node).is_none());

		// ensure negative numbers do not work
		let node = KdlNode::from_str("size bytes=-1").unwrap();
		assert!(Size::parse_node(&node).is_none());
	}

	#[test]
	fn test_parsing_url() {
		let raw_url = "https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz";
		let node = KdlNode::from_str(format!(r#"url "{}""#, raw_url).as_str()).unwrap();
		assert_eq!(
			Url::parse_node(&node).unwrap(),
			Url::parse(raw_url).unwrap()
		);
	}

	#[test]
	fn test_parsing_single_download_manifest_entry() {
		let version = "0.1.0";
		let arch = "aarch64-apple-darwin";
		let url = "https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz";
		let hash_alg = "SHA256";
		let digest = "b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b";
		let compress = "tar.gz";
		let size = "2_869_896";

		let node = KdlNode::from_str(
			format!(
				r#"plugin version="{}" arch="{}" {{
  url "{}"
  hash alg="{}" digest="{}"
  compress format="{}"
  size bytes={}
}}"#,
				version, arch, url, hash_alg, digest, compress, size
			)
			.as_str(),
		)
		.unwrap();

		let expected_entry = DownloadManifestEntry {
			version: parse_plugin_version(version).unwrap(),
			arch: Arch(arch.to_string()),
			url: Url::parse(url).unwrap(),
			hash: HashWithDigest::new(
				HashAlgorithm::try_from(hash_alg).unwrap(),
				digest.to_string().into(),
			),
			compress: Compress {
				format: ArchiveFormat::try_from(compress).unwrap(),
			},
			size: Size {
				bytes: u64::from_str(size.replace("_", "").as_str()).unwrap(),
			},
		};

		assert_eq!(
			DownloadManifestEntry::parse_node(&node).unwrap(),
			expected_entry
		);
	}

	#[test]
	fn test_parsing_multiple_download_manifest_entry() {
		let contents = r#"plugin version="0.1.0" arch="aarch64-apple-darwin" {
  url "https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz"
  hash alg="SHA256" digest="b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b"
  compress format="tar.xz"
  size bytes=2_869_896
}

plugin version="0.1.0" arch="x86_64-apple-darwin" {
  url "https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz"
  hash alg="SHA256" digest="ddb8c6d26dd9a91e11c99b3bd7ee2b9585aedac6e6df614190f1ba2bfe86dc19"
  compress format="tar.xz"
  size bytes=3_183_768
}"#;
		let entries = DownloadManifest::from_str(contents).unwrap();
		assert_eq!(entries.len(), 2);
		let mut entries_iter = entries.iter();
		assert_eq!(
			&DownloadManifestEntry {
				version: PluginVersion(0, 1, 0),
				arch: Arch("aarch64-apple-darwin".to_owned()),
				url: Url::parse("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz").unwrap(),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b".to_owned().into()),
				compress: Compress::new(ArchiveFormat::TarXz),
				size: Size {
					bytes: 2_869_896
				}
			},
		    entries_iter.next().unwrap()
		);
		assert_eq!(
			&DownloadManifestEntry {
				version: PluginVersion(0, 1, 0),
				arch: Arch("x86_64-apple-darwin".to_owned()),
				url: Url::parse("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz").unwrap(),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "ddb8c6d26dd9a91e11c99b3bd7ee2b9585aedac6e6df614190f1ba2bfe86dc19".to_owned().into()),
                compress: Compress::new(ArchiveFormat::TarXz),
                size: Size::new(3_183_768)
			},
		    entries_iter.next().unwrap()
        );
	}

	#[test]
	fn entry_ordering() {
		let a = DownloadManifestEntry {
				version: PluginVersion(0, 1, 0),
				arch: Arch("x86_64-apple-darwin".to_owned()),
				url: Url::parse("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz").unwrap(),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "ddb8c6d26dd9a91e11c99b3bd7ee2b9585aedac6e6df614190f1ba2bfe86dc19".to_owned().into()),
                compress: Compress::new(ArchiveFormat::TarXz),
                size: Size::new(3_183_768)
			};
		let b =DownloadManifestEntry {
				version: PluginVersion(0, 2, 0),
				arch: Arch("aarch64-apple-darwin".to_owned()),
				url: Url::parse("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz").unwrap(),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b".to_owned().into()),
				compress: Compress::new(ArchiveFormat::TarXz),
				size: Size {
					bytes: 2_869_896
				}
            };
		let c = DownloadManifestEntry {
				version: PluginVersion(1, 1, 0),
				arch: Arch("x86_64-apple-darwin".to_owned()),
				url: Url::parse("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz").unwrap(),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "ddb8c6d26dd9a91e11c99b3bd7ee2b9585aedac6e6df614190f1ba2bfe86dc19".to_owned().into()),
                compress: Compress::new(ArchiveFormat::TarXz),
                size: Size::new(3_183_768)
			};
		let d = DownloadManifestEntry {
				version: PluginVersion(1, 1, 0),
				arch: Arch("aarch64-apple-darwin".to_owned()),
				url: Url::parse("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz").unwrap(),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b".to_owned().into()),
				compress: Compress::new(ArchiveFormat::TarXz),
				size: Size {
					bytes: 2_869_896
				}
            };
		let mut raw_vec = vec![&d, &c, &b, &a];
		// smallest version, then arch used as tiebreaker
		let exp_vec = vec![&a, &b, &d, &c];
		raw_vec.sort();
		assert_eq!(raw_vec, exp_vec);
	}
}
