//! Globally defined agent containing system TLS Certs.

use rustls::{ClientConfig, RootCertStore};
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
		// Retrieve system certs
		let mut roots = RootCertStore::empty();
		let native_certs =
			rustls_native_certs::load_native_certs().expect("should load native certs");
		roots.add_parsable_certificates(native_certs);

		// Add certs to connection configuration
		let tls_config = ClientConfig::builder()
			.with_root_certificates(roots)
			.with_no_client_auth();

		// Construct agent
		AgentBuilder::new().tls_config(Arc::new(tls_config)).build()
	})
}
