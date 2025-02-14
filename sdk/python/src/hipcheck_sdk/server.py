from abc import ABC
from typing import List, Dict, Union, Optional
import concurrent
import time

import grpc
import json

import hipcheck_sdk.gen as gen
from hipcheck_sdk.error import ConfigError, to_set_config_response
from hipcheck_sdk.query import Query, query, query_registry

class Plugin(ABC):

    # Ensure that subclasses have required class variables
    def __init_subclass__(cls, **kwargs):
        for required in ('name', 'publisher',):
            try:
                getattr(cls, required)
            except AttributeError:
                raise TypeError(f"Can't instantiate abstract class {cls.__name__} without {required} attribute defined")
        return super().__init_subclass__(**kwargs)

    # Allowed to raise a ConfigError
    def set_config(self, config: dict):
        pass

    def default_policy_expr(self) -> Optional[str]:
        return None

    def explain_default_query(self) -> Optional[str]:
        return None

    def queries(self) -> List[Query]:
        global query_registry
        return list(query_registry.values())

    def default_query(self) -> Optional[Query]:
        queries = self.queries()
        for q in queries:
            if name.is_default():
                return q
        return None

class PluginServer(gen.PluginServiceServicer):

    def __init__(self, plugin: Plugin):
        self.plugin = plugin

    def register(plugin: Plugin):
        return PluginServer(plugin)

    def listen(self, port: int):
        server = grpc.server(concurrent.futures.ThreadPoolExecutor(max_workers=10))
        gen.add_PluginServiceServicer_to_server(self, server)
        server.add_insecure_port(f"[::]:{port}")
        server.start()
        server.wait_for_termination()

    def GetQuerySchemas(self, request, context):
        for q in self.plugin.queries():
            key_schema = json.dumps(q.key_schema)
            output_schema = json.dumps(q.output_schema)
            yield gen.GetQuerySchemasResponse(
                    query_name=q.name,
                    key_schema=key_schema,
                    output_schema=output_schema)

    def SetConfiguration(self, request, context):
        config = json.loads(request.configuration)
        try:
            result = self.plugin.set_config(config)
            return gen.SetConfigurationResponse(
                    status=gen.ConfigurationStatus.CONFIGURATION_STATUS_NONE,
                    message=""
                )
        except ConfigError as e:
            return to_set_config_response(e)

    def GetDefaultPolicyExpression(self, request, context):
        return gen.GetDefaultPolicyExpressionResponse(
                policy_expression=self.plugin.default_policy_expr())

    def ExplainDefaultQuery(self, request, context):
        return gen.ExplainDefaultQueryResponse(
                explanation=self.plugin.explain_default_query())

    def InitiateQueryProtocol(self, request_iterator, context):
        print("InitQueryProtocol", type(request_iterator), type(context))
        pass
