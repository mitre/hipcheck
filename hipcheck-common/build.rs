// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use tonic_build::configure;

fn main() -> Result<()> {
	// Compile the Hipcheck gRPC protocol spec to an .rs file
	configure().compile_protos(&["proto/hipcheck/v1/plugin_service.proto"], &["proto"])?;

	// Make the target available as a compile-time env var for plugin arch
	// resolution
	println!(
		"cargo:rustc-env=TARGET={}",
		std::env::var("TARGET").unwrap()
	);
	println!("cargo:rerun-if-changed-env=TARGET");

	Ok(())
}
