---
title: The Python Plugin SDK
weight: 3
---

# The Python Plugin SDK

The Hipcheck team maintains a Python library that provides developers with tools
to greatly simplify plugin development. This section provides a high-level guide
on how to implement a Hipcheck plugin using the library.

This guide assumes familiarity with the Python language, async Python using
[asyncio][asyncio], and Python project management. We leave it to the developer
to select the right Python project dependency and packaging management tool
that is right for them, from among the many that exist. For the purpose of
[packaging and releasing your plugin][plugin-release], we suggest using one
that will make it easy to build a wheel from your plugin source code.

To get started, first install the SDK (`hipcheck-sdk`) into your Python
environment. We publish the SDK on PyPI
[here](https://pypi.org/project/hipcheck-sdk/).

## Implementation Overview

General usage of the Python SDK is as follows. A user defines a subclass of the
`Plugin` type, and implements necessary functions and class variables that provide
information about the plugin to Hipcheck core. The user also defines a series of
functions that act as the query endpoints exposed by the plugin (in other words,
the functions that users of the plugin can call). These functions must have a
specific signature and are tagged with the `@query` function decorator.

The plugin's `__main__` script must parse arguments passed by Hipcheck core through
the CLI, register their `Plugin` subclass with the `PluginServer`, and start the
`PluginServer` listening for a gRPC connection from Hipcheck core. If no
customization of this behavior is desired, the Python SDK provides a
`run_server_for(plugin: Plugin)` convenience function to perform the above

## Defining Query Endpoints

The Hipcheck plugin communication protocol allows a plugin to expose multiple
named query endpoints that can be called by Hipcheck core or other plugins. In
the Python SDK, these endpoints are functions marked with the `@query`
decorator.

### Query Endpoint Signature

To be a valid query endpoint, a function must have the following
signature:

```python
from hipcheck_sdk import PluginEngine, query

@query
async def <QUERY_NAME>(engine: PluginEngine, key: <KEY_TYPE>) -> <OUTPUT_TYPE>
```

`<QUERY_NAME>`, the name of the function, is also used as the name of the query
endpoint as called by other plugins (unless the query is marked as the default
query, see [below](#query-decorator-parameters)). The `engine` parameter is
provided to allow the query endpoint to query other plugins, while `key` is the
input to your endpoint.

The `key` parameter and return type of the function should be type-hinted, as
the hints are used to derive input and output JSON schemas for the endpoint.
Internally, schema derivation is implemented using the `pydantic` library as
follows:

```python
def get_json_schema_for_type(ty):
	if issubclass(ty, pydantic.BaseModel):
		return ty.model_json_schema()
    else:
        adapter = pydantic.TypeAdapter(ty)
        return adapter.json_schema()
```

Thus, if your query endpoint takes or returns a complex type and you are having
trouble with the `pydantic.TypeAdapter`, you may consider redefining the type as
a subclass of `pydantic.BaseModel`.

#### Query Decorator Parameters

The `@query` decorator has the following optional parameters:
- `default: bool` - `True` if the plugin should be the default endpoint for this
	plugin. Defaults to `False`.
- `key_schema: dict` - Represents the JSON schema for the `key` parameter. If provided,
	this field is respected over the type hint on the `key` parameter in the function
	declaration. Can be used in place of the type hint or to override the derived
	JSON schema.
- `output_schema: dict` - Same behavior as `key_schema` but for the endpoint's return type.

For an endpoint marked `@query(default=True)`, that endpoint becomes the
default endpoint for the plugin, meaning that if Hipcheck core or another
plugin queries your plugin without providing an endpoint name, this endpoint
will be invoked. Only one endpoint may be marked as the default, marking
multiple will result in an error.

#### Using the Engine Handle

The `engine: PluginEngine` parameter is a handle to allow the query endpoint to
query other plugins' endpoints. This can be done as follows:

```python
	result = await engine.query(<TARGET_STR>, <KEY>)
```

`<TARGET_STR>` is a `str` of the form `<PUBLISHER>/<PLUGIN>/<QUERY>` (e.g.
`mitre/example_plugin/my_endpoint`) that indicates the target plugin and
endpoint to query.  If you are querying the plugin's default query, you may omit
the final slash and `<QUERY>` (e.g. `mitre/example_plugin`).

To supply a multiple keys in a single call, you may use the
`engine.batch_query()` function which takes a list of keys instead of a single
key. The returned list of results is in order corresponding with the order of
the keys.

#### Error Handling

Wherever possible, when an error occurs, please raise an error using an
appropriate subclass of the `hipcheck_sdk.error.SdkError` class. The API docs
can be found [here](todo).

## Defining the Plugin Subclass

Each plugin using the Python SDK must define a subclass of
`hipcheck_sdk.Plugin`; we will refer to this as the plugin class. The plugin
class must define two class variable strings, `publisher` and `name`, which
declare the plugin's publisher and name respectively. For example:

```python
from hipcheck_sdk import Plugin

class ExamplePlugin(Plugin):
    name = "example"
    publisher = "mitre"
```

The existence of these class variables will be checked at runtime via
introspection.

If you have already defined a valid query endpoint function, at this point you
have a valid Hipcheck plugin. When your pluging gets registered with the
`PluginServer`, all functions loaded in the Python interpreter that have a
`@query` decorator will be exposed as part of your set of query endpoints.

Most people will want to customize at least some aspects of the plugin behavior
by overriding other functions of the `Plugin` class. We describe how to do this
below.

### Setting Configuration

Plugins may require or allow users to supply a map of configuration keys and
values at startup. These are simple `str` to primtive pairs; if your plugin
requires more complex configuration, we prescribe designing a config file
format and having users specify the path to the file as a configuration
key/value pair. To define the logic for setting your plugin's configuration,
define the following function override in your plugin class:

```python
	def set_config(self, config: dict):
		# Your implementation here
```

You should raise exceptions that are subclasses of the
`hipcheck_sdk.error.ConfigError` as appropriate while parsing the `config:
dict`. A successful configuration should not return anything.

### Setting Default Policy Expression

Query endpoints return data that is used in the Hipcheck analysis; the default
query endpoint exposed by a plugin is the one most likely to be a top-level
analysis in a Hipcheck policy file. Thus, the SDK exposes a way for users to
define a default policy expression for the default query endpoint, in the case
that the policy file does not specify one. To do so, define the following
function override:

```python
 	def default_policy_expr(self) -> Optional[str]:
		# Your implementation here
```

For example:

```python
 	def default_policy_expr(self) -> Optional[str]:
		# If the user configured the plugin with a threshold in
		# `set_config`, return a policy expression based on that
		# value. Otherwise, there is no default.
		if self.opt_threshold is None:
            return None
        else:
            return f"(lte $ {self.opt_threshold})"

```

### Explaining the Default Query

If your plugin defines a default query endpoint, it is good practice to implement the following function in your plugin class:

```python
	def explain_default_query(self) -> Optional[str]:
		# Your implementation here
```

This describes the type of data returned by the default query, and is used in
the automatic generation of an English explanation when a policy expression
passes or fails. You may consider examining other plugin implementations in the
Hipcheck repository or testing on your own to ensure that the `str` returned by
this function harmonizes with the English explanation logic.

## Implementing Plugin Server Startup

As mentioned in the overview, at startup a plugin must parse CLI arguments,
register the plugin subclass with the `PluginServer`, and start the
`PluginServer` listening on a port specified by the CLI arguments. For most
users, you can simply call the `run_server_for()` function to do all this:

```python
if __name__ == "__main__":
    run_server_for(ExamplePlugin())
```

The function takes an instance of your plugin subclass.

That's all for the basics, happy plugin development!

[asyncio]: https://docs.python.org/3/library/asyncio.html
[plugin-release]: @/docs/guide/making-plugins/release.md
[policy-expr]: @/docs/guide/config/policy-expr.md
[policy-file]: @/docs/guide/config/policy-file.md
