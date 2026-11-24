use anyhow::{Result, anyhow};
use std::path::PathBuf;

/// Get the path to the root `Cargo.toml` of the workspace.
pub fn get_workspace_path() -> Result<PathBuf> {
	let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.ok_or_else(|| anyhow!("failed to get workspace root dir"))?
		.to_owned();
	Ok(root)
}
