# SPDX-License-Identifier: Apache-2.0

import functools
import typing
from typing import Dict, Optional, Callable
from dataclasses import dataclass

import pydantic


@dataclass
class Endpoint:
    """
    Class to encapsulate information about a `@query`-decorated function, thus
    declared to be an endpoint for this plugin.

    :meta private:
    """

    # The name of the endpoint. If the default endpoint, name is ""
    name: str
    # The actual function that implements this endpoint
    func: Callable
    # The JSON Schema for the expected key
    key_schema: dict
    # The object type to convert json to
    key_type: type
    # The JSON Schema for the produced output object
    output_schema: dict

    def is_default(self):
        return self.name == ""


# A global registry of all detected `@query`-decorated functions in this plugin.
# Used by the default `Plugin` class implementation to implement `queries()` and
# `schemas()` functions.
query_registry: Dict[str, Endpoint] = {}


def get_json_schema_for_type(ty: type) -> dict:
    """
    Gets the JSON Schema for a Python object type. If the type is a child of
    pydantic.BaseModel, use that. Otherwise, try to derive the schema.

    :param type ty: The type for which to derive a JSON schema
    :return: A jsonable dict representing the schema for `ty`

    :meta private:
    """
    if issubclass(ty, pydantic.BaseModel):
        return ty.model_json_schema()
    else:
        adapter = pydantic.TypeAdapter(ty)
        return adapter.json_schema()


def register_query(func, default, key_schema, output_schema):
    """
    Add the function to `query_registry`. If `key_schema` or `output_schema`
    are None, try to derive the schema.

    :meta private:
    """
    global query_registry

    # Validate that func has 2 positional args
    if func.__code__.co_argcount != 2:
        raise TypeError("query function must have exactly 2 positional arguments")
    var_names = func.__code__.co_varnames
    hints = typing.get_type_hints(func)

    if key_schema is None:
        # Try to derive from function
        key_type = hints[var_names[1]]
        key_schema = get_json_schema_for_type(key_type)
    else:
        # We do not generate a class definition for key_schema due to
        # potential code execution security concerns
        key_type = None

    if output_schema is None:
        if "return" not in hints:
            raise TypeError(
                "cannot deduce query output type without return type hint on signature"
            )
        out_hint = hints["return"]
        output_schema = get_json_schema_for_type(out_hint)

    key = func.__name__

    # Create an additional entry that maps this func to the empty string so queries
    #   received without an endpoint name will call it
    if default:
        if "" in query_registry:
            raise TypeError("default query already defined")
        query_registry[""] = Endpoint("", func, key_schema, key_type, output_schema)

    query_registry[key] = Endpoint(key, func, key_schema, key_type, output_schema)


def query(
    f_py=None,
    default: bool = False,
    key_schema: Optional[dict] = None,
    output_schema=None,
):
    """
    Decorator function for query endpoints. Endpoint functions must have the
    following signature:

    .. code-block:: python

        async fn <QUERY_NAME>(engine: hipcheck_sdk.engine.PluginEngine, key: <TYPE>) -> <TYPE>

    :param bool default: Whether this endpoint is the default for the plugin
    :param dict key_schema: A jsonable dict representing the schema for the key
        to this enpdpoint. If `None`, derive from type hint on key parameter of
        function. If a schema is supplied explicitly instead of having the SDK
        use the type hint, the object passed to the query func will not be
        automatically converted to a class instance.
    :param dict output_schema: A jsonable dict representing the schema for the
        return value of this endpoint. If `None`, derive from type hint on
        return value of function.
    :raises TypeError: The function lacked type hints or a JSON schema could
        not be derived from them
    """
    global query_registry
    assert callable(f_py) or f_py is None

    def _decorator(func):
        register_query(func, default, key_schema, output_schema)

        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            return func(*args, **kwargs)

        return wrapper

    return _decorator(f_py) if callable(f_py) else _decorator
