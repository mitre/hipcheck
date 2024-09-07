// SPDX-License-Identifier: Apache-2.0

use crate::hc_error;
use std::{fmt::Display, str::FromStr};

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

/// Architecture `hc` was built for
pub const CURRENT_ARCH: SupportedArch = {
	#[cfg(target_arch = "x86_64")]
	{
		#[cfg(target_os = "macos")]
		{
			SupportedArch::X86_64AppleDarwin
		}
		#[cfg(target_os = "linux")]
		{
			SupportedArch::X86_64UnknownLinuxGnu
		}
		#[cfg(target_os = "windows")]
		{
			SupportedArch::X86_64PcWindowsMsvc
		}
	}
	#[cfg(target_arch = "aarch64")]
	{
		#[cfg(target_os = "macos")]
		{
			SupportedArch::Aarch64AppleDarwin
		}
	}
};

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
