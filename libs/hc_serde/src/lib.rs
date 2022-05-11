// SPDX-License-Identifier: Apache-2.0

//! Provides common access to the `serde` crate, used widely
//! throughout Hipcheck.
//!
//! This dependency has not been included in `hc_common` in order to
//! support use of `graphql_client` derive macros, which require that
//! `serde` be a direct dependency of the containing crate.  By
//! placing the direct `serde` dependency here, we can alias this
//! crate as `serde` when we need to.
//!
//! Note that `hc_common` re-exports this crate's own `serde` export.
//! Use `hc_common` over this whenever possible.

pub use serde::{self, *};
