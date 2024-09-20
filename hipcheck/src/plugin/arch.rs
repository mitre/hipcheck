// SPDX-License-Identifier: Apache-2.0

use crate::error::Result;
use crate::hc_error;
use clap::ValueEnum;
use std::{fmt::Display, result::Result as StdResult, str::FromStr, sync::OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValueEnum)]
/// Officially supported target triples, as of RFD #0004
///
/// NOTE: these architectures correspond to the offically supported Rust platforms
pub enum KnownArch {
	/// Used for macOS running on "Apple Silicon" running on a 64-bit ARM Instruction Set Architecture (ISA)
	Aarch64AppleDarwin,
	/// Used for macOS running on the Intel 64-bit ISA
	X86_64AppleDarwin,
	/// Used for Windows running on the Intel 64-bit ISA with the Microsoft Visual Studio Code toolchain for compilation
	X86_64PcWindowsMsvc,
	/// Used for Linux operating systems running on the Intel 64-bit ISA with a GNU toolchain for compilation
	X86_64UnknownLinuxGnu,
	/// Used for Linux operating systems running on a 64-bit ARM ISA
	Aarch64UnknownLinuxGnu,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Arch {
	Known(KnownArch),
	Unknown(String),
}

pub const DETECTED_ARCH_STR: &str = env!("TARGET");

pub const DETECTED_ARCH: Option<KnownArch> = {
	if cfg!(target_arch = "x86_64") {
		if cfg!(target_os = "macos") {
			Some(KnownArch::X86_64AppleDarwin)
		} else if cfg!(target_os = "linux") {
			Some(KnownArch::X86_64UnknownLinuxGnu)
		} else if cfg!(target_os = "windows") {
			Some(KnownArch::X86_64PcWindowsMsvc)
		} else {
			None
		}
	} else if cfg!(target_arch = "aarch64") {
		if cfg!(target_os = "macos") {
			Some(KnownArch::Aarch64AppleDarwin)
		} else if cfg!(target_os = "linux") {
			Some(KnownArch::Aarch64UnknownLinuxGnu)
		} else {
			None
		}
	} else {
		None
	}
};

pub static USER_PROVIDED_ARCH: OnceLock<Arch> = OnceLock::new();

/// Get the target architecture for plugins. If the user provided a target,
/// return that. Otherwise, if the `hc` binary was compiled for a supported
/// architecture, return that. Otherwise return None.
pub fn get_current_arch() -> Arch {
	if let Some(arch) = USER_PROVIDED_ARCH.get() {
		arch.clone()
	} else if let Some(known_arch) = DETECTED_ARCH {
		Arch::Known(known_arch)
	} else {
		Arch::Unknown(DETECTED_ARCH_STR.to_owned())
	}
}

pub fn try_set_arch(arch: &Arch) -> Result<()> {
	let set_arch = USER_PROVIDED_ARCH.get_or_init(|| arch.clone());
	if set_arch == arch {
		Ok(())
	} else {
		Err(hc_error!(
			"Architecture could not be set to {}, has already been set to {}",
			arch,
			set_arch
		))
	}
}

impl FromStr for KnownArch {
	type Err = crate::Error;

	fn from_str(s: &str) -> StdResult<Self, Self::Err> {
		match s {
			"aarch64-apple-darwin" => Ok(Self::Aarch64AppleDarwin),
			"aarch64-unknown-linux-gnu" => Ok(Self::Aarch64UnknownLinuxGnu),
			"x86_64-apple-darwin" => Ok(Self::X86_64AppleDarwin),
			"x86_64-pc-windows-msvc" => Ok(Self::X86_64PcWindowsMsvc),
			"x86_64-unknown-linux-gnu" => Ok(Self::X86_64UnknownLinuxGnu),
			_ => Err(hc_error!("Error parsing arch '{}'", s)),
		}
	}
}

impl Display for KnownArch {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let target_triple = match self {
			KnownArch::Aarch64AppleDarwin => "aarch64-apple-darwin",
			KnownArch::Aarch64UnknownLinuxGnu => "aarch64-unknown-linux-gnu",
			KnownArch::X86_64AppleDarwin => "x86_64-apple-darwin",
			KnownArch::X86_64PcWindowsMsvc => "x86_64-pc-windows-msvc",
			KnownArch::X86_64UnknownLinuxGnu => "x86_64-unknown-linux-gnu",
		};
		write!(f, "{}", target_triple)
	}
}

impl FromStr for Arch {
	type Err = std::convert::Infallible;

	fn from_str(s: &str) -> StdResult<Self, Self::Err> {
		if let Ok(known_arch) = FromStr::from_str(s) {
			Ok(Arch::Known(known_arch))
		} else {
			Ok(Arch::Unknown(s.to_owned()))
		}
	}
}

impl Display for Arch {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Arch::Known(known_arch) => known_arch.fmt(f),
			Arch::Unknown(arch_str) => arch_str.fmt(f),
		}
	}
}
