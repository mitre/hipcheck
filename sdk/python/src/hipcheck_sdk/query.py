import functools
import typing
from typing import Dict, Callable
from dataclasses import dataclass

import pydantic

from hipcheck_sdk.engine import PluginEngine

@dataclass
class Query:
    name: str
    func: Callable
    key_schema: dict
    output_schema: dict

    def is_default(self):
        return name == ''


query_registry: Dict[str, Query] = {}


def get_json_schema_for_type(ty):
    if issubclass(ty, pydantic.BaseModel):
        return ty.model_json_schema()
    else:
        adapter = pydantic.TypeAdapter(ty)
        return adapter.json_schema()


def register_query(func, default, key_schema, output_schema):
    # @Todo - here we perform different behavior based on arguments and func
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
        if 'return' not in hints:
            raise TypeError("cannot deduce query output type without return type hint on signature")
        out_hint = hints['return']
        output_schema = get_json_schema_for_type(out_hint)

    key = func.__name__
    if default:
        if "" in query_registry:
            raise TypeError("default query already defined")
        key = ""
    query_registry[key] = Query(key, func, key_schema, output_schema)


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
