// SPDX-License-Identifier: Apache-2.0

use convert_case::Casing;
use proc_macro::TokenStream;
use proc_macro2::Span;
use std::ops::Not;
use std::sync::{LazyLock, Mutex};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Ident, ItemFn, Meta, PatType, parse_macro_input};

static QUERIES: LazyLock<Mutex<Vec<NamedQuerySpec>>> = LazyLock::new(|| Mutex::new(vec![]));

#[allow(unused)]
#[derive(Debug, Clone)]
struct NamedQuerySpec {
	pub struct_name: String,
	pub function: String,
	pub default: bool,
}

struct QuerySpec {
	pub function: Ident,
	pub input_type: syn::Type,
	pub output_type: syn::Type,
	pub default: bool,
}

/// Parse Path to confirm that it represents a Result<T: Serialize> and return the type T
fn parse_result_generic(p: &syn::Path) -> Result<syn::Type, Error> {
	use syn::GenericArgument;
	use syn::PathArguments;
	// Assert it is a Result
	// Panic: Safe to unwrap because there should be at least one element in the sequence
	let last = p.segments.last().unwrap();
	if last.ident != "Result" {
		return Err(Error::new(
			p.span(),
			"Expected return type to be a Result<T: Serialize>",
		));
	}
	match &last.arguments {
		PathArguments::AngleBracketed(x) => {
			let Some(GenericArgument::Type(ty)) = x.args.first() else {
				return Err(Error::new(
					p.span(),
					"Expected return type to be a Result<T: Serialize>",
				));
			};
			Ok(ty.clone())
		}
		_ => Err(Error::new(
			p.span(),
			"Expected return type to be a Result<T: Serialize>",
		)),
	}
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

fn parse_named_query_spec(opt_meta: Option<Meta>, item_fn: ItemFn) -> Result<QuerySpec, Error> {
	use syn::Meta::*;
	use syn::ReturnType;
	let sig = &item_fn.sig;

	let function = sig.ident.clone();

	let input_type: syn::Type = {
		let inputs = &sig.inputs;
		if inputs.len() != 2 {
			return Err(Error::new(
				item_fn.span(),
				"Query function must take two arguments: &mut PluginEngine, and an input type that implements Serialize",
			));
		}
		// Validate that the first arg is type &mut PluginEngine
		if let Some(syn::FnArg::Typed(engine_arg)) = inputs.get(0) {
			parse_plugin_engine(engine_arg)?;
		}

		if let Some(input_arg) = inputs.get(1) {
			let syn::FnArg::Typed(input_arg_info) = input_arg else {
				return Err(Error::new(
					item_fn.span(),
					"Query function must take two arguments: &mut PluginEngine, and an input type that implements Serialize",
				));
			};
			input_arg_info.ty.as_ref().clone()
		} else {
			return Err(Error::new(
				item_fn.span(),
				"Query function must take two arguments: &mut PluginEngine, and an input type that implements Serialize",
			));
		}
	};

	let output_type = match &sig.output {
		ReturnType::Default => {
			return Err(Error::new(
				item_fn.span(),
				"Query function must return Result<T: Serialize>",
			));
		}
		ReturnType::Type(_, b_type) => {
			use syn::Type;
			match b_type.as_ref() {
				Type::Path(p) => parse_result_generic(&p.path)?,
				_ => {
					return Err(Error::new(
						item_fn.span(),
						"Query function must return Result<T: Serialize>",
					));
				}
			}
		}
	};

	let default = match opt_meta {
		Some(NameValue(nv)) => {
			// Panic: Safe to unwrap because there should be at least one element in the sequence
			if nv.path.segments.first().unwrap().ident == "default" {
				match nv.value {
					syn::Expr::Lit(e) => match e.lit {
						syn::Lit::Bool(s) => s.value,
						_ => {
							return Err(Error::new(
								item_fn.span(),
								"Default field on query function options must have a Boolean value",
							));
						}
					},
					_ => {
						return Err(Error::new(
							item_fn.span(),
							"Default field on query function options must have a Boolean value",
						));
					}
				}
			} else {
				return Err(Error::new(
					item_fn.span(),
					"Default field must be set if options are included for the query function",
				));
			}
		}
		Some(Path(p)) => {
			let seg: &syn::PathSegment = p.segments.first().unwrap();
			if seg.ident == "default" {
				match seg.arguments {
					syn::PathArguments::None => true,
					_ => {
						return Err(Error::new(
							item_fn.span(),
							"Default field in query options path cannot have any parenthized or bracketed arguments",
						));
					}
				}
			} else {
				return Err(Error::new(
					item_fn.span(),
					"Default field must be set if options are included for the query function",
				));
			}
		}
		None => false,
		_ => {
			return Err(Error::new(
				item_fn.span(),
				"Cannot parse query function options",
			));
		}
	};

	Ok(QuerySpec {
		function,
		default,
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
pub fn query(attr: TokenStream, item: TokenStream) -> TokenStream {
	let mut to_return = proc_macro2::TokenStream::from(item.clone());
	let item_fn = parse_macro_input!(item as ItemFn);
	let opt_meta: Option<Meta> = if attr.is_empty().not() {
		Some(parse_macro_input!(attr as Meta))
	} else {
		None
	};
	let spec = match parse_named_query_spec(opt_meta, item_fn) {
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
				hipcheck_sdk::prelude::schema_for!(#input_type).schema
			}

			fn output_schema(&self) -> hipcheck_sdk::prelude::JsonSchema {
				hipcheck_sdk::prelude::schema_for!(#output_type).schema
			}

			async fn run(&self, engine: &mut hipcheck_sdk::prelude::PluginEngine, input: hipcheck_sdk::prelude::Value) -> hipcheck_sdk::prelude::Result<hipcheck_sdk::prelude::Value> {
				let input = hipcheck_sdk::prelude::from_value(input).map_err(|_|
					hipcheck_sdk::prelude::Error::UnexpectedPluginQueryInputFormat)?;
				let output = #ident(engine, input).await?;
				hipcheck_sdk::prelude::to_value(output).map_err(|_|
					hipcheck_sdk::prelude::Error::UnexpectedPluginQueryOutputFormat)
			}
		}
	};

	QUERIES.lock().unwrap().push(NamedQuerySpec {
		struct_name: struct_name.to_string(),
		function: spec.function.to_string(),
		default: spec.default,
	});

	to_return.extend(to_follow);
	proc_macro::TokenStream::from(to_return)
}

/// Generates an implementation of the `Plugin::queries()` trait function using
/// all previously-expanded `#[query]` attribute macros. Due to Rust's macro
/// expansion ordering, all `#[query]` functions must come before this macro
/// to ensure they are seen and added.
#[proc_macro]
pub fn queries(_item: TokenStream) -> TokenStream {
	let mut agg = proc_macro2::TokenStream::new();
	let q_lock = QUERIES.lock().unwrap();
	// Create a NamedQuery for each #query func we've seen
	for q in q_lock.iter() {
		let name = match q.default {
			true => "",
			false => q.function.as_str(),
		};
		let inner = Ident::new(q.struct_name.as_str(), Span::call_site());
		let out = quote::quote! {
			NamedQuery {
				name: #name,
				inner: Box::new(#inner {})
			},
		};
		agg.extend(out);
	}
	tracing::debug!(
		"Auto-generating Plugin::queries() with {} detected queries",
		q_lock.len()
	);
	// Impl `Plugin::queries` as a vec of generated NamedQuery instances
	let out = quote::quote! {
		fn queries(&self) -> impl Iterator<Item = NamedQuery> {
			vec![#agg].into_iter()
		}
	};
	proc_macro::TokenStream::from(out)
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
