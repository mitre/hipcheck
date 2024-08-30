use std::{
	fs::File,
	io::{Read, Seek, Write},
	path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use tar::Archive;
use url::Url;
use xz2::read::XzDecoder;
use zip_extensions::{zip_extract, ZipArchiveExtensions};

use crate::error::Error;
use crate::hc_error;
use crate::plugin::{ArchiveFormat, HashAlgorithm, HashWithDigest};
use crate::util::http::agent::agent;

/// download a plugin, verify its size and hash
pub fn download_plugin(
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
pub fn extract_plugin(
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
