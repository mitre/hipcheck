# SPDX-License-Identifier: Apache-2.0

import functools
import typing
from typing import Dict, Callable
from dataclasses import dataclass

import pydantic

from hipcheck_sdk.engine import PluginEngine


# Class to encapsulate information about a `@query`-decorated function, thus
# declared to be an endpoint for this plugin.
@dataclass
class Endpoint:
    # The name of the endpoint. If the default endpoint, name is ""
    name: str
    # The actual function that implements this endpoint
    func: Callable
    # The JSON Schema for the expected key
    key_schema: dict
    # The JSON Schema for the produced output object
    output_schema: dict

    def is_default(self):
        return name == ""


# A global registry of all detected `@query`-decorated functions in this plugin.
# Used by the default `Plugin` class implementation to implement `queries()` and
# `schemas()` functions.
query_registry: Dict[str, Endpoint] = {}


# Gets the JSON Schema for a Python object type. If the type is a child of
# pydantic.BaseModel, use that. Otherwise, try to derive the schema.
def get_json_schema_for_type(ty):
    if issubclass(ty, pydantic.BaseModel):
        return ty.model_json_schema()
    else:
        adapter = pydantic.TypeAdapter(ty)
        return adapter.json_schema()


# Add the function to `query_registry`. If `key_schema` or `output_schema` are None,
# try to derive the schema.
def register_query(func, default, key_schema, output_schema):
    global query_registry

    # Validate that func has 2 positional args
    if func.__code__.co_argcount != 2:
        raise TypeError("query function must have exactly 2 positional arguments")
    var_names = func.__code__.co_varnames
    hints = typing.get_type_hints(func)

    if key_schema is None:
        # Try to derive from function
        key_hint = hints[var_names[1]]
        key_schema = get_json_schema_for_type(key_hint)

    if output_schema is None:
        if "return" not in hints:
            raise TypeError(
                "cannot deduce query output type without return type hint on signature"
            )
        out_hint = hints["return"]
        output_schema = get_json_schema_for_type(out_hint)

    key = func.__name__
    if default:
        if "" in query_registry:
            raise TypeError("default query already defined")
        key = ""
    query_registry[key] = Endpoint(key, func, key_schema, output_schema)


# Decorator function for query endpoints. Endpoint functions must have the following
# signature:
#   async fn <QUERY_NAME>(engine: hipcheck_sdk.engine.PluginEngine, key: <TYPE>) -> <TYPE>
#
# The decorator allows arguments: `default: bool` to indicate the query should be the
# default query for the plugin; `key_schema: Optional[dict]` to denote the expected
# key JSON schema; `output_schema`, works the same as `key_schema` for output. If
# `key_schema` and/or `output_schema` are left blank, they are derived from the type
# hints on the function. Otherwise an error is raised.
def query(f_py=None, default=False, key_schema=None, output_schema=None):
    global query_registry
    assert callable(f_py) or f_py is None
    registry = {}

    def _decorator(func):
        register_query(func, default, key_schema, output_schema)

        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            return func(*args, **kwargs)

        return wrapper

    return _decorator(f_py) if callable(f_py) else _decorator
