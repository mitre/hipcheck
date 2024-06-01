// SPDX-License-Identifier: Apache-2.0

use proc_macro2::TokenStream;
use quote::quote;
use std::ops::Not as _;
use syn::{
	spanned::Spanned, Data, DataStruct, DeriveInput, Error, Field, Fields, FieldsNamed, Ident,
	Result,
};

/// Convenience macro for producing new `syn::Error`s.
macro_rules! err {
	( $span:expr, $msg:literal ) => {
		Err(Error::new($span, $msg))
	};
}

/// A `proc_macro2`-flavor implementation of the derive macro.
pub fn derive_update(input: DeriveInput) -> Result<TokenStream> {
	let ident = &input.ident;
	let fields = extract_field_names(&input)?;

	Ok(quote! {
		impl crate::cli::Update for #ident {
			fn update(&mut self, other: &Self) {
				#( self.#fields.update(&other.#fields ); )*
			}
		}
	})
}

/// Extract field names from derive input.
fn extract_field_names(input: &DeriveInput) -> Result<Vec<Ident>> {
	let strukt = extract_struct(input)?;
	let fields = extract_named_fields(strukt)?;
	let names = extract_field_names_from_fields(&fields[..])?;
	Ok(names)
}

/// Validate the input type has no generic parameters.
fn validate_no_generics(input: &DeriveInput) -> Result<()> {
	// We don't support generic types.
	let generics = input.generics.params.iter().collect::<Vec<_>>();
	if generics.is_empty().not() {
		return err!(
			input.span(),
			"#[derive(Update)] does not support generic types"
		);
	}

	Ok(())
}

/// Extract a struct from the input.
fn extract_struct(input: &DeriveInput) -> Result<&DataStruct> {
	validate_no_generics(input)?;

	match &input.data {
		Data::Struct(struct_data) => Ok(struct_data),
		Data::Enum(_) => err!(input.span(), "#[derive(Update)] does not support enums"),
		Data::Union(_) => err!(input.span(), "#[derive(Update)] does not support unions"),
	}
}

/// Extract named fields from a struct.
fn extract_named_fields(input: &DataStruct) -> Result<Vec<&Field>> {
	match &input.fields {
		Fields::Named(fields) => Ok(collect_named_fields(fields)),
		field @ Fields::Unnamed(_) => err!(
			field.span(),
			"#[derive(Update)] does not support unnamed fields"
		),
		field @ Fields::Unit => err!(
			field.span(),
			"#[derive(Update)] does not support unit structs"
		),
	}
}

/// Validate all fields are named.
fn validate_named_fields(fields: &[&Field]) -> Result<()> {
	for field in fields {
		if field.ident.is_none() {
			return err!(
				field.span(),
				"#[derive(Update)] does not support unnamed fields"
			);
		}
	}

	Ok(())
}

/// Collect named fields into a convenient `Vec`.
fn collect_named_fields(fields: &FieldsNamed) -> Vec<&Field> {
	fields.named.iter().collect::<Vec<_>>()
}

/// Extract field names from a bunch of named fields.
fn extract_field_names_from_fields(fields: &[&Field]) -> Result<Vec<Ident>> {
	// SAFETY: We confirm the `ident` is present, so the `unwrap` is fine.
	validate_named_fields(fields)?;

	let names = fields
		.iter()
		.map(|field| field.ident.as_ref().unwrap().to_owned())
		.collect();

	Ok(names)
}
