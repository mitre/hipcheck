use crate::cli::SetupArgs;
use crate::error::Result;
use crate::hc_error;
use crate::http::agent;
use regex::Regex;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tar::Archive;
use xz2::read::XzDecoder;

static R_HC_SOURCE: OnceLock<Regex> = OnceLock::new();

fn get_source_regex<'a>() -> &'a Regex {
	R_HC_SOURCE.get_or_init(|| Regex::new("^hipcheck-[a-z0-9_]+-[a-z0-9_]+-[a-z0-9_]+").unwrap())
}

#[derive(Debug, Clone)]
pub struct SetupSourcePath {
	pub path: SourceType,
	delete: bool,
}
impl SetupSourcePath {
	pub fn cleanup(&self) {
		use SourceType::*;
		if self.delete {
			let _res = match &self.path {
				Dir(p) => std::fs::remove_dir_all(p),
				Tar(p) | Zip(p) => std::fs::remove_file(p),
			};
		}
	}
	// Convert to a SourceType::Dir
	pub fn try_unpack(self) -> Result<SetupSourcePath> {
		use SourceType::*;
		let (new_path, delete) = match self.path.clone() {
			Dir(p) => (p, self.delete),
			// For tars and zips, we have to provide the decompressor with the parent dir,
			//  which will produce a directory with the same name as the archive minus the
			//  file extension. We return the name of that new directory.
			Tar(p) => {
				let new_fname: &str = p
					.file_name()
					.ok_or(hc_error!("malformed file name"))?
					.to_str()
					.ok_or(hc_error!("failed to convert tar file name to utf8"))?
					.strip_suffix(".tar.xz")
					.ok_or(hc_error!("tar file with improper extension"))?;
				let tgt_p = p.with_file_name(new_fname);
				let parent_p = PathBuf::from(tgt_p.as_path().parent().unwrap());
				let tar_gz = File::open(p)?;
				let mut archive = Archive::new(XzDecoder::new(tar_gz));
				archive.unpack(parent_p.as_path())?;
				self.cleanup();
				(tgt_p, true)
			}
			Zip(p) => {
				let new_fname: &str = p
					.file_stem()
					.ok_or(hc_error!("malformed .zip file name"))?
					.to_str()
					.ok_or(hc_error!(".zip file not utf8"))?;
				let tgt_p = p.with_file_name(new_fname);
				let parent_p = PathBuf::from(tgt_p.as_path().parent().unwrap());
				let mut archive = zip::ZipArchive::new(File::open(p)?)?;
				archive.extract(parent_p.as_path())?;
				self.cleanup();
				(tgt_p, true)
			}
		};
		Ok(SetupSourcePath {
			path: SourceType::Dir(new_path),
			delete,
		})
	}
}

#[derive(Debug, Clone)]
pub enum SourceType {
	Dir(PathBuf),
	Tar(PathBuf),
	Zip(PathBuf),
}
impl TryFrom<&Path> for SourceType {
	type Error = crate::error::Error;
	fn try_from(value: &Path) -> Result<SourceType> {
		use SourceType::*;
		let source_regex = get_source_regex();
		let file_name = value
			.file_name()
			.ok_or(hc_error!("path without a file name"))?
			.to_str()
			.ok_or(hc_error!("file name not valid utf8"))?;
		if !source_regex.is_match(file_name) {
			return Err(hc_error!("file does not match regex"));
		}
		if value.is_dir() {
			return Ok(Dir(PathBuf::from(value)));
		}
		if file_name.ends_with(".tar.xz") {
			return Ok(Tar(PathBuf::from(value)));
		}
		if file_name.ends_with(".zip") {
			return Ok(Zip(PathBuf::from(value)));
		}
		Err(hc_error!("unknown source type"))
	}
}

pub fn search_dir_for_source(path: &Path) -> Option<SourceType> {
	for entry in walkdir::WalkDir::new(path)
		.max_depth(1)
		.into_iter()
		.flatten()
	{
		if let Ok(source) = SourceType::try_from(entry.path()) {
			return Some(source);
		}
	}
	None
}

pub fn try_get_source_path_from_path(path: &Path) -> Option<SourceType> {
	// First treat path as a direct source dir / archive
	if let Ok(source) = SourceType::try_from(path) {
		Some(source)
	}
	// If that failed and path is a dir, see if we can find it underneath
	else if path.is_dir() {
		search_dir_for_source(path)
	} else {
		None
	}
}

// Search for a dir or archive matching the format of our `cargo-dist` release bundle. Search
// hierarchy is 1) source cmdline arg, 2) current dir 3) platform downloads dir. If nothing
// found and user did not forbid internet access, we can then pull from github release page.
pub fn try_resolve_source_path(args: &SetupArgs) -> Result<SetupSourcePath> {
	let try_dirs: Vec<Option<PathBuf>> = vec![
		args.source.clone(),
		std::env::current_dir().ok(),
		dirs::download_dir(),
	];
	for try_dir in try_dirs.into_iter().flatten() {
		if let Some(bp) = try_get_source_path_from_path(try_dir.as_path()) {
			return Ok(SetupSourcePath {
				path: bp,
				delete: false,
			});
		}
	}
	// If allowed by user, download from github
	if !args.offline {
		// Since we're just getting the conf/target dir from here, we don't
		// technically need to grab the right version
		let f_name: &str = "hipcheck-x86_64-unknown-linux-gnu.tar.xz";
		let remote = format!(
			"https://github.com/mitre/hipcheck/releases/download/hipcheck-v{}/{}",
			env!("CARGO_PKG_VERSION"),
			f_name
		);

		let mut out_file = File::create(f_name)?;
		let agent = agent::agent();

		println!("Downloading Hipcheck release from remote.");
		let resp = agent.get(remote.as_str()).call()?;
		std::io::copy(&mut resp.into_reader(), &mut out_file)?;

		return Ok(SetupSourcePath {
			path: SourceType::Tar(std::fs::canonicalize(f_name)?),
			delete: true,
		});
	}
	Err(hc_error!("could not find suitable source file"))
}

pub fn resolve_and_transform_source(args: &SetupArgs) -> Result<SetupSourcePath> {
	try_resolve_source_path(args)?.try_unpack()
}
