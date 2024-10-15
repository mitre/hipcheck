// SPDX-License-Identifier: Apache-2.0

// Use the `README.md` as the crate docs.
#![doc = include_str!("../README.md")]

mod update;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, Error};

/// Derive an implementation of the `Update` trait for the type.
#[proc_macro_derive(Update)]
pub fn derive_update(input: TokenStream) -> TokenStream {
	// Parse the input.
	let input = parse_macro_input!(input as DeriveInput);

	// Generate the token stream.
	update::derive_update(input)
		.unwrap_or_else(Error::into_compile_error)
		.into()
}
