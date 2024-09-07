// SPDX-License-Identifier: Apache-2.0

mod git2_log_shim;
mod git2_rustls_transport;
mod indicatif_log_bridge;

use crate::shell::verbosity::Verbosity;
use crate::shell::Shell;
use env_logger::Env;
use rustls::crypto::ring;
use rustls::crypto::CryptoProvider;

/// Initialize global state for the program.
///
/// **NOTE:** The order in which these operations are done is precise, and
/// should not be changed!
pub fn init() {
	init_shell();
	init_logging();
	init_libgit2();
	init_cryptography();
}

fn init_shell() {
	Shell::init(Verbosity::Normal);
}

fn init_logging() {
	let env = Env::new().filter("HC_LOG").write_style("HC_LOG_STYLE");
	let logger = env_logger::Builder::from_env(env).build();
	indicatif_log_bridge::LogWrapper(logger)
		.try_init()
		.expect("logging initialization must succeed");
}

fn init_libgit2() {
	// Tell the `git2` crate to pass its tracing messages to the log crate.
	git2_log_shim::git2_set_trace_log_shim();

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
