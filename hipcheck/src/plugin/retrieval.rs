// SPDX-License-Identifier: Apache-2.0

use crate::{
	cache::plugin::HcPluginCache,
	error::{Context, Error},
	hc_error,
	plugin::{
		ArchiveFormat, DownloadManifest, HashAlgorithm, HashWithDigest, PluginId,
		PluginIdVersionRange, PluginManifest, PluginVersion,
		download_manifest::DownloadManifestEntry, get_current_arch, try_get_bin_for_entrypoint,
	},
	policy::policy_file::{ManifestLocation, PolicyPlugin},
	util::{fs::file_sha256, http::agent::agent},
};
use flate2::read::GzDecoder;
use fs_extra::{dir::remove, file::write_all};
use pathbuf::pathbuf;
use std::{
	collections::HashSet,
	fs::{DirEntry, File, read_dir, rename},
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

	let num_plugins = policy_plugins.len();
	log::info!("Retrieving {} plugins", num_plugins);

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
	plugin_id: PluginIdVersionRange,
	manifest_location: &Option<ManifestLocation>,
	plugin_cache: &HcPluginCache,
	required_plugins: &mut HashSet<PluginId>,
) -> Result<(), Error> {
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
	let updated_plugin_id = PluginId::new(
		plugin_manifest.publisher.clone(),
		plugin_manifest.name.clone(),
		plugin_manifest.version.clone(),
	);

	if required_plugins.contains(&updated_plugin_id) {
		return Ok(());
	}

	required_plugins.insert(updated_plugin_id);
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
	plugin_id: PluginIdVersionRange,
	plugin_url: &Url,
	plugin_cache: &HcPluginCache,
	force: bool,
) -> Result<PluginManifest, Error> {
	// If exact plugin version was provided, use existing cache entry if not force
	if plugin_id
		.version()
		.version_req
		.comparators
		.first()
		.is_some_and(|comp| comp.op == semver::Op::Exact)
	{
		let version_req_syntax_string = plugin_id.version().version_req.to_string();
		let version_syntax_string = version_req_syntax_string
			.chars()
			.skip(1)
			.collect::<String>();
		let plugin_version = PluginVersion::new(version_syntax_string.as_str())?;
		let plugin_id_for_cache = PluginId::new(
			plugin_id.publisher().clone(),
			plugin_id.name().clone(),
			plugin_version.clone(),
		);
		let target_manifest = plugin_cache.plugin_kdl(&plugin_id_for_cache);
		if target_manifest.is_file() && !force {
			log::debug!("Using existing entry in cache for {}", &plugin_id_for_cache);
			return PluginManifest::from_file(target_manifest);
		}
	}

	let download_manifest = retrieve_download_manifest(plugin_url)?;
	let entry = select_plugin_version(&plugin_id, &download_manifest)?;
	let updated_plugin_id = PluginId::new(
		plugin_id.publisher().clone(),
		plugin_id.name().clone(),
		entry.version.clone(),
	);
	download_and_unpack_plugin(entry, updated_plugin_id, plugin_cache)
}

fn select_plugin_version<'a>(
	plugin_id: &'a PluginIdVersionRange,
	download_manifest: &'a DownloadManifest,
) -> Result<&'a DownloadManifestEntry, Error> {
	let current_arch = get_current_arch();
	let version = plugin_id.version();
	let latest_version_entry = download_manifest
		.entries
		.iter()
		.filter(|entry| {
			entry.arch == current_arch && version.version_req.matches(&entry.version.version)
		})
		.max_by(|a, b| a.version.version.cmp(&b.version.version));

	if let Some(entry) = latest_version_entry {
		Ok(entry)
	} else {
		Err(hc_error!(
			"Could not find download manifest entry for '{}' [{}]",
			plugin_id.to_policy_file_plugin_identifier(),
			current_arch,
		))
	}
}

