// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use pathbuf::pathbuf;
use tonic_build::compile_protos;

fn main() -> Result<()> {
	// Compile the Hipcheck gRPC protocol spec to an .rs file
	let root = env!("CARGO_MANIFEST_DIR");
	let path = pathbuf![root, "proto", "hipcheck", "v1", "hipcheck.proto"];
	compile_protos(path)?;

	// Make the target available as a compile-time env var for plugin arch
	// resolution
	println!(
		"cargo:rustc-env=TARGET={}",
		std::env::var("TARGET").unwrap()
	);
	println!("cargo:rerun-if-changed-env=TARGET");

	Ok(())
}
