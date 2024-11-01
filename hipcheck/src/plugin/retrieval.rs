// SPDX-License-Identifier: Apache-2.0

use crate::{
	cache::plugin::HcPluginCache,
	error::Error,
	hc_error,
	plugin::{
		download_manifest::DownloadManifestEntry, ArchiveFormat, DownloadManifest, HashAlgorithm,
		HashWithDigest, PluginId, PluginManifest,
	},
	policy::policy_file::{ManifestLocation, PolicyPlugin},
	util::http::agent::agent,
};
use flate2::read::GzDecoder;
use fs_extra::{dir::remove, file::write_all};
use std::{
	collections::HashSet,
	fs::File,
	io::{Read, Write},
	path::{Path, PathBuf},
	str::FromStr,
};
use tar::Archive;
use url::Url;
use xz2::read::XzDecoder;

use super::get_current_arch;

/// determine all of the plugins that need to be run and locate download them, if they do not exist
pub fn retrieve_plugins(
	policy_plugins: &[PolicyPlugin],
	plugin_cache: &HcPluginCache,
) -> Result<HashSet<PluginId>, Error> {
	let mut required_plugins = HashSet::new();

	for policy_plugin in policy_plugins.iter() {
		retrieve_plugin(
			policy_plugin.get_plugin_id(),
			&policy_plugin.manifest,
			plugin_cache,
			&mut required_plugins,
		)?;
	}
	Ok(required_plugins)
}

fn retrieve_plugin(
	plugin_id: PluginId,
	manifest_location: &Option<ManifestLocation>,
	plugin_cache: &HcPluginCache,
	required_plugins: &mut HashSet<PluginId>,
) -> Result<(), Error> {
	if required_plugins.contains(&plugin_id) {
		return Ok(());
	}
	// TODO: if the plugin.kdl file for the plugin already exists, then should we skip the retrieval process?
	// if plugin_cache.plugin_kdl(&plugin_id).exists()

	log::debug!("Retrieving Plugin ID: {:?}", plugin_id);

	let plugin_manifest = match manifest_location {
		Some(ManifestLocation::Url(plugin_url)) => {
			retrieve_plugin_from_network(plugin_id.clone(), plugin_url, plugin_cache)?
		}
		Some(ManifestLocation::Local(plugin_manifest_path)) => {
			retrieve_local_plugin(plugin_id.clone(), plugin_manifest_path, plugin_cache)?
		}
		None => {
			// in the future, this could attempt to reach a known package registry
			return Err(hc_error!(
				"No manifest specified for {}",
				plugin_id.to_policy_file_plugin_identifier()
			));
		}
	};
	required_plugins.insert(plugin_id);
	for dependency in plugin_manifest.dependencies.0 {
		retrieve_plugin(
			dependency.as_ref().clone(),
			&dependency.manifest,
			plugin_cache,
			required_plugins,
		)?;
	}
	Ok(())
}

fn retrieve_plugin_from_network(
	plugin_id: PluginId,
	plugin_url: &Url,
	plugin_cache: &HcPluginCache,
) -> Result<PluginManifest, Error> {
	let current_arch = get_current_arch();
	let download_manifest = retrieve_download_manifest(plugin_url)?;
	for entry in &download_manifest.entries {
		if entry.arch == current_arch {
			return download_and_unpack_plugin(entry, plugin_id, plugin_cache);
		}
	}
	Err(hc_error!(
		"Could not find download manifest entry for arch '{}'",
		current_arch
	))
}

/// retrieves a plugin from the local filesystem by copying its `plugin.kdl` and `entrypoint` binary to the plugin_cache
fn retrieve_local_plugin(
	plugin_id: PluginId,
	plugin_manifest_path: &PathBuf,
	plugin_cache: &HcPluginCache,
) -> Result<PluginManifest, Error> {
	let download_dir = plugin_cache.plugin_download_dir(&plugin_id);
	std::fs::create_dir_all(&download_dir).map_err(|e| {
		hc_error!(
			"Error [{}] creating download directory {}",
			e,
			download_dir.to_string_lossy()
		)
	})?;

	let mut plugin_manifest = PluginManifest::from_file(plugin_manifest_path)?;
	let current_arch = get_current_arch();

	let original_entrypoint = plugin_manifest
		.update_entrypoint(&current_arch, plugin_cache.plugin_download_dir(&plugin_id))?;

	// @Note - sneaky potential for unexpected behavior if we write local plugin manifest
	// to a cache dir that already included a remote download
	std::fs::copy(
		&original_entrypoint,
		plugin_cache
			.plugin_download_dir(&plugin_id)
			// unwrap is safe here, we just updated the entrypoint for current arch
			.join(plugin_manifest.get_entrypoint(&current_arch).unwrap()),
	)?;

	let plugin_kdl_path = plugin_cache.plugin_kdl(&plugin_id);
	write_all(&plugin_kdl_path, &plugin_manifest.to_kdl_formatted_string()).map_err(|e| {
		hc_error!(
			"Error [{}] writing {}",
			e,
			plugin_kdl_path.to_string_lossy()
		)
	})?;

	Ok(plugin_manifest)
}

/// This function does the following:
/// 1. Download specified plugin for the current arch
/// 1. Verify its size and hash
/// 1. Extract plugin into plugin-specific folder
/// 1. Finds `plugin.kdl` inside plugin-specific folder and parses it
fn download_and_unpack_plugin(
	download_manifest_entry: &DownloadManifestEntry,
	plugin_id: PluginId,
	plugin_cache: &HcPluginCache,
) -> Result<PluginManifest, Error> {
	let download_dir = plugin_cache.plugin_download_dir(&plugin_id);

	let output_path = download_plugin(
		&download_manifest_entry.url,
		download_dir.as_path(),
		download_manifest_entry.size.bytes,
		&download_manifest_entry.hash,
	)
	.map_err(|e| {
		// delete any leftover remnants
		let _ = remove(download_dir.as_path());
		hc_error!(
			"Error [{}] downloading '{}'",
			e,
			&download_manifest_entry.url
		)
	})?;

	extract_plugin(
		output_path.as_path(),
		download_dir.as_path(),
		download_manifest_entry.compress.format,
	)
	.map_err(|e| {
		// delete any leftover remnants
		let _ = remove(download_dir.as_path());
		hc_error!(
			"Error [{}] extracting plugin '{}'",
			e,
			plugin_id.to_policy_file_plugin_identifier(),
		)
	})?;

	PluginManifest::from_file(plugin_cache.plugin_kdl(&plugin_id))
}

/// download a plugin, verify its size and hash
fn download_plugin(
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
fn extract_plugin(
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

/// fetch and deserialize a DownloadManifest from a URL
fn retrieve_download_manifest(url: &Url) -> Result<DownloadManifest, Error> {
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
		.map_err(|e| hc_error!("Error [{}] reading download manifest into buffer", e))?;
	contents.truncate(amount_read);
	let contents = String::from_utf8_lossy(&contents);
	DownloadManifest::from_str(&contents)
}
