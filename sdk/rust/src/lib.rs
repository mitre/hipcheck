// SPDX-License-Identifier: Apache-2.0

#![allow(unexpected_cfgs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]

//! Hipcheck Plugin SDK in Rust.
//!
//! ## What is Hipcheck?
//! [Hipcheck][hipcheck] is a command line interface (CLI) tool for analyzing open source software
//! packages and source repositories to understand their software supply chain risk. It analyzes a
//! project's software development practices and detects active supply chain attacks to give you
//! both a long-term and immediate picture of the risk from using a package.
//!
//! Part of Hipcheck's value is its [plugin system][hipcheck_plugins], which allows anyone to write
//! a new data source or analysis component, or build even higher level analyses off of the results
//! of multiple other components.
//!
//! ## The Plugin SDK
//! This crate is a Rust SDK to help developers focus on writing the essential logic of their
//! Hipcheck plugins instead of worrying about session management or communication with Hipcheck
//! core. The essential steps of using this SDK are to implement the `Query` trait for each query
//! endpoint you wish to support, then implement the `Plugin` trait to tie your plugin together and
//! describe things like configuration parameters.
//!
//! For more, see our [detailed guide][web_sdk_docs] on writing plugins using this crate.
//!
//! [hipcheck]: https://hipcheck.mitre.org/
//! [hipcheck_plugins]: https://hipcheck.mitre.org/docs/guide/making-plugins/creating-a-plugin/
//! [web_sdk_docs]: https://hipcheck.mitre.org/docs/guide/making-plugins/rust-sdk/

//================================================================================================
// Re-exports
//------------------------------------------------------------------------------------------------

// All of these will appear as "top-level" items in the SDK.
pub use crate::config::PluginConfig;
pub use crate::engine::PluginEngine;
pub use crate::engine::query_builder::QueryBuilder;
pub use crate::log::init_tracing_logger;
pub use crate::plugin::Plugin;
pub use crate::query::{DynQuery, Query, query_endpoint::QueryEndpoint, query_schema::QuerySchema};
pub use crate::server::PluginServer;
pub use crate::target::QueryTarget;
pub use hipcheck_common::types::LogLevel;

//================================================================================================
// Private Modules
//------------------------------------------------------------------------------------------------

mod config;
mod engine;
mod log;
mod plugin;
mod query;
mod server;
mod target;

//================================================================================================
// Public Modules
//------------------------------------------------------------------------------------------------

/// Re-export of user-facing third-party dependencies
pub mod deps {
	pub use jiff::{Span, Zoned};
	pub use schemars::{Schema as JsonSchema, schema_for};
	pub use serde_json::{Value, from_str, from_value, to_value};
	pub use tonic::async_trait;
}

// Materials for error reporting and handling.
pub mod error;
// Type representing the host to connect a server to.
pub mod host;

#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
/// Macros for simplifying `Query` and `Plugin` trait implementations
pub mod macros {
	pub use hipcheck_sdk_macros::*;
}

#[cfg(feature = "mock_engine")]
#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
/// Tools for unit-testing plugin `Query` implementations
pub mod mock {
	pub use crate::engine::mock_responses::MockResponses;
}

/// A utility module containing everything needed to write a plugin, just write `use
/// hipcheck_sdk::prelude::*`.
pub mod prelude {
	pub use crate::deps::*;
	pub use crate::engine::{PluginEngine, query_builder::QueryBuilder};
	pub use crate::error::{ConfigError, ConfigResult, Error, Result};
	pub use crate::host::Host;
	pub use crate::server::{PluginServer, QueryResult};
	pub use crate::types::{KnownRemote, RemoteGitRepo};
	pub use crate::{DynQuery, Plugin, Query, QueryEndpoint, QuerySchema, QueryTarget};

	// Re-export macros
	#[cfg(feature = "macros")]
	#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
	pub use crate::macros::{queries, query};

	#[cfg(feature = "mock_engine")]
	#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
	pub use crate::engine::mock_responses::MockResponses;
}

/// The definitions of Hipcheck's analysis `Target` object and its sub-types for use in writing
/// query endpoints.
pub mod types;
