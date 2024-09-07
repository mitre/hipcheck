// SPDX-License-Identifier: Apache-2.0

use super::{DownloadManifest, PluginId, PluginName};
use crate::{
	cache::plugin_cache::HcPluginCache,
	config::MITRE_PUBLISHER,
	error::Error,
	hc_error,
	plugin::{ArchiveFormat, HashAlgorithm, HashWithDigest},
	policy::policy_file::{PolicyPlugin, PolicyPluginName},
	util::http::agent::agent,
};
use flate2::read::GzDecoder;
use std::{
	collections::HashSet,
	fs::File,
	io::{Read, Seek, Write},
	path::{Path, PathBuf},
};
use tar::Archive;
use url::Url;
use xz2::read::XzDecoder;
use zip_extensions::{zip_extract, ZipArchiveExtensions};

/// download, verify and unpack all of the plugins specified in a Policy file, as well as all of their dependencies
pub fn retrieve_plugins(
	policy_plugins: &[PolicyPlugin],
	plugin_cache: &HcPluginCache,
) -> Result<HashSet<PluginId>, Error> {
	let mut download_plugins = HashSet::new();

	for policy_plugin in policy_plugins.iter() {
		/// TODO: while the legacy passes are still integrated in the main codebase, we skip downloading them!
		if policy_plugin.name.publisher.0.as_str() == MITRE_PUBLISHER {
			continue;
		}

		match &policy_plugin.manifest {
			Some(url) => {
				let download_manifest = DownloadManifest::from_network(url).map_err(|e| {
					hc_error!(
						"Error [{}] retrieving plugin manifest for {}",
						e,
						&policy_plugin.name
					)
				})?;
				download_manifest.download_and_unpack_all_plugins(
					plugin_cache,
					&policy_plugin.name.publisher,
					&policy_plugin.name.name,
					&policy_plugin.version,
					&mut download_plugins,
				)?;
			}
			None => return Err(hc_error!("No manifest provided for {}", policy_plugin.name)),
		}
	}
	Ok(download_plugins)
}

/// download a plugin, verify its size and hash
pub(super) fn download_plugin(
	url: &Url,
	download_dir: &Path,
	expected_size: u64,
	expected_hash_with_digest: &HashWithDigest,
) -> Result<PathBuf, Error> {
	// retrieve archive
	let agent = agent();
	let response = agent
		.get(url.as_str())
		.call()
		.map_err(|e| hc_error!("Error [{}] retrieving download manifest {}", e, url))?;
	let error_code = response.status();
	if error_code != 200 {
		return Err(hc_error!(
			"HTTP error code {} when retrieving {}",
			error_code,
			url
		));
	}

	// extract bytes from response
	// preallocate 10 MB to cut down on number of allocations needed
	let mut contents = Vec::with_capacity(10 * 1024 * 1024);
	let amount_read = response
		.into_reader()
		.read_to_end(&mut contents)
		.map_err(|e| hc_error!("Error [{}] reading download into buffer", e))?;
	contents.truncate(amount_read);

	// verify size of download
	if expected_size != amount_read as u64 {
		return Err(hc_error!(
			"File size mismatch, Expected {} B, Found {} B",
			expected_size,
			amount_read
		));
	}

	// verify hash
	let actual_hash = match expected_hash_with_digest.hash_algorithm {
		HashAlgorithm::Sha256 => sha256::digest(&contents),
		HashAlgorithm::Blake3 => blake3::hash(&contents).to_string(),
	};
	if actual_hash != expected_hash_with_digest.digest {
		return Err(hc_error!(
			"Plugin hash mismatch. Expected [{}], Received [{}]",
			actual_hash,
			expected_hash_with_digest.digest
		));
	}

	let filename = url.path_segments().unwrap().last().unwrap();
	std::fs::create_dir_all(download_dir).map_err(|e| {
		hc_error!(
			"Error [{}] creating download directory {}",
			e,
			download_dir.to_string_lossy()
		)
	})?;
	let output_path = Path::new(download_dir).join(filename);
	let mut file = File::create(&output_path).map_err(|e| {
		hc_error!(
			"Error [{}] creating file: {}",
			e,
			output_path.to_string_lossy()
		)
	})?;
	file.write_all(&contents).map_err(|e| {
		hc_error!(
			"Error [{}] writing to file: {}",
			e,
			output_path.to_string_lossy()
		)
	})?;

	Ok(output_path)
}

/// Extract a bundle located at `bundle_path` into `extract_dir` by applying the specified `ArchiveFormat` extractions
pub(super) fn extract_plugin(
	bundle_path: &Path,
	extract_dir: &Path,
	archive_format: ArchiveFormat,
) -> Result<(), Error> {
	let file = File::open(bundle_path).map_err(|e| {
		hc_error!(
			"Error [{}] opening file {}",
			e,
			bundle_path.to_string_lossy()
		)
	})?;

	// perform decompression, if necessary, then unarchive
	match archive_format {
		ArchiveFormat::TarXz => {
			let decoder = XzDecoder::new(file);
			let mut archive = Archive::new(decoder);
			archive.unpack(extract_dir).map_err(|e| {
				hc_error!("Error [{}] extracting {}", e, bundle_path.to_string_lossy())
			})?;
		}
		ArchiveFormat::TarGz => {
			let decoder = GzDecoder::new(file);
			let mut archive = Archive::new(decoder);
			archive.unpack(extract_dir).map_err(|e| {
				hc_error!("Error [{}] extracting {}", e, bundle_path.to_string_lossy())
			})?;
		}
		ArchiveFormat::TarZst => {
			let decoder = zstd::Decoder::new(file).unwrap();
			let mut archive = Archive::new(decoder);
			archive.unpack(extract_dir).map_err(|e| {
				hc_error!("Error [{}] extracting {}", e, bundle_path.to_string_lossy())
			})?;
		}
		ArchiveFormat::Tar => {
			let mut archive = Archive::new(file);
			archive.unpack(extract_dir).map_err(|e| {
				hc_error!("Error [{}] extracting {}", e, bundle_path.to_string_lossy())
			})?;
		}
		ArchiveFormat::Zip => {
			let mut archive = zip::ZipArchive::new(file).unwrap();
			archive.extract(extract_dir).map_err(|e| {
				hc_error!("Error [{}] extracting {}", e, bundle_path.to_string_lossy())
			})?;
		}
	};

	Ok(())
}
