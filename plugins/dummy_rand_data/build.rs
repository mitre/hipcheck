// SPDX-License-Identifier: Apache-2.0

fn main() -> anyhow::Result<()> {
	tonic_build::compile_protos("../../proto/hipcheck/v1/hipcheck.proto")?;
	Ok(())
}
