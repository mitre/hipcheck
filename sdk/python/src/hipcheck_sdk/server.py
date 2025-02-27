from abc import ABC
from typing import List, Dict, Union, Optional
import concurrent
import time
import signal

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

    async def run_inner(self):
        # Outstanding issue in tonic crate used by Hipcheck core for gRPC:
        #   https://github.com/hyperium/tonic/issues/515
        # We have to send *something* otherwise the stream creation gets
        # blocked on the tonic side.
        # ID currently 0 so that it gets ignored by Hipcheck core, but that's
        #   a bit hacky.
        query = gen.Query(
                id=0,
                state=gen.QueryState.QUERY_STATE_UNSPECIFIED,
                publisher_name="",
                plugin_name="",
                query_name="",
                key=[],
                output=[],
                split=False)
        await self.out.put(query)

        async for request in self.stream:
            query = request.query
            print("Got query id: ", query.id)
            query.state = gen.QueryState.QUERY_STATE_REPLY_COMPLETE
            query.output.append(query.key[0])
            dir(query.key)
            query.key.clear()
            print("Python handler putting query resp")
            await self.out.put(query)
            print("Python handler finished putting")
        print("Stream closed, exiting")

    async def run(self):
        try:
            await self.run_inner()
        except Exception as e:
            query = gen.Query(
                id=1,
                state=gen.QueryState.QUERY_STATE_UNSPECIFIED,
                publisher_name="",
                plugin_name="",
                query_name="",
                key=[""],
                output=[f"HcSessionSocket error: {e}"],
                split=False)
            await self.out.put(query)
        finally:
            # Shut down queue so that PluginServer also closes.
            # queue.shutdown() available in 3.13, but we are using
            # a sentinel None value for now
            await self.out.put(None)

class PluginServer(gen.PluginServiceServicer):

    def __init__(self, plugin: Plugin):
        self.plugin = plugin

    def register(plugin: Plugin):
        return PluginServer(plugin)

    def listen(self, port: int):
        async def inner(s: PluginServer, port: int):
            # Create server
            server = grpc.aio.server()
            gen.add_PluginServiceServicer_to_server(self, server)
            server.add_insecure_port(f"[::]:{port}")
            await server.start()

            # Define handler func to stop server
            async def stop_server():
                print("Stopping server")
                await server.stop(1)

            # Register handler
            loop = asyncio.get_event_loop()
            for signame in ('SIGINT', 'SIGTERM'):
                loop.add_signal_handler(getattr(signal, signame),
                        lambda: asyncio.create_task(stop_server()))
            s.stop_queue = asyncio.Queue()

            # Wait for either the server to terminate, or for a single queue object
            #   that notifies us to stop the server
            wait_server_task = asyncio.create_task(server.wait_for_termination())
            notify_stop_task = asyncio.create_task(self.stop_queue.get())
            done, pending = await asyncio.wait([wait_server_task, notify_stop_task],
                    return_when=asyncio.FIRST_COMPLETED)

            # If the "wait for server" task is still pending, we got notifed by the stop_queue,
            #   so trigger server shutdown
            if wait_server_task in pending:
                await stop_server()
                # Now that we have called server.stop, the wait_server task should finish quickly
                await wait_server_task

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
        session_socket = HcSessionSocket(stream, context)
        out_queue = session_socket.get_queue()

        socket_task = asyncio.create_task(session_socket.run())
        while True:
            query = await out_queue.get()
            # In 3.13 there is QueueShutDown to signal this, but
            #   to not require 3.13 we are using a sentinel 'None'
            #   value instead
            if query is None:
                break
            yield gen.InitiateQueryProtocolResponse(query=query)
            out_queue.task_done()
        # We currently have the semantics that when the query protocol
        # with HC core closes, the plugin must shut down.
        await self.stop_queue.put(None)
