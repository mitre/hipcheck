use crate::{error::Error, hc_error};
use kdl::{KdlDocument, KdlNode, KdlValue};
use std::{fmt::Display, str::FromStr};

// NOTE: the implementation in this crate was largely derived from RFD #0004

#[allow(unused)]
// Helper trait to make it easier to parse KdlNodes into our own types
trait ParseKdlNode
where
	Self: Sized,
{
	/// Return the name of the attribute used to identify the node pertaining to this struct
	fn kdl_key() -> &'static str;

	/// Attempt to convert a `kdl::KdlNode` into Self
	fn parse_node(node: &KdlNode) -> Option<Self>;
}

#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url(pub String);

impl Url {
	pub fn new(url: String) -> Self {
		Self(url)
	}
}

impl AsRef<String> for Url {
	fn as_ref(&self) -> &String {
		&self.0
	}
}

impl ParseKdlNode for Url {
	fn kdl_key() -> &'static str {
		"url"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		// per RFD #0004, the first positional argument will be the URL and it will be a String
		let url = node.entries().first()?;
		match url.value() {
			KdlValue::String(url) => Some(Url(url.clone())),
			_ => None,
		}
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
	fn new(hash_algorithm: HashAlgorithm, digest: String) -> Self {
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

		let specified_algorithm = node.get("alg")?;
		let hash_algorithm = match specified_algorithm.value() {
			KdlValue::String(alg) => HashAlgorithm::try_from(alg.as_str()).ok()?,
			_ => return None,
		};

		let specified_digest = node.get("digest")?;
		let digest = match specified_digest.value() {
			KdlValue::String(digest) => digest.clone(),
			_ => return None,
		};

		Some(HashWithDigest::new(hash_algorithm, digest))
	}
}

#[allow(unused)]
#[derive(Clone, Debug, Eq, PartialEq)]
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

#[allow(unused)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Compress {
	/// compression algorithm used for the downloaded archive
	pub format: ArchiveFormat,
}

impl Compress {
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
		let specified_format = node.get("format")?;
		let format = match specified_format.value() {
			KdlValue::String(format) => ArchiveFormat::try_from(format.as_str()).ok()?,
			_ => return None,
		};
		Some(Compress { format })
	}
}

#[allow(unused)]
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
		let bytes = match specified_size.value() {
			// Negative size and a size of 0 do not make sense
			KdlValue::Base10(bytes) => {
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
#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadManifestEntry {
	// TODO: make this a SemVer type?
	/// A `SemVer` version of the plugin. Not a version requirement as in the plugin manifest file,
	/// but only a specific concrete version
	pub version: String,
	// TODO: make this a target-triple enum?
	/// The target architecture for a plugin
	pub arch: String,
	/// The URL of the archive file to download containing the plugin executable artifact and
	/// plugin manifest.
	pub url: Url,
	/// Contains info about what algorithm was used to hash the archive and what the expected
	/// digest is
	pub hash: HashWithDigest,
	/// Defines how to handle decompressing the downloaded plugin archive
	pub compress: Compress,
	/// Describes the size of the downloaded artifact, used to validate the download was
	/// successful, makes it more difficult for an attacker to distribute malformed artifacts
	pub size: Size,
}

/// Returns the first successful node that can be parsed into T, if there is one
fn extract_data<T>(nodes: &[KdlNode]) -> Option<T>
where
	T: ParseKdlNode,
{
	for node in nodes {
		if let Some(val) = T::parse_node(node) {
			return Some(val);
		}
	}
	None
}

impl ParseKdlNode for DownloadManifestEntry {
	fn kdl_key() -> &'static str {
		"plugin"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let version = node.get("version")?;
		let version = match version.value() {
			KdlValue::String(version) => version.to_string(),
			_ => return None,
		};

		let arch = node.get("arch")?;
		let arch = match arch.value() {
			KdlValue::String(arch) => arch.to_string(),
			_ => return None,
		};

		// there should be one child for each plugin and it should contain the url, hash, compress
		// and size information
		let nodes = node.children()?.nodes();

		// extract the url, hash, compress and size from the child
		let url: Url = extract_data(nodes)?;
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
	entries: Vec<DownloadManifestEntry>,
}

impl DownloadManifest {
	pub fn iter(&self) -> impl Iterator<Item = &DownloadManifestEntry> {
		self.entries.iter()
	}

	pub fn len(&self) -> usize {
		self.entries.len()
	}
}

impl FromStr for DownloadManifest {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let document = KdlDocument::from_str(s)
			.map_err(|e| hc_error!("Error parsing download manifest file: {e}"))?;
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
	use kdl::KdlDocument;
	use std::str::FromStr;

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
		let url = "https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz";
		let node = KdlNode::from_str(format!(r#"url "{}""#, url).as_str()).unwrap();
		assert_eq!(Url::parse_node(&node).unwrap(), Url(url.to_string()));
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
			version: version.to_string(),
			arch: arch.to_string(),
			url: Url(url.to_string()),
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
				version: "0.1.0".to_owned(),
				arch: "aarch64-apple-darwin".to_owned(),
				url: Url::new("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz".to_owned()),
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
				version: "0.1.0".to_owned(),
				arch: "x86_64-apple-darwin".to_owned(),
				url: Url::new("https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz".to_owned()),
				hash: HashWithDigest::new(HashAlgorithm::Sha256, "ddb8c6d26dd9a91e11c99b3bd7ee2b9585aedac6e6df614190f1ba2bfe86dc19".to_owned()),
                compress: Compress::new(ArchiveFormat::TarXz),
                size: Size::new(3_183_768)
			},
		    entries_iter.next().unwrap()
        );
	}
}
