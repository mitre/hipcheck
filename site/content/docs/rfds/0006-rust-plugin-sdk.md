---
title: Rust Plugin SDK
weight: 6
slug: 0006
extra:
  rfd: 6
  primary_author: Andrew Lilley Brinker
  primary_author_link: https://github.com/alilleybrinker
  status: Accepted
  pr: 402
---

# Rust Plugin SDK

Now that we've landed the initial implementation of the plugin system as
described in [RFD 4], the next step is to split out our own existing
analyses and data providers into separate plugins. We have already developed
a couple of "dummy" plugins to test the proper functioning of the plugin
system within `hc`, but the code in those plugins which implements the
plugin gRPC interface and the query protocol is not well abstracted or
reusable.

Additionally, we'd like to start supporting folks outside of the Hipcheck
project in developing their own plugins, and the same problem arises of that
currently being difficult to do.

Together, these problems motivate the need to begin developing Software
Development Kits (SDKs) for building Hipcheck plugins.

One of the strengths of the new plugin system is that plugins can be written
in any programming language. At the same time, for building an SDK, we need
to pick programming languages to prioritize. In our case, Rust is the obvious
first choice since it's the language we use and the one we'll use for our
own plugins. Over time we will no doubt add SDKs in other popular languages.

The remainder of this RFD outlines the design of the Rust plugin SDK,
particularly the interface we'll expose to plugin authors.

## Plugin SDK Interface

### The `Plugin` Trait

The central trait of the plugin interface is the `Plugin` trait, which the
user will use to define their plugin. The trait looks like this:

```rust
pub trait Plugin: Send + Sync + 'static {
	/// The name of the publisher of the plugin.
	const PUBLISHER: &'static str;

	/// The name of the plugin.
	const NAME: &'static str;

	/// Handles setting configuration.
	fn set_config(&mut self, config: JsonValue) -> StdResult<(), ConfigError>;

	/// Gets the plugin's default policy expression.
	fn default_policy_expr(&self) -> Result<String>;

	/// Get an explanation of the plugin's default query.
	fn default_query_explanation(&self) -> Result<Option<String>>;

	/// Get all the queries supported by the plugin.
	fn queries(&self) -> impl Iterator<Item = NamedQuery>;

	/// Get the plugin's default query, if it has one.
	fn default_query(&self) -> Option<DynQuery> {
		self.queries()
			.find_map(|named| named.is_default().then_some(named.inner))
	}

	/// Get all schemas for queries provided by the plugin.
	fn schemas(&self) -> impl Iterator<Item = QuerySchema> {
		self.queries().map(|query| QuerySchema {
			query_name: query.name,
			input_schema: query.inner.input_schema(),
			output_schema: query.inner.output_schema(),
		})
	}
}
```

Let's walk through the details of this design.

First, note that `Plugin` has two supertraits and a lifetime bound.
The `Plugin` type must be `Send` and `Sync`, and it must outlive the
`'static` bound. The `Send` and `Sync` requirements are actually ones
we inherit from Tonic, the library we're using for gRPC under the hood.
The `'static` requirement is one folks can often misunderstand in Rust,
but here means that the `Plugin` type may not hold onto any borrowed
data. If the `Plugin` wants to hold pointers to any additional data
it does not own itself, it can't do it through references, but must
use a different pointer type like a `Box` or `Arc` wrapped in the
appropriate synchronizing containers.

Next, we'll note that `PUBLISHER` and `NAME` are both constants
associated with the trait. This is because we want to enforce that
these are static, since they're essential to dispatch with Hipcheck
itself for queries. We also require the `&'static str` type to ensure
that these are fully-static string slices.

Next, the `set_config` method is part of how `hc` initializes plugins,
providing the configuration block from the end-user's policy file for
that plugin so the plugin can attempt to initialize any of its own
internal state. The plugin is expected to report any errors arising
from invalid configuration provided to this call.

Then, the `default_policy_expr` function provided a "filled-out"
default policy expression, which may be based on configuration set by
the call to `set_config`. This policy expression is what will be used
for scoring the output of the plugin's default query if the user
does not provide their own policy expression and the plugin is used
as a plugin in the scoring tree specified in the end-user's policy file.

The `default_query_explanation` function lets plugins provide an explanation
of the operation of their default query. Plugins don't have to provide a
default query, but if they do then they should explain it.

Note that currently the policy expression will be in the form of a string.
In the future we'd like to augment this API to provide a more structured
type for constructing policy expressions within the plugin SDK, to help
reduce the possibility of errors in the production of this default
policy expression.

Finally, the `queries` method provides an iterator over the queries defined
by the plugin. Plugins may define any number of queries they want. The
details of how those queries are defined is provided in the next section.

### The `Query` Trait

This is the trait that defines a single query for a plugin. Note that
one major constraint on the design of this trait is that we want it to be
trait-object compatible (normal Rust terminology here is "object safe"
though I prefer to more recently-proposed term "dyn safe"). To be dyn
safe, a trait has to meet certain requirements. In our case that means this
trait can't have a `const` `NAME` field for the name of the query, as
constants like this are not dyn safe.

The design for the `Query` trait will therefore look like this:

```rust
#[async_trait]
/// Defines a single query for the plugin.
pub trait Query {
	/// Get the input schema for the query.
	fn input_schema(&self) -> JsonSchema;

	/// Get the output schema for the query.
	fn output_schema(&self) -> JsonSchema;

	/// Run the plugin, optionally making queries to other plugins.
	async fn run(&self, engine: &mut PluginEngine, input: JsonValue) -> Result<JsonValue, Error>;
}
```

