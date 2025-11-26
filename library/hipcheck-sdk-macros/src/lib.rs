// SPDX-License-Identifier: Apache-2.0

use convert_case::Casing;
use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::{
	Attribute, Data, DeriveInput, Error, GenericArgument, Ident, ItemFn, PatType, PathArguments,
	ReturnType, parse::Parse, parse_macro_input, punctuated::Punctuated, spanned::Spanned,
};

/// Specification of a single query, used for generating the query trait impl.
#[derive(Debug)]
struct QuerySpec {
	pub function: Ident,
	pub input_type: syn::Type,
	pub output_type: syn::Type,
}

/// Parse Path to confirm that it represents a Result<T: Serialize> and return the type T
fn parse_result_generic(p: &syn::Path) -> Result<syn::Type, Error> {
	// Assert it is a Result
	// Panic: Safe to unwrap because there should be at least one element in the sequence
	let last = p.segments.last().unwrap();

	if last.ident != "Result" {
		return Err(Error::new(
			p.span(),
			"Expected return type to be a Result<T: Serialize>",
		));
	}

	let PathArguments::AngleBracketed(x) = &last.arguments else {
		return Err(Error::new(
			p.span(),
			"Expected return type to be a Result<T: Serialize>",
		));
	};

	let Some(GenericArgument::Type(ty)) = x.args.first() else {
		return Err(Error::new(
			p.span(),
			"Expected return type to be a Result<T: Serialize>",
		));
	};

	Ok(ty.clone())
}

/// Parse PatType to confirm that it contains a &mut PluginEngine
fn parse_plugin_engine(engine_arg: &PatType) -> Result<(), Error> {
	if let syn::Type::Reference(type_reference) = engine_arg.ty.as_ref()
		&& type_reference.mutability.is_some()
		&& let syn::Type::Path(type_path) = type_reference.elem.as_ref()
	{
		let last = type_path.path.segments.last().unwrap();

		if last.ident == "PluginEngine" {
			return Ok(());
		}
	}

	Err(Error::new(
		engine_arg.span(),
		"The first argument of the query function must be a &mut PluginEngine",
	))
}

fn parse_named_query_spec(item_fn: ItemFn) -> Result<QuerySpec, Error> {
	let sig = &item_fn.sig;
	let function = sig.ident.clone();

	let input_type = {
		// Validate that there are two function arguments.
		if sig.inputs.len() != 2 {
			return Err(Error::new(
				item_fn.span(),
				"Query function must take two arguments: &mut PluginEngine, and an input type that implements Serialize",
			));
		}

		// Validate that the first arg is type &mut PluginEngine
		if let Some(syn::FnArg::Typed(engine_arg)) = &sig.inputs.get(0) {
			parse_plugin_engine(engine_arg)?;
		}

		// Validate that the second argument is a typed function arg.
		let Some(syn::FnArg::Typed(input_arg_info)) = &sig.inputs.get(1) else {
			return Err(Error::new(
				item_fn.span(),
				"Query function must take two arguments: &mut PluginEngine, and an input type that implements Serialize",
			));
		};

		input_arg_info.ty.as_ref().clone()
	};

	let output_type = match &sig.output {
		ReturnType::Default => {
			return Err(Error::new(
				item_fn.span(),
				"Query function must return Result<T: Serialize>",
			));
		}
		ReturnType::Type(_, b_type) => match b_type.as_ref() {
			syn::Type::Path(p) => parse_result_generic(&p.path)?,
			_ => {
				return Err(Error::new(
					item_fn.span(),
					"Query function must return Result<T: Serialize>",
				));
			}
		},
	};

	Ok(QuerySpec {
		function,
		input_type,
		output_type,
	})
}

