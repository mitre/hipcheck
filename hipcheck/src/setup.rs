// SPDX-License-Identifier: Apache-2.0

use crate::error::Result;
use std::io::Write;
use std::{fs::File, path::Path};

static BINARY_TOML: &str = include_str!("../../config/Binary.toml");
static EXEC_KDL: &str = include_str!("../../config/Exec.kdl");
static HIPCHECK_KDL: &str = include_str!("../../config/Hipcheck.kdl");
static HIPCHECK_TOML: &str = include_str!("../../config/Hipcheck.toml");
static LANGS_TOML: &str = include_str!("../../config/Langs.toml");
static ORGS_KDL: &str = include_str!("../../config/Orgs.kdl");
static TYPOS_TOML: &str = include_str!("../../config/Typos.toml");

pub fn write_config_binaries(path: &Path) -> Result<()> {
	std::fs::create_dir_all(path)?;

	let files = [
		("Langs.toml", LANGS_TOML),
		("Typos.toml", TYPOS_TOML),
		("Binary.toml", BINARY_TOML),
		("Exec.kdl", EXEC_KDL),
		("Hipcheck.kdl", HIPCHECK_KDL),
		("Hipcheck.toml", HIPCHECK_TOML),
		("Orgs.kdl", ORGS_KDL),
	];

	for (file_name, content) in &files {
		let file_path = path.join(file_name);
		let mut file = File::create(file_path)?;
		file.write_all(content.as_bytes())?;
	}

	Ok(())
}
