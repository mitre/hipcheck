# SPDX-License-Identifier: Apache-2.0

from abc import ABC
from typing import List, Dict, Union, Optional
import concurrent
import time
import signal
import logging

import asyncio
import grpc
import json

import hipcheck_sdk.gen as gen
from hipcheck_sdk.error import (
    ConfigError,
    to_set_config_response,
    ReceivedReplyWhenExpectingSubmitChunk,
)
from hipcheck_sdk.query import Endpoint, query, query_registry
from hipcheck_sdk.engine import PluginEngine
from hipcheck_sdk.chunk import Query

logger = logging.getLogger(__name__)


class Plugin(ABC):
    # Ensure that subclasses have required class variables `name` and `publisher`
    def __init_subclass__(cls, **kwargs):
        for required in (
            "name",
            "publisher",
        ):
            try:
                getattr(cls, required)
            except AttributeError:
                raise TypeError(
                    f"Can't instantiate abstract class {cls.__name__} without {required} attribute defined"
                )
        return super().__init_subclass__(**kwargs)

    # Handle setting configuration. The `config` parameter is a dict of `str, str` pairs.
    # Allowed to raise a ConfigError.
    def set_config(self, config: dict):
        pass

    # Get the plugin's default policy expression. This will only ever be called after
    # `Plugin.set_config()`. For more information on policy expression syntax, see the Hipcheck
    # website.
    def default_policy_expr(self) -> Optional[str]:
        return None

    # Get an unstructured description of what is returned by the plugin's default query.
    def explain_default_query(self) -> Optional[str]:
        return None

    # Get all the queries supported by the plugin. Each query endpoint in a plugin is a
    # function decorated with `@query`. This function returns an iterator containing one
    # `Endpoint` instance for each `@query` function defined in this plugin and imported
    # when the plugin server starts.
    def queries(self) -> List[Endpoint]:
        global query_registry
        return list(query_registry.values())

    # Get the plugin's default query, if it has one. The default query is a `@query`
    # function with `default=True` in the decorator arguments.
    def default_query(self) -> Optional[Endpoint]:
        queries = self.queries()
        for q in queries:
            if name.is_default():
                return q
        return None


# Manages incoming gRPC query messages in the bidirectional query protocol. Determines
# when to pass messages onto existing `PluginEngine` object queues or create a new
# `PluginEngine` to represent a new session. When `PluginEngine` objects close because
# the session ends, they put their ID on the `self.drop` Queue, so this object can
# clear their state from `self.sessions`.
class HcSessionSocket:
    def __init__(self, stream, context):
        self.stream = stream
        self.context = context
        self.out = asyncio.Queue()
        self.drop = asyncio.Queue()
        self.sessions: Dict[int, asyncio.Queue] = {}

    def get_queue(self):
        return self.out

    # Clean up completed sessions by going through all drop messages.
    async def cleanup_sessions(self):
        while not self.drop.empty():
            session_id = await self.drop.get()
            val = self.sessions.pop(session_id)
            if val is None:
                logger.warning(
                    "HcSessionSocket got request to drop a session that does not exist"
                )
                continue
            task, queue = val
            await task

    # Using the session tracker, determine if this message constitutes
    # a new session or should be passed to an existing one.
    def decide_action(self, query: Query) -> Optional[asyncio.Queue]:
        if query.id in self.sessions.keys():
            return self.sessions[query.id][1]

        if query.state in [
            gen.QueryState.QUERY_STATE_SUBMIT_IN_PROGRESS,
            gen.QueryState.QUERY_STATE_SUBMIT_COMPLETE,
        ]:
            return None

        raise ReceivedReplyWhenExpectingSubmitChunk()

    async def run_inner(self, plugin):
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
            split=False,
        )
        await self.out.put(query)

        async for request in self.stream:
            query = request.query

            # While we were waiting for a message, some session objects may have
            # dropped, handle them before we look at the ID of this message.
            # The downside of this strategy is that once we receive our last message,
            # we won't clean up any sessions that close after
            await self.cleanup_sessions()

            decision = self.decide_action(query)
            if isinstance(decision, asyncio.Queue):
                await decision.put(query)
            else:
                engine_queue = asyncio.Queue()
                session = PluginEngine(
                    session_id=query.id, tx=self.out, rx=engine_queue, drop_tx=self.drop
                )
                await engine_queue.put(query)

                task = asyncio.create_task(session.handle_session(plugin))

                self.sessions[query.id] = (task, engine_queue)

        logger.debug("Stream closed, exiting")

    async def run(self, plugin):
        try:
            await self.run_inner(plugin)
        except Exception as e:
            logger.error(f"{e}")
            query = gen.Query(
                id=1,
                state=gen.QueryState.QUERY_STATE_UNSPECIFIED,
                publisher_name="",
                plugin_name="",
                query_name="",
                key=[""],
                output=[f"HcSessionSocket error: {e}"],
                split=False,
            )
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

    def listen(self, port: int, host="127.0.0.1"):
        async def inner(s: PluginServer, port: int):
            # Create server
            server = grpc.aio.server()
            gen.add_PluginServiceServicer_to_server(self, server)
            server.add_insecure_port(f"{host}:{port}")
            await server.start()

            # Define handler func to stop server
            async def stop_server():
                await server.stop(1)

            # Register handler
            loop = asyncio.get_event_loop()
            for signame in ("SIGINT", "SIGTERM"):
                loop.add_signal_handler(
                    getattr(signal, signame), lambda: asyncio.create_task(stop_server())
                )
            s.stop_queue = asyncio.Queue()

            # Wait for either the server to terminate, or for a single queue object
            #   that notifies us to stop the server
            wait_server_task = asyncio.create_task(server.wait_for_termination())
            notify_stop_task = asyncio.create_task(self.stop_queue.get())
            done, pending = await asyncio.wait(
                [wait_server_task, notify_stop_task],
                return_when=asyncio.FIRST_COMPLETED,
            )

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
                query_name=q.name, key_schema=key_schema, output_schema=output_schema
            )

    def SetConfiguration(self, request, context):
        config = json.loads(request.configuration)
        try:
            result = self.plugin.set_config(config)
            return gen.SetConfigurationResponse(
                status=gen.ConfigurationStatus.CONFIGURATION_STATUS_NONE, message=""
            )
        except ConfigError as e:
            return to_set_config_response(e)

    def GetDefaultPolicyExpression(self, request, context):
        return gen.GetDefaultPolicyExpressionResponse(
            policy_expression=self.plugin.default_policy_expr()
        )

    def ExplainDefaultQuery(self, request, context):
        return gen.ExplainDefaultQueryResponse(
            explanation=self.plugin.explain_default_query()
        )

    async def InitiateQueryProtocol(self, stream, context):
        session_socket = HcSessionSocket(stream, context)
        out_queue = session_socket.get_queue()

        socket_task = asyncio.create_task(session_socket.run(self.plugin))
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
