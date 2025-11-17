// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use pathbuf::pathbuf;
use tonic_prost_build::configure;

fn main() -> Result<()> {
	// Compile the Hipcheck gRPC protocol spec to an .rs file
	let root = env!("CARGO_MANIFEST_DIR");

	let protos = vec![pathbuf![root, "proto", "hipcheck", "v1", "hipcheck.proto"]];
	let includes = vec![pathbuf![root, "proto"]];

	configure().compile_protos(&protos, &includes)?;

	// Make the target available as a compile-time env var for plugin arch
	// resolution
	println!(
		"cargo:rustc-env=TARGET={}",
		std::env::var("TARGET").unwrap()
	);
	println!("cargo:rerun-if-changed-env=TARGET");

	Ok(())
}
