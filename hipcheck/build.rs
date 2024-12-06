// SPDX-License-Identifier: Apache-2.0

include!("src/target/types.rs");

use anyhow::Result;
use pathbuf::pathbuf;
use schemars::schema_for;
use std::{fs, path::Path};

fn generate_schemars_for_target_types(out_dir: &Path) -> Result<()> {
	let out_schemars = vec![("target", schema_for!(Target))];
	for (key, schema) in out_schemars {
		let out_path = pathbuf![out_dir, format!("hipcheck_{}_schema.json", key).as_str()];
		fs::write(out_path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();
	}
	Ok(())
}

fn main() -> Result<()> {
	generate_schemars_for_target_types(Path::new("../sdk/schema"))?;

	// Make the target available as a compile-time env var for plugin arch
	// resolution
	println!(
		"cargo:rustc-env=TARGET={}",
		std::env::var("TARGET").unwrap()
	);
	println!("cargo:rerun-if-changed-env=TARGET");

	Ok(())
}
