// SPDX-License-Identifier: Apache-2.0
mod git2_rustls_transport;

use crate::shell::{verbosity::Verbosity, Shell};
use rustls::crypto::{ring, CryptoProvider};
use tracing_subscriber::EnvFilter;

/// Initialize global state for the program.
///
/// **NOTE:** The order in which these operations are done is precise, and
/// should not be changed!
pub fn init() {
	init_shell();
	init_logging();
	init_libgit2();
	init_cryptography();
	init_software_versions();
}

fn init_shell() {
	Shell::init(Verbosity::Normal);
}

fn init_logging() {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::from_env("HC_LOG"))
		.init();
}

fn init_libgit2() {
	// Make libgit2 use a rustls + ureq based transport for executing the git
	// protocol over http(s). I would normally just let libgit2 use its own
	// implementation but have seen that this rustls/ureq transport is 2-3 times
	// faster on my machine — enough of a performance bump to warrant using this.
	git2_rustls_transport::register();
}

fn init_cryptography() {
	// Install a process-wide default crypto provider.
	CryptoProvider::install_default(ring::default_provider())
		.expect("installed process-wide default crypto provider");
}

fn init_software_versions() {
	if let Err(e) = crate::version::init_software_versions() {
		panic!("Error saving off dependent binary versions: {}", e);
	}
}
