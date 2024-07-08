// SPDX-License-Identifier: Apache-2.0

//! Constructs an [`Agent`] with TLS using system certificates that can be used for making HTTP requests.

use crate::error::Result;
use rustls::{crypto::{ring::default_provider, CryptoProvider}, ClientConfig, RootCertStore};
use std::sync::Arc;
use ureq::{Agent, AgentBuilder};

/// Construct a new agent using system certs
pub fn new_agent() -> Result<Agent> {
	// Retrieve system certs
	let mut roots = RootCertStore::empty();
	for cert in rustls_native_certs::load_native_certs()? {
		roots.add(cert)?;
	}

	// Rustls makes us setup/install a crypto provider for this process before we can build a client config. 
	// Use the default provider backed by the `ring` crate. 
	if CryptoProvider::get_default().is_none() {
		CryptoProvider::install_default(default_provider())
			.expect("could not install default crypto provider");
	}

	// Add certs to connection configuration
	let tls_config = ClientConfig::builder()
		.with_root_certificates(roots)
		.with_no_client_auth();

	// Construct agent
	let agent = AgentBuilder::new().tls_config(Arc::new(tls_config)).build();

	Ok(agent)
}