Here, `Error` is a type we've defined for error handling, and `JsonValue`
is `serde_json::Value`. This interface allows queries to use the `PluginEngine`
type to make queries to other plugins.

Note that from the perspective of a query author, this is a very nice and
ergonomic model. A query is just a single asynchronous function, where
making other queries is just an await point. Any details of handling this
asynchronous operation correctly, or of handling the execution of the
underlying query protocol, are hidden.

### The `NamedQuery` Type

One challenge with the `Query` trait needing to be made into a trait object
is that we can't replicate the static associated string design of the `Plugin`
trait for it. Instead, we introduce a `NamedQuery` struct which looks like
this:

```rust
/// Query trait object.
pub type DynQuery = Box<dyn Query>;

pub struct NamedQuery {
	/// The name of the query.
	name: &'static str,

	/// The query object.
	inner: DynQuery,
}

impl NamedQuery {
	/// Is the current query the default query?
	fn is_default(&self) -> bool {
		self.name.is_empty()
	}
}
```

As you can see, this is just a struct that combines a query name and
a query trait object. So generally when dealing with queries in the
`Plugin` trait, we're actually dealing with `NamedQuery`.

### The `PluginEngine` Type

The `PluginEngine` is an opaque handle for making queries to the
`PluginServer`. Its interface looks like:

```rust
pub struct PluginEngine {
    // Not specified here...
}

impl PluginEngine {
    async fn query(&mut self, target: Into<QueryTarget>, input: JsonValue) -> Result<JsonValue, Error> {
        // ...
    }
}
```

As you can see, this looks very similar to the `run` function for queries,
but includes a `target` which specifies the plugin and query to call. The full
details of how this `target` works are specific below.

### The `QueryTarget` Type

The `QueryTarget` type is a wrapper for three pieces of information, the
plugin publisher, plugin name, and query name. Query name may optionally be
empty to call the default query of a plugin. Informally, its representation
is effectively:

```rust
struct QueryTarget {
    publisher: String,
    plugin: String,
    query: Option<String>,
}
```

`Into<QueryTarget>` will be implemented for `&str`, and will attempt to
parse a `/`-separated string of these parts into a `QueryTarget` the `Into`
bound on the `PluginEngine::query` function is provided to make the API
friendlier to use. With this design, the API looks like:

```rust
// Assuming a variable "target_date" has been defined.
let result = engine.query("mitre/activity/num_weeks_since", json!({ "target_date": target_date }))?;
```

### The `PluginServer` Type

The `PluginServer` type is what turns the end-user's `Plugin`-implementing
type and uses it to actually run the gRPC server interface. Note that we do
mean _server_ interface; in the Hipcheck architecture, plugins are run as
separate processes which expose a gRPC server for which `hc` is the client.

The `PluginServer` type handles all the fiddly operation of the plugin gRPC
protocol, including validating inputs, converting inputs and outputs to and
from gRPC message types, reporting gRPC-friendly errors, running the
query protocol request/response flow, chunking query responses, and
un-chunking query requests.

The public API of the `PluginServer` type is:

```rust
pub struct PluginServer<P> {
	plugin: P,
}

impl<P: Plugin> PluginServer<P> {
	/// Create a new plugin server for the provided plugin.
	pub fn register(plugin: P) -> PluginServer<P> {
		PluginServer { plugin }
	}

	/// Run the plugin server on the provided port.
	pub async fn listen(self, port: u16) -> Result<()> {
		let service = PluginServiceServer::new(self);
		let host = format!("127.0.0.1:{}", port).parse().unwrap();

		Server::builder()
			.add_service(service)
			.serve(host)
			.await
			.map_err(Error::FailedToStartServer)?;

		Ok(())
	}
}
```

This means that the only thing the end-user of the plugin SDK can do with
a `PluginServer` is 1) create it, and 2) start running it on a specific
port.

The `PluginServiceServer` type comes from the gRPC code generated by
`prost` in concert with `tonic`. Our `PluginServer` type also implements
the `PluginService` trait defined by our generated gRPC code, and it's
this interface that connects our `PluginServer` to `tonic`.

## Scope Limits

We've purposefully kept the scope of this initial SDK definition as minimal
as possible. In particular we do not provide any out-of-the-box mechanisms
for handling:

- Terminal output
- Logging
- Command line argument parsing

These are all things we _could_ do, but this would run the risk of providing
a too-opinionated SDK, and would also delay our ability to ship a usable SDK.
These aren't problems we need to solve today, so we do not solve them here.

## Future Work

In the future, it might be valuable to provide macros to users to assist with
defining queries. In particular, given that queries are essentially an async
function with a specific kind of signature, where the input and output types
must implement the relevant JSON schema trait, we could imagine a procedural
macro which generates a type for a query and implements the `Query` trait for
that type when annotated on an appropriate async function. In that case, it
might look like this:

```rust
#[hipcheck_sdk::query]
async fn do_thing(engine: &mut PluginEngine, input: InputType) -> Result<OutputType> {
    // ...
}
```

Where `InputType` and `OutputType` are types defined by the user of the SDK
which implement the `schemars::JsonSchema` trait. This macro could generate
a `DoThing` type implementing `Query`.

[RFD 4]: https://mitre.github.io/hipcheck/rfds/0004/
