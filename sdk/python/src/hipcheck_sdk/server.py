from abc import ABC
from typing import List, Dict, Union, Optional
import concurrent
import time

import asyncio
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

class HcSessionSocket:

    def __init__(self, stream, context):
        self.stream = stream
        self.context = context
        self.out = asyncio.Queue()

    def get_queue(self):
        return self.out

    async def run(self):
        # Outstanding issue in tonic crate used by Hipcheck core for gRPC:
        #   https://github.com/hyperium/tonic/issues/515
        # We have to send *something* otherwise the stream creation gets
        # blocked on the tonic side.
        query = gen.Query(
                id=0,
                state=gen.QueryState.QUERY_STATE_UNSPECIFIED,
                publisher_name="",
                plugin_name="",
                query_name="",
                key="",
                output="",
                split=False)
        await self.out.put(query)

        async for request in self.stream:
            query = request.query
            print("Got query id: ", query.id)
            query.state = gen.QueryState.QUERY_STATE_COMPLETE
            query.output = query.key
            query.key = ""
            await self.out.put(query)


class PluginServer(gen.PluginServiceServicer):

    def __init__(self, plugin: Plugin):
        self.plugin = plugin

    def register(plugin: Plugin):
        return PluginServer(plugin)

    def listen(self, port: int):
        async def inner(s: PluginServer, port: int):
            server = grpc.aio.server()
            # concurrent.futures.ThreadPoolExecutor(max_workers=10))
            gen.add_PluginServiceServicer_to_server(self, server)
            server.add_insecure_port(f"[::]:{port}")
            await server.start()
            await server.wait_for_termination()
        asyncio.run(inner(self, port))

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

    async def InitiateQueryProtocol(self, stream, context):
        # Outstanding issue in tonic crate used by Hipcheck core for gRPC:
        #   https://github.com/hyperium/tonic/issues/515
        # We have to send *something* otherwise the stream creation gets
        # blocked on the tonic side.
        session_socket = HcSessionSocket(stream, context)
        out_queue = session_socket.get_queue()

        socket_task = asyncio.create_task(session_socket.run())
        while True:
            query = await out_queue.get()
            yield gen.InitiateQueryProtocolResponse(query=query)
            out_queue.task_done()
#
#
#             async for out in out_queue:
#                 yield gen.InitiateQueryProtocolResponse(query=query)
#
#        async def inner(stream, context):
#            query = gen.Query(
#                    id=0,
#                    state=gen.QueryState.QUERY_STATE_UNSPECIFIED,
#                    publisher_name="",
#                    plugin_name="",
#                    query_name="",
#                    key="",
#                    output="",
#                    split=False)
#            yield gen.InitiateQueryProtocolResponse(query=query)
#            for request in stream:
#                query = request.query
#                print("Got query id: ", query.id)
#                query.state = gen.QueryState.QUERY_STATE_COMPLETE
#                query.output = query.key
#                query.key = ""
#                yield gen.InitiateQueryProtocolResponse(query=query)
#        return inner(stream, context)
    # async def InitiateQueryProtocol(self, stream, context):
    #     print(stream)
    #     async for request in stream:
    #         query = request.query
    #         print("Got query id: ", query.id)
    #         query.state = gen.QueryState.QUERY_STATE_COMPLETE
    #         query.output = query.key
    #         query.key = ""
    #         yield gen.InitiateQueryProtocolResponse(query)

        # print(dir(stream))
        # request = await stream.recv()
        # await stream.send_message(gen.InitiateQueryProtocolResponse(query))
        # session_tracker: Dict[int,

        # for request in request_iterator:
        #     query = request.query
            # Add to existing session queue or create new one

            # For all available messages from each plugin, yield message
            # Might need to do chunking here instead of in session

        # print("InitQueryProtocol", type(request_iterator), type(context))
        # pass
