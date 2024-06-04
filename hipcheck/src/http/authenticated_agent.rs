// SPDX-License-Identifier: Apache-2.0

//! Defines an authenticated [`Agent`] type that adds token auth to all requests.

use crate::error::Result;
use crate::http::hidden::Hidden;
use crate::http::tls::new_agent;
use ureq::{Agent, Request};

/// An [`Agent`] which authenticates requests with token auth.
///
/// This wrapper is used to work around the fact that `ureq` removed functionality
/// to do this as part of the [`Agent`] type directly.
pub struct AuthenticatedAgent<'token> {
	/// The agent used to make the request.
	agent: Agent,

	/// The token to use with each request.
	token: Hidden<&'token str>,
}

impl<'token> AuthenticatedAgent<'token> {
	/// Construct a new authenticated agent.
	pub fn new(token: &'token str) -> Result<AuthenticatedAgent<'token>> {
		let agent = new_agent()?;

		let token = Hidden::new(token);

		Ok(AuthenticatedAgent { agent, token })
	}

	/// Make an authenticated GET request.
	pub fn get(&self, path: &str) -> Request {
		self.agent.get(path).token_auth(self.token.as_ref())
	}

	/// Make an authenticated POST request.
	pub fn post(&self, path: &str) -> Request {
		self.agent.post(path).token_auth(self.token.as_ref())
	}
}

/// The key to use for the authorization HTTP header.
const AUTH_KEY: &str = "Authorization";

/// Extension trait to add a convenient "token auth" method.
trait TokenAuth {
	/// Sets a token authentication header on a request.
	fn token_auth(self, token: &str) -> Self;
}

impl TokenAuth for Request {
	fn token_auth(self, token: &str) -> Self {
		self.set(AUTH_KEY, &format!("token {}", token))
	}
}
