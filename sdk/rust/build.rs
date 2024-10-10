// SPDX-License-Identifier: Apache-2.0

use pathbuf::pathbuf;
use tonic_build::compile_protos;

fn main() -> anyhow::Result<()> {
	let root = env!("CARGO_MANIFEST_DIR");
	let path = pathbuf![root, "proto", "hipcheck", "v1", "plugin_service.proto"];
	compile_protos(path)?;
	Ok(())
}
