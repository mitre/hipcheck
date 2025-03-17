// SPDX-License-Identifier: Apache-2.0

//! Globally defined agent containing system TLS Certs.

use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use std::sync::{Arc, OnceLock};
use ureq::{Agent, AgentBuilder};

/// Global static holding the agent with the appropriate TLS certs.
static AGENT: OnceLock<Agent> = OnceLock::new();

/// Get or initialize the global static agent used in making http(s) requests for hipcheck.
///
/// # Panics
/// - If native certs cannot be loaded the first time this function is called.
pub fn agent() -> &'static Agent {
	AGENT.get_or_init(|| {
		// Create connection configuration with system certs retrieved by rustls platform verifier
		let tls_config = ClientConfig::with_platform_verifier();
		// Construct agent
		AgentBuilder::new().tls_config(Arc::new(tls_config)).build()
	})
}
