// SPDX-License-Identifier: Apache-2.0

//! Defines an authenticated [`Agent`] type that adds token auth to all requests.

use crate::tls::agent;
use hipcheck_sdk::error::Result;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use ureq::{Agent, Request};

/// An [`Agent`] which authenticates requests with token auth.
///
/// This wrapper is used to work around the fact that `ureq` removed functionality
/// to do this as part of the [`Agent`] type directly.
pub struct AuthenticatedAgent<'token> {
	/// The agent used to make the request.
	agent: &'static Agent,

	/// The token to use with each request.
	token: Redacted<&'token str>,
}

impl<'token> AuthenticatedAgent<'token> {
	/// Construct a new authenticated agent.
	pub fn new(token: &'token str) -> Result<AuthenticatedAgent<'token>> {
		Ok(AuthenticatedAgent {
			agent: agent::agent()?,
			token: Redacted::new(token),
		})
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

/// Helper container to ensure a value isn't printed.
struct Redacted<T>(T);

impl<T> Redacted<T> {
	/// Construct a new redacted value.
	pub fn new(val: T) -> Redacted<T> {
		Redacted(val)
	}
}

impl<T> AsRef<T> for Redacted<T> {
	fn as_ref(&self) -> &T {
		&self.0
	}
}

impl<T> AsMut<T> for Redacted<T> {
	fn as_mut(&mut self) -> &mut T {
		&mut self.0
	}
}

impl<T> Debug for Redacted<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		write!(f, "<redacted>")
	}
}
