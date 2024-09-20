// SPDX-License-Identifier: Apache-2.0

use crate::error::Result;
use crate::hc_error;
use clap::ValueEnum;
use std::{fmt::Display, result::Result as StdResult, str::FromStr, sync::OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValueEnum)]
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

pub const DETECTED_ARCH: Option<SupportedArch> = {
	if cfg!(target_arch = "x86_64") {
		if cfg!(target_os = "macos") {
			Some(SupportedArch::X86_64AppleDarwin)
		} else if cfg!(target_os = "linux") {
			Some(SupportedArch::X86_64UnknownLinuxGnu)
		} else if cfg!(target_os = "windows") {
			Some(SupportedArch::X86_64PcWindowsMsvc)
		} else {
			None
		}
	} else if cfg!(target_arch = "aarch64") {
		if cfg!(target_os = "macos") {
			Some(SupportedArch::Aarch64AppleDarwin)
		} else {
			None
		}
	} else {
		None
	}
};

pub static USER_PROVIDED_ARCH: OnceLock<SupportedArch> = OnceLock::new();

/// Get the target architecture for plugins. If the user provided a target,
/// return that. Otherwise, if the `hc` binary was compiled for a supported
/// architecture, return that. Otherwise return None.
pub fn get_current_arch() -> Option<SupportedArch> {
	if let Some(arch) = USER_PROVIDED_ARCH.get() {
		Some(*arch)
	} else if DETECTED_ARCH.is_some() {
		DETECTED_ARCH
	} else {
		None
	}
}

/// Like `get_current_arch()`, but returns an error message suggesting the
/// user specifies a target on the CLI
pub fn try_get_current_arch() -> Result<SupportedArch> {
	if let Some(arch) = get_current_arch() {
		Ok(arch)
	} else {
		Err(hc_error!("Could not resolve the current machine to one of the Hipcheck supported architectures. Please specify --arch on the commandline."))
	}
}

pub fn try_set_arch(arch: SupportedArch) -> Result<()> {
	let set_arch = USER_PROVIDED_ARCH.get_or_init(|| arch);
	if *set_arch == arch {
		Ok(())
	} else {
		Err(hc_error!(
			"Architecture could not be set to {}, has already been set to {}",
			arch,
			set_arch
		))
	}
}

impl FromStr for SupportedArch {
	type Err = crate::Error;

	fn from_str(s: &str) -> StdResult<Self, Self::Err> {
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
