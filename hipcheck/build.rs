// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Context as _;
use anyhow::Result;
use std::convert::AsRef;
use std::ffi::OsStr;
use std::iter::IntoIterator;
use std::ops::Not as _;
use std::path::Path;
use std::process::Command;
use which::which;

fn main() -> Result<()> {
	let repo_dir = env!("CARGO_MANIFEST_DIR", "can't find Cargo manifest directory");
	let head = get_head_commit(repo_dir).unwrap_or_default();

	tonic_build::compile_protos("proto/hipcheck.proto")?;

	println!("cargo:rustc-env=HC_HEAD_COMMIT={}", head);
	Ok(())
}

fn get_head_commit<P: AsRef<Path>>(path: P) -> Result<String> {
	fn inner(path: &Path) -> Result<String> {
		let output = GitCommand::for_repo(path, ["rev-parse", "--short", "HEAD"])?
			.output()
			.context("can't get HEAD commit hash")?;

		Ok(output.trim().to_owned())
	}

	inner(path.as_ref())
}

struct GitCommand {
	command: Command,
}

impl GitCommand {
	fn for_repo<I, S>(repo_path: &Path, args: I) -> Result<GitCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		// Init the command.
		let git_path = which("git").context("can't find git command")?;
		let mut command = Command::new(git_path);
		command.args(args);

		// Set the path if necessary
		command.current_dir(repo_path);

		if cfg!(windows) {
			// this method is broken on Windows. See: https://github.com/rust-lang/rust/issues/31259
			//command.env_clear()
		} else {
			command.env_clear();
		};

		Ok(GitCommand { command })
	}

	fn output(&mut self) -> Result<String> {
		let output = self.command.output()?;

		if output.status.success() {
			let output_text = String::from_utf8_lossy(&output.stdout).to_string();
			return Ok(output_text);
		}

		match String::from_utf8(output.stderr) {
			Ok(msg) if msg.is_empty().not() => Err(anyhow!(
				"git failed with message '{}' [status: {}]",
				msg.trim(),
				output.status
			)),
			_ => Err(anyhow!("git failed [status: {}]", output.status)),
		}
	}
}
