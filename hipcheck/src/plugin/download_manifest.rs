// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use crate::plugin::arch::KnownArch;
use crate::{
	hc_error,
	plugin::{arch::Arch, PluginVersion},
	util::kdl::{extract_data, ParseKdlNode},
};
use kdl::{KdlDocument, KdlNode, KdlValue};
use std::{fmt::Display, str::FromStr};

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
	type Error = crate::Error;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value {
			"SHA256" => Ok(HashAlgorithm::Sha256),
			"BLAKE3" => Ok(HashAlgorithm::Blake3),
			_ => Err(hc_error!("Invalid hash algorithm specified: '{}'", value)),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashWithDigest {
	/// hash algorithm used
	pub hash_algorithm: HashAlgorithm,
	/// expected hash of artifact when hashed with the hash_algorithm specified
	pub digest: String,
}

impl HashWithDigest {
	pub fn new(hash_algorithm: HashAlgorithm, digest: String) -> Self {
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
		Some(HashWithDigest::new(hash_algorithm, digest))
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
	type Error = crate::error::Error;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value {
			"tar.xz" => Ok(ArchiveFormat::TarXz),
			"tar.gz" => Ok(ArchiveFormat::TarGz),
			"tar.zst" => Ok(ArchiveFormat::TarZst),
			"tar" => Ok(ArchiveFormat::Tar),
			"zip" => Ok(ArchiveFormat::Zip),
			_ => Err(hc_error!("Invalid compression format specified")),
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
	#[cfg(test)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadManifestEntry {
	// TODO: make this a SemVer type?
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

impl ParseKdlNode for DownloadManifestEntry {
	fn kdl_key() -> &'static str {
		"plugin"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		// Per RFD #0004, version is of type String
		let version = PluginVersion(node.get("version")?.as_string()?.to_string());
		// Per RFD #0004, arch is of type String
		let arch = Arch::from_str(node.get("arch")?.as_string()?).ok()?;

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
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let document = KdlDocument::from_str(s)
			.map_err(|e| hc_error!("Error parsing download manifest file: {}", e.to_string()))?;
		let mut entries = vec![];
		for node in document.nodes() {
			if let Some(entry) = DownloadManifestEntry::parse_node(node) {
				entries.push(entry);
			} else {
				return Err(hc_error!("Error parsing download manifest entry: {}", node));
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
			HashWithDigest::new(HashAlgorithm::Sha256, digest.to_string());
		assert_eq!(parsed_hash_with_digest, expected_hash_with_digest);

		// test BLAKE3 parsing
		let node =
			KdlNode::from_str(format!(r#"  hash alg="BLAKE3" digest="{}""#, digest).as_str())
				.unwrap();
		let parsed_hash_with_digest = HashWithDigest::parse_node(&node).unwrap();
		let expected_hash_with_digest =
			HashWithDigest::new(HashAlgorithm::Blake3, digest.to_string());
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
			version: PluginVersion(version.to_string()),
			arch: Arch::Known(KnownArch::from_str(arch).unwrap()),
			url: Url::parse(url).unwrap(),
			hash: HashWithDigest::new(
				HashAlgorithm::try_from(hash_alg).unwrap(),
				digest.to_string(),
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
				version: PluginVersion("0.1.0".to_owned()),
				arch: Arch::Known(KnownArch::Aarch64AppleDarwin),
				url: Url::parse("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz").unwrap(),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b".to_owned()),
				compress: Compress::new(ArchiveFormat::TarXz),
				size: Size {
					bytes: 2_869_896
				}
			},
		    entries_iter.next().unwrap()
		);
		assert_eq!(
			&DownloadManifestEntry {
				version: PluginVersion("0.1.0".to_owned()),
				arch: Arch::Known(KnownArch::X86_64AppleDarwin),
				url: Url::parse("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz").unwrap(),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "ddb8c6d26dd9a91e11c99b3bd7ee2b9585aedac6e6df614190f1ba2bfe86dc19".to_owned()),
                compress: Compress::new(ArchiveFormat::TarXz),
                size: Size::new(3_183_768)
			},
		    entries_iter.next().unwrap()
        );
	}
}
