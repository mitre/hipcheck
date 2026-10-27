// SPDX-License-Identifier: Apache-2.0

//! Globally defined agent containing system TLS Certs.

use crate::Result;
use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use std::sync::{Arc, OnceLock};
use ureq::{Agent, AgentBuilder};

/// Global static holding the agent with the appropriate TLS certs.
static AGENT: OnceLock<Agent> = OnceLock::new();

/// Get or initialize the global static agent used in making http(s) requests for hipcheck.
pub fn agent() -> Result<&'static Agent> {
	// Create connection configuration with system certs retrieved by rustls platform verifier
	let tls_config = ClientConfig::with_platform_verifier()?;
	let agent = AGENT.get_or_init(|| AgentBuilder::new().tls_config(Arc::new(tls_config)).build());
	Ok(agent)
}
