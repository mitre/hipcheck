// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use pathbuf::pathbuf;
use tonic_build::compile_protos;

fn main() -> Result<()> {
	let root = env!("CARGO_MANIFEST_DIR");
	let path = pathbuf![root, "proto", "hipcheck", "v1", "hipcheck.proto"];
	compile_protos(path)?;
	Ok(())
}
