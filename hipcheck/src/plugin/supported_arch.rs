use std::{fmt::Display, str::FromStr};

use crate::hc_error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// Officially supported target triples, as of RFD #0004
///
/// NOTE: these architectures correspond to the offically supported Rust platforms
pub enum SupportedArch {
	/// Used for macOS running on "Apple Silicon" running on a 64-bit ARM Instruction Set Architecture (ISA)
	Aarch64AppleDarwin,
	/// Used for macOS running on the Intel 64-bit ISA
	X86_64AppleDarwin,
	/// Used for Windows running on the Intel 64-bit ISA with the Microsoft Visual Studio Code toolchain for compilation
	X86_64PcWindowsMsvc,
	/// Used for Linux operating systems running on the Intel 64-bit ISA with a GNU toolchain for compilation
	X86_64UnknownLinuxGnu,
}

impl FromStr for SupportedArch {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"aarch64-apple-darwin" => Ok(Self::Aarch64AppleDarwin),
			"x86_64-apple-darwin" => Ok(Self::X86_64AppleDarwin),
			"x86_64-pc-windows-msvc" => Ok(Self::X86_64PcWindowsMsvc),
			"x86_64-unknown-linux-gnu" => Ok(Self::X86_64UnknownLinuxGnu),
			_ => Err(hc_error!("Error parsing arch '{}'", s)),
		}
	}
}

impl Display for SupportedArch {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let target_triple = match self {
			SupportedArch::Aarch64AppleDarwin => "aarch64-apple-darwin",
			SupportedArch::X86_64AppleDarwin => "x86_64-apple-darwin",
			SupportedArch::X86_64PcWindowsMsvc => "x86_64-pc-windows-msvc",
			SupportedArch::X86_64UnknownLinuxGnu => "x86_64-unknown-linux-gnu",
		};
		write!(f, "{}", target_triple)
	}
}
