// Adapted from the build script used by cargo-nextest [1]. Used under the
// terms of the MIT license.
//
// [1]: https://github.com/nextest-rs/nextest/blob/main/cargo-nextest/build.rs

use std::ops::Not as _;
use std::path::PathBuf;
use std::process::Command;

// This script determines the current Git commit hash and date, and sets
// environment variables for use when building `nv-server`, specifically so
// the server can report commit hash information when reporting its own
// version.
//
// Input environment variables:
//
// - CFG_OMIT_COMMIT_HASH: Will cause the script to *not* determine the current
//   Git commit hash.
//
// Output environment variables:
//
// - NV_BUILD_COMMIT_HASH: The full Git commit hash.
// - NV_BUILD_COMMIT_SHORT_HASH: The abbreviated Git commit hash.
// - NV_BUILD_COMMIT_DATE: The date of the Git commit.

fn main() {
	add_commit_info_to_env();
}

fn add_commit_info_to_env() {
	println!("cargo:rerun-if-env-changed=CFG_OMIT_COMMIT_HASH");

	if std::env::var_os("CFG_OMIT_COMMIT_HASH").is_some() {
		return;
	}

	if let Some(info) = CommitInfo::get() {
		println!("cargo:rustc-env=NV_BUILD_COMMIT_HASH={}", info.hash);
		println!(
			"cargo:rustc-env=NV_BUILD_COMMIT_SHORT_HASH={}",
			info.short_hash,
		);
		println!("cargo:rustc-env=NV_BUILD_COMMIT_DATE={}", info.date);
	}
}

struct CommitInfo {
	hash: String,
	short_hash: String,
	date: String,
}

impl CommitInfo {
	fn get() -> Option<Self> {
		Self::from_git()
	}

	fn from_git() -> Option<Self> {
		// nv-server is two levels down from the root of the repository.
		if path_to_git().exists().not() {
			return None;
		}

		let output = match Command::new("git")
			.arg("log")
			.arg("-1")
			.arg("--date=short")
			.arg("--format=%H %h %cd")
			.arg("--abbrev=9")
			.output()
		{
			Ok(output) if output.status.success() => output,
			_ => return None,
		};

		let stdout = String::from_utf8(output.stdout).expect("git output is ASCII");
		Self::from_string(&stdout)
	}

	fn from_string(s: &str) -> Option<Self> {
		let mut parts = s.split_whitespace().map(ToOwned::to_owned);

		Some(Self {
			hash: parts.next()?,
			short_hash: parts.next()?,
			date: parts.next()?,
		})
	}
}

// Normally I'd use the `pathbuf` crate to do this, but I'm keeping this
// build script dependency-free to minimize the impact it has on build times.
fn path_to_git() -> PathBuf {
	let mut p = PathBuf::new();
	p.push("..");
	p.push("..");
	p.push(".git");
	p
}
