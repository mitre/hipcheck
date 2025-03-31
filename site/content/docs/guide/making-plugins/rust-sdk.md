---
title: The Rust Plugin SDK
weight: 2
---


# The Rust Plugin SDK

The Hipcheck team maintains a library crate `hipcheck-sdk` which provides
developers with tools for greatly simplifying plugin development in Rust. This
section will describe at a high level how a plugin author can use the SDK, but
for more detailed information please see the [API docs](https://docs.rs/hipcheck-sdk).

The first step is to add `hipcheck-sdk` as a dependency to your Rust project.
If you plan to use the macro approach described below, please add the `"macros"`
feature.

Next, the SDK provides `prelude` module which authors can import to get
access to all the essential types it exposes. If you want to manage your imports
to avoid potential type name collisions you may do so, otherwise simply write
`use hipcheck_sdk::prelude::*`.

### Defining Query Endpoints

The Hipcheck plugin communication protocol allows a plugin to expose multiple
named query endpoints that can be called by Hipcheck core or other plugins.
Developers may choose to use the `query` [attribute macro](#using-proc-macros)
to mark functions as endpoints, or [manually implement](#manual-implementation)
the `Query` trait.

#### Using Proc Macros

The SDK offers an attribute proc macro `query` for marking `async` functions
as query endpoints. As a reminder, you must have enabled the `"macros"` feature
on your `hipcheck_sdk` dependency to use the SDK macros.

To mark an `async fn` as a query endpoint, The function signature must be of the
form

```rust
async fn [FUNC_NAME](engine: &mut PluginEngine, input: [INPUT_TYPE]) -> Result<[OUTPUT_TYPE]>
```

Where:
- `PluginEngine` and `Result` are from `hipcheck_sdk::prelude`
- `[INPUT_TYPE]` and `[OUTPUT_TYPE]` are Rust types that implement
	`serde::Serialize` and `schemars::JsonSchema`. These traits are implemented
	already for many standard types.

To tag this function as a query endpoint, simply (@Todo - how to import?) and
apply the `#[query]` attribute to the function.

Importantly, this attribute will create a struct with Pascal-case version of
your function name (e.g. `foo_bar()` -> `struct FooBar`). You will need this
struct name to implement `Plugin::queries()` [below](#the-plugin-trait).

For a description of how the `PluginEngine` is used to query other plugins, see
[below](#querying-other-plugins).

#### Manual Implementation

For each query endpoint you want to define, you must create a struct that
implements the `Query` trait from the `prelude`. `Query` is declared as such:

```rust
#[tonic::async_trait]
trait Query: Send {
	fn input_schema(&self) -> JsonSchema;

	fn output_schema(&self) -> JsonSchema;

	async fn run(&self, engine: &mut PluginEngine, input: JsonValue) -> Result<JsonValue>;
}
```

The `input_schema()` and `output_schema()` function calls allow you to declare
the signature of the query (what type of JSON value it takes and returns,
respectively) as a `schemars::schema::Schema` object. Since schemas are
themselves JSON objects, we recommend you store these as separate `.json`
files that you reference in  `include_str!()` macro calls to copy the contents
into your binary at compile time as a `&'static str`. For example:

```rust
static MY_QUERY_KEY_SCHEMA: &str = include_str!("../schema/my_query_key_schema.json");
static MY_QUERY_OUTPUT_SCHEMA: &str = include_str!("../schema/my_query_output_schema.json");
```

#### The `Query::run()` Function

The `run()` function is the place where your actual query logic wil go. Let's
look at it in more detail. It's an `async` function since the underlying SDK
may execute the `run()` functions of different `impl Query` structs in parallel
as queries from Hipcheck come in, and `async` allows for simple and efficient
concurrency. The function takes a (mutable) reference to a `PluginEngine`
struct. We will discuss `PluginEngine` below, but for now just know that
this struct exposes an `async query()` function that allows your
query endpoint to in turn request information from other plugins. With that complexity
out of the way, all that's left is a simple function that takes a JSON object as
input and returns a JSON object of its own, wrapped in a `Result` to allow for failure.

The first step of your `run()` function implementation will likely be to parse the JSON
value in to primitive typed data that you can manipulate. This could involve
deserializing to a struct or `match`ing on the `JsonValue` variants manually.
If the value of `input` does not match what your query endpoint expects in its
input schema, you can return an `Err(Error::UnexpectedPluginQueryInputFormat)`,
where `Error` is the `enum` type from the SDK `prelude`.  For more information on the
different error variants, see the [API docs](https://docs.rs/hipcheck-sdk).

If your query endpoint can complete with just the input data, then you can
simply perform the calculations, serialize the output type to a JSON value, and
return it wrapped in `Ok`. However, many plugins will rely on additional data from other
plugins. In the next subsection we will describe how to do that in more detail.

#### Querying Other Plugins

As mentioned above, the `run()` function receives a handle to a `PluginEngine` instance
which exposes the following generic function:

```rust
async fn query<T, V>(&mut self, target: T, input: V) -> Result<JsonValue>
where
	T: TryInto<QueryTarget, Error: Into<Error>>,
	V: Into<JsonValue>;

struct QueryTarget {
	publisher: String,
	plugin: String,
	query: Option<String>,
}
```

At a high-level, this function simply takes a value that identifies the target
plugin and query endpoint, and passes the `input` value to give to that query
endpoint's `run()` function, then returns the forwarded result of that
operation.

The "target query endpoint" identifier is anything that implements
`TryInto<QueryTarget>`. The SDK implements this trait for `String`, so you can
pass a string of the format `publisher/plugin[/query]` where the bracketed
substring is optional. Each plugin is allowed to declare an unnamed "default"
query; by omitting the `/query` from your target string, you are targetting the
default query endpoint for the plugin. If you don't want to pass a `String` to
`target`, you can always instantiate a `QueryTarget` yourself and pass that.

### The `Plugin` Trait

At this point, you should have one struct that implements `Query` for each
query endpoint you want your plugin to expose. Now, you need to implement the
`Plugin` trait which will tie everything together and expose some additional
information about your plugin to Hipcheck. The `Plugin` trait is as follows:

```rust
trait Plugin: Send + Sync + 'static {

	const PUBLISHER: &'static str;

	const NAME: &'static str;

	fn set_config(&self, config: JsonValue) -> StdResult<(), ConfigError>;

	fn queries(&self) -> impl Iterator<Item = NamedQuery>;

	fn explain_default_query(&self) -> Result<Option<String>>;

	fn default_policy_expr(&self) -> Result<String>;
}

pub struct NamedQuery {
	name: &'static str,
	inner: DynQuery,
}

type DynQuery = Box<dyn Query>;
```

The associated strings `PUBLISHER` and `NAME` allow you to declare the publisher
and name of the plugin, respectively.

The `set_config()` function allows Hipcheck users to pass a set of `String`
key-value pairs to your plugin as a configuration step before any endpoints are
queried. On success, simply return `Ok(())`. If the contents of the `config`
JSON value do not match what you expect, return a `ConfigError` enum variant to
describe why.

Your implementation of `queries()` is what actually binds each of your `impl
Query` structs to the plugin. As briefly mentioned above, query endpoints have
names, with up to one query allowed be unnamed (`name` is an empty string) and
thus designated as the "default" query for the plugin. Due to limitations of
Rust, the SDK must introduce a `NamedQuery` struct to bind a name to the query
structs. Your implementation of `queries()` will, for each `impl Query` struct,
instantiate that struct, then use that to create a `NamedQuery` instance with
the appropriate `name` field. Finally, return an iterator of all the
`NamedQuery` instances.

Plugins are not required to declare a default query endpoint, but plugins
designed for "top-level" analysis (namely those that are not explicitly
designed to provide data to other plugins) are highly encouraged to do so.
Furthermore, it is highly suggested that the default query endpoint is designed
to take the `Target` schema (@Todo - link to it), as this is the object type
passed to the designated query endpoints of all "top-level" plugins declared in
the Hipcheck policy file.

If you do define a default query endpoint, `Plugin::explain_default_query()`
should return a `Ok(Some(_))` containing a string that explains the default
query.

Lastly, if yours is an analysis plugin, users will need to write [policy
expressions](policy-expr) to interpret your plugin's output. In many cases, it
may be appropriate to define a default policy expression associated with your
default query endpoint so that users do not have to write one themselves. This
is the purpose of `default_policy_expr()`. This function will only ever be
called by the SDK after `set_config()` has completed, so you can also take
configuration parameters to influence the value returned by
`default_policy_expr().` For example, if the output of your plugin will
generally will be compared against an integer/float threshold, you can return a
`(lte $ <THRESHOLD>)` where `<THRESHOLD>` may be a value received from
`set_config()`.

### Running Your Plugin

At this point you now have a struct that implements `Plugin`. The last thing to
do is write some boilerplate code for starting the plugin server. The Rust SDK
exposes a `PluginServer` type as follows:

```rust
pub struct PluginServer<P> {
	plugin: Arc<P>,
}

impl<P: Plugin> PluginServer<P> {
	pub fn register(plugin: P) -> PluginServer<P> {
		...
	}

	pub async fn listen(self, port: u16) -> Result<()> {
		...
	}
}
```

So, once you have parsed the port from the CLI `--port <PORT>` flag that
Hipcheck passes to your plugin, you simply pass an instance of your `impl
Plugin` struct to `PluginServer::register()`, then call `listen(<PORT>).await`
on the returned `PluginServer` instance. This function will not return until
the gRPC channel with Hipcheck core is closed.

### Testing Plugin Endpoints

While working on your plugin implementation, it may be useful to unit-test
your query endpoint logic instead of having to test it indirectly through
an `hc check` analysis run. For this purpose, the Rust SDK offers a way to
"mock" responses from calls to the `PluginEngine.query()` function that your
query endpoint may make throughout its execution.

When in a `#cfg[test]` context or the Rust SDK `mock_engine` feature is enabled,
the `PluginEngine::mock(mock_responses: MockResponses) -> PluginEngine`
constructor becomes available. This constructor takes a `MockResponses` object
that acts as a dictionary of query targets to response values. `MockResponses`
is type-defined as follows:

```rust
pub struct MockResponses(pub(crate) HashMap<(QueryTarget, JsonValue), Result<JsonValue>>);
```

Looking at it, `MockResponses` takes as keys `QueryTarget`, `JsonValue` pairs
that represent the target plugin endpoint and the key that would be passed to
it. The value of the internal map is the mock response object for
`PluginEngine.query()` to return.  Here is an example of constructing one such
object:

```rust
fn mock_responses() -> Result<MockResponses, Error> {
    let mut mock_responses = MockResponses::new();
    mock_responses.insert("mitre/linguist", PathBuf::from("foo.java"), Ok(true))?;
    mock_responses.insert("mitre/linguist", PathBuf::from("bar.java"), Ok(true))?;
    mock_responses.insert("mitre/linguist", PathBuf::from("baz.java"), Ok(true))?;
    mock_responses.insert("mitre/linguist", PathBuf::from("oof.txt"), Ok(false))?;
    Ok(mock_responses)
}
```

This function sets up mock responses for calls to the default query endpoint of
the `mitre/linguist` plugin for four different keys. For example, with a
`PluginEngine` constructed using this `MockResponses` instance, a call to
`engine.query("mitre/linguist", PathBuf::from("oof.txt"))` would return
`Ok(false)` immediately without attempting to contact the Hipcheck core over
gRPC. The SDK offers the `MockResponses.insert()` function for building up
key/value pairs, which takes the query endpoint target, key, and output as
separate parameters. Each of the key and output types must be serializable to a
`serde_json::JsonValue` object.

Within a given test case, a `PluginEngine` instance can be mocked using an
instance of the above type as follows:

```rust
	let mut engine = PluginEngine::mock(mock_responses().unwrap());
```

From then on, the engine can be used, for example, as follows:

```rust
let freqs = commit_churns(&mut engine, key).await.unwrap();
````

Assertion statements can then be run on the output, namely `freqs` in this example:

```rust
	assert_eq!(freqs.len(), 2);
    assert_eq!(freqs[0].churn, -1.0);
    assert_eq!(freqs[1].churn, 1.0);
```

To inspect the concerns that would have been recorded during query execution
via `engine.record_concern()`, you can call the special testing-only function
`PluginEngine.get_concerns(&self) -> &[String]`.

And that's all there is to it! Happy plugin development!