/// retrieves a plugin from the local filesystem by copying its `plugin.kdl` and `entrypoint` binary to the plugin_cache
fn retrieve_local_plugin(
	plugin_id: PluginIdVersionRange,
	plugin_manifest_path: &PathBuf,
	plugin_cache: &HcPluginCache,
) -> Result<PluginManifest, Error> {
	let mut plugin_manifest = PluginManifest::from_file(plugin_manifest_path)?;
	let current_arch = get_current_arch();
	let plugin_version = plugin_manifest.version.clone();
	let updated_plugin_id = PluginId::new(
		plugin_id.publisher().clone(),
		plugin_id.name().clone(),
		plugin_version,
	);

	let download_dir = plugin_cache.plugin_download_dir(&updated_plugin_id);
	std::fs::create_dir_all(&download_dir).map_err(|e| {
		hc_error!(
			"Error [{}] creating download directory {}",
			e,
			download_dir.to_string_lossy()
		)
	})?;

	let curr_entrypoint = plugin_manifest.get_entrypoint_for(&current_arch)?;

	if let Some(curr_bin) = try_get_bin_for_entrypoint(&curr_entrypoint).0 {
		// Only do copy if using a path to a binary instead of a PATH-based resolution (i.e.
		// `docker _`. We wouldn't want to copy the `docker` binary in this case
		if std::fs::exists(curr_bin)? {
			let original_entrypoint = plugin_manifest.update_entrypoint(
				&current_arch,
				plugin_cache.plugin_download_dir(&updated_plugin_id),
			)?;

			let new_entrypoint = plugin_manifest.get_entrypoint_for(&current_arch)?;
			// unwrap is safe here, we just updated the entrypoint for current arch
			let new_bin = try_get_bin_for_entrypoint(&new_entrypoint).0.unwrap();

			// path where the binary for this plugin will get cached
			let binary_cache_location = plugin_cache
				.plugin_download_dir(&updated_plugin_id)
				.join(new_bin);

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

	let plugin_kdl_path = plugin_cache.plugin_kdl(&updated_plugin_id);
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

	let filename = url.path_segments().unwrap().next_back().unwrap();
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
		.next_back()
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

#[cfg(test)]
mod test {

	use super::*;
	use crate::plugin::{PluginName, PluginPublisher, PluginVersion, PluginVersionReq};
	use std::str::FromStr;

	#[test]

	fn test_versionreq_to_version_above() {
		let plugin_id = &PluginIdVersionRange::new(
			PluginPublisher("mitre".to_string()),
			PluginName("git".to_string()),
			PluginVersionReq::new("^0.1").unwrap(),
		);
		let download_manifest = &DownloadManifest::from_str(
			r#"plugin version="0.1.0" arch="aarch64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-aarch64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="66ece652f44ff47ad8f5ab45d4de1cde9f35fdc15c7c43b89aebb7203d277e8e"
				compress format="tar.xz"
				size bytes=1183768
			}

			plugin version="0.1.0" arch="x86_64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="b3c7e463c701988bdb568bf520f71ef7e6fa21b5c5bc68449a53a42e59b575ab"
				compress format="tar.xz"
				size bytes=1289520
			}

			plugin version="0.1.0" arch="x86_64-pc-windows-msvc" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-pc-windows-msvc.zip"
				hash alg="SHA256" digest="aea41cd5b91c79432baf0b00b5f85fef33ecaefcc2dfe24028b5d7dbf6e0365c"
				compress format="zip"
				size bytes=4322356
			}

			plugin version="0.1.0" arch="x86_64-unknown-linux-gnu" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-unknown-linux-gnu.tar.xz"
				hash alg="SHA256" digest="dfe3f12d5c4b8c9397f69aad40db27978f5777dd16440380e8f35c6ddf65f485"
				compress format="tar.xz"
				size bytes=1382112
			}

			plugin version="0.1.2" arch="aarch64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-aarch64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="c4285a08a829a18b68e5c678dff00333e1150c00a94c7326eba3970a4519b733"
				compress format="tar.xz"
				size bytes=1156136
			}

			plugin version="0.1.2" arch="x86_64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="0689448a70e0c01ac62428113760eeb717756ef2b74079c4092996ed2e5d9832"
				compress format="tar.xz"
				size bytes=1256796
			}

			plugin version="0.1.2" arch="x86_64-pc-windows-msvc" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-pc-windows-msvc.zip"
				hash alg="SHA256" digest="896ae92d03e6452fa25bcec8c54027431d533d41aaf7c0ef954cca0534fae14b"
				compress format="zip"
				size bytes=4103220
			}

			plugin version="0.1.2" arch="x86_64-unknown-linux-gnu" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-unknown-linux-gnu.tar.xz"
				hash alg="SHA256" digest="f3d74b2d66923413c69826a733e268509e00015044bb4c93a3dbcfbf281ff365"
				compress format="tar.xz"
				size bytes=1339456
			}"#
					).unwrap();
		let entry = select_plugin_version(plugin_id, download_manifest).unwrap();
		let expected_version = PluginVersion::new("0.1.2").unwrap();
		println!("Entry Version: {:}", entry.version.version);
		assert_eq!(entry.version.version, expected_version.version);
	}

	#[test]

	fn test_versionreq_to_version_below() {
		let plugin_id = &PluginIdVersionRange::new(
			PluginPublisher("mitre".to_string()),
			PluginName("git".to_string()),
			PluginVersionReq::new("<=0.3.1").unwrap(),
		);
		let download_manifest = &DownloadManifest::from_str(
			r#"plugin version="0.1.55" arch="aarch64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-aarch64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="66ece652f44ff47ad8f5ab45d4de1cde9f35fdc15c7c43b89aebb7203d277e8e"
				compress format="tar.xz"
				size bytes=1183768
			}

			plugin version="0.1.55" arch="x86_64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="b3c7e463c701988bdb568bf520f71ef7e6fa21b5c5bc68449a53a42e59b575ab"
				compress format="tar.xz"
				size bytes=1289520
			}

			plugin version="0.1.55" arch="x86_64-pc-windows-msvc" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-pc-windows-msvc.zip"
				hash alg="SHA256" digest="aea41cd5b91c79432baf0b00b5f85fef33ecaefcc2dfe24028b5d7dbf6e0365c"
				compress format="zip"
				size bytes=4322356
			}

			plugin version="0.1.55" arch="x86_64-unknown-linux-gnu" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-unknown-linux-gnu.tar.xz"
				hash alg="SHA256" digest="dfe3f12d5c4b8c9397f69aad40db27978f5777dd16440380e8f35c6ddf65f485"
				compress format="tar.xz"
				size bytes=1382112
			}

			plugin version="0.3.22" arch="aarch64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-aarch64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="c4285a08a829a18b68e5c678dff00333e1150c00a94c7326eba3970a4519b733"
				compress format="tar.xz"
				size bytes=1156136
			}

			plugin version="0.3.22" arch="x86_64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="0689448a70e0c01ac62428113760eeb717756ef2b74079c4092996ed2e5d9832"
				compress format="tar.xz"
				size bytes=1256796
			}

			plugin version="0.3.22" arch="x86_64-pc-windows-msvc" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-pc-windows-msvc.zip"
				hash alg="SHA256" digest="896ae92d03e6452fa25bcec8c54027431d533d41aaf7c0ef954cca0534fae14b"
				compress format="zip"
				size bytes=4103220
			}

			plugin version="0.3.22" arch="x86_64-unknown-linux-gnu" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-unknown-linux-gnu.tar.xz"
				hash alg="SHA256" digest="f3d74b2d66923413c69826a733e268509e00015044bb4c93a3dbcfbf281ff365"
				compress format="tar.xz"
				size bytes=1339456
			}"#
					).unwrap();
		let entry = select_plugin_version(plugin_id, download_manifest).unwrap();
		let expected_version = PluginVersion::new("0.1.55").unwrap();
		println!("Entry Version: {:}", entry.version.version);
		assert_eq!(entry.version.version, expected_version.version);
	}

	#[test]
	fn test_versionreq_to_version_equals() {
		let plugin_id = &PluginIdVersionRange::new(
			PluginPublisher("mitre".to_string()),
			PluginName("git".to_string()),
			PluginVersionReq::new("=0.2.1").unwrap(),
		);
		let download_manifest = &DownloadManifest::from_str(
			r#"plugin version="0.2.1" arch="aarch64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-aarch64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="66ece652f44ff47ad8f5ab45d4de1cde9f35fdc15c7c43b89aebb7203d277e8e"
				compress format="tar.xz"
				size bytes=1183768
			}

			plugin version="0.2.1" arch="x86_64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="b3c7e463c701988bdb568bf520f71ef7e6fa21b5c5bc68449a53a42e59b575ab"
				compress format="tar.xz"
				size bytes=1289520
			}

			plugin version="0.2.1" arch="x86_64-pc-windows-msvc" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-pc-windows-msvc.zip"
				hash alg="SHA256" digest="aea41cd5b91c79432baf0b00b5f85fef33ecaefcc2dfe24028b5d7dbf6e0365c"
				compress format="zip"
				size bytes=4322356
			}

			plugin version="0.2.1" arch="x86_64-unknown-linux-gnu" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.1.0/binary-x86_64-unknown-linux-gnu.tar.xz"
				hash alg="SHA256" digest="dfe3f12d5c4b8c9397f69aad40db27978f5777dd16440380e8f35c6ddf65f485"
				compress format="tar.xz"
				size bytes=1382112
			}

			plugin version="0.4.8" arch="aarch64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-aarch64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="c4285a08a829a18b68e5c678dff00333e1150c00a94c7326eba3970a4519b733"
				compress format="tar.xz"
				size bytes=1156136
			}

			plugin version="0.4.8" arch="x86_64-apple-darwin" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-apple-darwin.tar.xz"
				hash alg="SHA256" digest="0689448a70e0c01ac62428113760eeb717756ef2b74079c4092996ed2e5d9832"
				compress format="tar.xz"
				size bytes=1256796
			}

			plugin version="0.4.8" arch="x86_64-pc-windows-msvc" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-pc-windows-msvc.zip"
				hash alg="SHA256" digest="896ae92d03e6452fa25bcec8c54027431d533d41aaf7c0ef954cca0534fae14b"
				compress format="zip"
				size bytes=4103220
			}

			plugin version="0.4.8" arch="x86_64-unknown-linux-gnu" {
				url "https://github.com/mitre/hipcheck/releases/download/binary-v0.2.0/binary-x86_64-unknown-linux-gnu.tar.xz"
				hash alg="SHA256" digest="f3d74b2d66923413c69826a733e268509e00015044bb4c93a3dbcfbf281ff365"
				compress format="tar.xz"
				size bytes=1339456
			}"#
					).unwrap();
		let entry = select_plugin_version(plugin_id, download_manifest).unwrap();
		let expected_version = PluginVersion::new("0.2.1").unwrap();
		assert_eq!(entry.version.version, expected_version.version);
	}
}