/// An attribute on a function that creates an associated struct that implements
/// the Hipcheck Rust SDK's `Query` trait. The function must have the signature
/// `fn(&mut PluginEngine, content: impl serde::Deserialize) ->
/// hipcheck_sdk::Result<impl serde::Serialize>`. The generated struct's name is
/// the pascal-case version of the function name (e.g. `do_something()` ->
/// `DoSomething`).
#[proc_macro_attribute]
pub fn query(_attr: TokenStream, item: TokenStream) -> TokenStream {
	let mut to_return = proc_macro2::TokenStream::from(item.clone());

	let item_fn = parse_macro_input!(item as ItemFn);

	let spec = match parse_named_query_spec(item_fn) {
		Ok(span) => span,
		Err(err) => return err.to_compile_error().into(),
	};

	let struct_name = Ident::new(
		spec.function
			.to_string()
			.to_case(convert_case::Case::Pascal)
			.as_str(),
		Span::call_site(),
	);

	let ident = &spec.function;
	let input_type = spec.input_type;
	let output_type = spec.output_type;

	let to_follow = quote::quote! {
		struct #struct_name {}

		#[hipcheck_sdk::prelude::async_trait]
		impl hipcheck_sdk::prelude::Query for #struct_name {
			fn input_schema(&self) -> hipcheck_sdk::prelude::JsonSchema {
				hipcheck_sdk::prelude::schema_for!(#input_type)
			}

			fn output_schema(&self) -> hipcheck_sdk::prelude::JsonSchema {
				hipcheck_sdk::prelude::schema_for!(#output_type)
			}

			async fn run(
				&self,
				engine: &mut hipcheck_sdk::prelude::PluginEngine,
				input: hipcheck_sdk::prelude::Value
			) -> hipcheck_sdk::prelude::Result<hipcheck_sdk::prelude::Value>
			{
				let input = hipcheck_sdk::prelude::from_value(input).map_err(|_|
					hipcheck_sdk::prelude::Error::UnexpectedPluginQueryInputFormat)?;

				let output = #ident(engine, input).await?;
				hipcheck_sdk::prelude::to_value(output).map_err(|_|
					hipcheck_sdk::prelude::Error::UnexpectedPluginQueryOutputFormat)
			}
		}
	};

	to_return.extend(to_follow);
	proc_macro::TokenStream::from(to_return)
}

/// Generates an implementation of the `Plugin::queries()` trait function using
/// all previously-expanded `#[query]` attribute macros. Due to Rust's macro
/// expansion ordering, all `#[query]` functions must come before this macro
/// to ensure they are seen and added.
#[proc_macro]
pub fn queries(item: TokenStream) -> TokenStream {
	let query_list = parse_macro_input!(item as QueryList);

	// Create a NamedQuery for each #query func we've seen
	let agg = query_list
		.inner
		.iter()
		.map(|query_entry| {
			let name = if query_entry.is_default() {
				"".to_string()
			} else {
				query_entry.ident.to_string()
			};

			let struct_name = Ident::new(
				query_entry
					.ident
					.to_string()
					.to_case(convert_case::Case::Pascal)
					.as_str(),
				Span::call_site(),
			);

			quote::quote! {
				NamedQuery {
					name: #name,
					inner: Box::new(#struct_name {})
				},
			}
		})
		.collect::<proc_macro2::TokenStream>();

	tracing::debug!(
		"Auto-generating Plugin::queries() with {} detected queries",
		query_list.inner.len()
	);

	// Impl `Plugin::queries` as a vec of generated NamedQuery instances
	let out = quote::quote! {
		fn queries(&self) -> impl Iterator<Item = NamedQuery> {
			vec![#agg].into_iter()
		}
	};

	proc_macro::TokenStream::from(out)
}

/// A list of query names, separated by commas, with an optional default annotation.
///
/// Could look like: `#[default] default_query, some_other_query, another_query`.
#[derive(Debug)]
struct QueryList {
	/// List of queries
	inner: Punctuated<QueryListEntry, syn::Token![,]>,
}

impl Parse for QueryList {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		Ok(Self {
			inner: input.parse_terminated(QueryListEntry::parse, syn::Token![,])?,
		})
	}
}

/// A single entry in the query list, with an optional default annotation.
///
/// Could look like: `#[default] query_name` or just `query_name`.
#[derive(Debug)]
struct QueryListEntry {
	attrs: Vec<Attribute>,
	/// Identifier for the query function.
	ident: Ident,
}

impl QueryListEntry {
	pub fn is_default(&self) -> bool {
		self.attrs
			.iter()
			.any(|attr| attr.path().is_ident("default"))
	}
}

impl Parse for QueryListEntry {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		Ok(Self {
			attrs: input.call(Attribute::parse_outer).unwrap_or_default(),
			ident: input.parse()?,
		})
	}
}

/// Generates a derived macro implementation of the `PluginConfig` trait to
/// deserialize each plugin config field derived from the Policy File.
/// Config-related errors are handled by the `ConfigError` crate to generate
/// specific error messages that detail the plugin, field, and type expected from
/// the Policy File.
#[proc_macro_derive(PluginConfig)]
pub fn derive_plugin_config(input: TokenStream) -> TokenStream {
	// Parse the input struct
	let input = parse_macro_input!(input as DeriveInput);

	// Extract the RawConfig struct name
	let struct_name = &input.ident;

	let Data::Struct(syn::DataStruct { fields, .. }) = &input.data else {
		// Return an error if the macro is used on something other than a struct
		return syn::Error::new(input.span(), "PluginConfig can only be derived for structs")
			.to_compile_error()
			.into();
	};

	// Helper function to convert field names to dashed strings
	fn to_dashed_field_name(field: &syn::Field) -> String {
		field.ident.as_ref().unwrap().to_string().replace("_", "-")
	}

	// Generate deserialization logic for each field
	let field_deserialization: Vec<_> = fields
		.iter()
		.map(|field| {
			let field_name = field.ident.as_ref().unwrap();
			let field_name_str = to_dashed_field_name(field);
			let field_type = &field.ty;

			quote::quote! {
				let #field_name = if let Some(value) = config.remove(#field_name_str) {
					// Map contained value, return an error if an invalid value is provided for the field
					serde_json::from_value::<#field_type>(value.clone()).map_err(|_| {
						ConfigError::InvalidConfigValue {
							field_name: #field_name_str.to_owned().into_boxed_str(),
							value: format!("{:?}", value).into_boxed_str(),
							reason: format!(
								"Expected type: {}, but got: {:?}",
								stringify!(#field_type),
								value
							).into_boxed_str(),
						}
					})?
				} else {
					// Try deserializing from null. If it works, value's type indicates it was
					// optional. If this fails, missing required config.
					serde_json::from_value::<#field_type>(serde_json::Value::Null).map_err(|_| {
						ConfigError::MissingRequiredConfig {
							field_name: #field_name_str.to_owned().into_boxed_str(),
							field_type: stringify!(#field_type).to_owned().into_boxed_str(),
							possible_values: vec![],
						}
					})?
				};
			}
		})
		.collect();

	// After the expected fields are extracted, there should be no remaining fields in the config map
	let validate_fields = quote::quote! {
		if let Some((unexpected_key, value)) = config.iter().next() {
			// Return an error if any remaining key/value pair in map
			return Err(ConfigError::UnrecognizedConfig {
				field_name: unexpected_key.to_string().into_boxed_str(),
				field_value: format!("{:?}", value).into_boxed_str(),
				possible_confusables: vec![],
			});
		}
	};

	// Generate code to initialize the struct fields
	let initialize_struct: Vec<_> = fields
		.iter()
		.map(|field| {
			let field_name = field.ident.as_ref().unwrap();
			quote::quote! {
				#field_name
			}
		})
		.collect();

	// Generate the implementation of the PluginConfig trait
	let impl_block = quote::quote! {
		impl<'de> PluginConfig<'de> for #struct_name {
			fn deserialize(conf_ref: &serde_json::Value) -> StdResult<Self, ConfigError> {
				let mut conf_owned = conf_ref.clone();
				let mut dummy = serde_json::Map::new();
				let config = conf_owned.as_object_mut().unwrap_or(&mut dummy);

				#(#field_deserialization)* // Deserialize each field
				#validate_fields
				Ok(Self {
					#(#initialize_struct),* // Initialize each field
				})
			}
		}
	};

	// Return the generated TokenStream
	proc_macro::TokenStream::from(impl_block)
}
