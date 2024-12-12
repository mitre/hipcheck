// SPDX-License-Identifier: Apache-2.0

use crate::{
	cache::plugin::HcPluginCache,
	error::{Context, Error},
	hc_error,
	plugin::{
		download_manifest::DownloadManifestEntry, get_current_arch, try_get_bin_for_entrypoint,
		ArchiveFormat, DownloadManifest, HashAlgorithm, HashWithDigest, PluginId, PluginManifest,
	},
	policy::policy_file::{ManifestLocation, PolicyPlugin},
	util::{fs::file_sha256, http::agent::agent},
};
use flate2::read::GzDecoder;
use fs_extra::{dir::remove, file::write_all};
use pathbuf::pathbuf;
use std::{
	collections::HashSet,
	fs::{read_dir, rename, DirEntry, File},
	io::{Read, Write},
	path::{Path, PathBuf},
	str::FromStr,
};
use tar::Archive;
use url::Url;
use xz2::read::XzDecoder;

/// determine all of the plugins that need to be run and locate download them, if they do not exist
pub fn retrieve_plugins(
	policy_plugins: &[PolicyPlugin],
	plugin_cache: &HcPluginCache,
) -> Result<HashSet<PluginId>, Error> {
	#[cfg(feature = "print-timings")]
	let _0 = crate::benchmarking::print_scope_time!("retrieve plugins");

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

	log::debug!(
		"Retrieving Plugin ID {} from {:?}",
		plugin_id,
		manifest_location
	);

	let plugin_manifest = match manifest_location {
		Some(ManifestLocation::Url(plugin_url)) => {
			retrieve_plugin_from_network(plugin_id.clone(), plugin_url, plugin_cache, false)?
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
	force: bool,
) -> Result<PluginManifest, Error> {
	// Use existing cache entry if not force
	let target_manifest = plugin_cache.plugin_kdl(&plugin_id);
	if target_manifest.is_file() && !force {
		log::debug!("Using existing entry in cache for {}", &plugin_id);
		return PluginManifest::from_file(target_manifest);
	}

	let current_arch = get_current_arch();
	let version = plugin_id.version();
	let download_manifest = retrieve_download_manifest(plugin_url)?;
	for entry in &download_manifest.entries {
		if entry.arch == current_arch && version == &entry.version {
			return download_and_unpack_plugin(entry, plugin_id, plugin_cache);
		}
	}
	Err(hc_error!(
		"Could not find download manifest entry for arch '{}' with version '{}'",
		current_arch,
		version.0
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

	let curr_entrypoint = plugin_manifest.get_entrypoint_for(&current_arch)?;

	if let Some(curr_bin) = try_get_bin_for_entrypoint(&curr_entrypoint).0 {
		// Only do copy if using a path to a binary instead of a PATH-based resolution (i.e.
		// `docker _`. We wouldn't want to copy the `docker` binary in this case
		if std::fs::exists(curr_bin)? {
			let original_entrypoint = plugin_manifest
				.update_entrypoint(&current_arch, plugin_cache.plugin_download_dir(&plugin_id))?;

			let new_entrypoint = plugin_manifest.get_entrypoint_for(&current_arch)?;
			// unwrap is safe here, we just updated the entrypoint for current arch
			let new_bin = try_get_bin_for_entrypoint(&new_entrypoint).0.unwrap();

			// path where the binary for this plugin will get cached
			let binary_cache_location = plugin_cache.plugin_download_dir(&plugin_id).join(new_bin);

			// if on windows, first check if we can skip copying. this is because windows won't let
			// you overwrite a plugin binary that is currently in use (such as when another hc
			// instance is already running)
			if !cfg!(target_os = "windows")
				|| file_sha256(&original_entrypoint)?
					!= file_sha256(&binary_cache_location).unwrap_or_default()
			{
				// @Note - sneaky potential for unexpected behavior if we write local plugin manifest
				// to a cache dir that already included a remote download
				//
				// Due to an issue that arises on macOS when copying a binary over a running copy of a binary, we copy the
				// file to a temp file, then move it, rather than copying directly to its source
				//
				// See: https://forums.developer.apple.com/forums/thread/126187
				let tmp_file = tempfile::NamedTempFile::new()?;
				std::fs::copy(&original_entrypoint, tmp_file.path())?;
				std::fs::rename(tmp_file, binary_cache_location)?;
			}
		}
	}

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
		true,
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
	delete_bundle: bool,
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

	let bundle_path_name = bundle_path.file_name().unwrap().to_string_lossy();
	let extension = format!(".{}", archive_format);

	// If extracting archive caused a dir with the same name as the archive to be created, copy
	// the contents of that dir up a level
	if let Some(subdir_name) = bundle_path_name.strip_suffix(&extension) {
		let extract_subdir = pathbuf![extract_dir, subdir_name];

		// Also does implicit exists() check
		if extract_subdir.is_dir() {
			for extracted_content in read_dir(&extract_subdir)? {
				let extracted_content = extracted_content?;
				move_to_extract_dir(extract_dir, &extracted_content)?;
			}
			std::fs::remove_dir_all(extract_subdir)
				.context("Failed to clean up plugin's extracted subdir")?;
		}
	}

	if delete_bundle {
		std::fs::remove_file(bundle_path)?;
	}

	Ok(())
}

fn move_to_extract_dir(extract_dir: &Path, entry: &DirEntry) -> Result<(), Error> {
	let remaining_path = entry
		.path()
		.components()
		.last()
		.ok_or_else(|| hc_error!("no last component: {}", entry.path().display()))
		.map(|component| {
			let path: &Path = component.as_ref();
			path.to_path_buf()
		})?;

	let new_path = pathbuf![extract_dir, &remaining_path];
	rename(entry.path(), new_path)?;
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
