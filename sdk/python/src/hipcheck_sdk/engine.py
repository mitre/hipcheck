# SPDX-License-Identifier: Apache-2.0

from typing import List, Tuple, Dict, Optional

import asyncio
import logging
import pydantic

from hipcheck_sdk import *
import hipcheck_sdk.gen as gen
from hipcheck_sdk.chunk import *
from hipcheck_sdk.error import *

logger = logging.getLogger(__name__)


def split_once(s: str, delim: str) -> Tuple[str, Optional[str]]:
    """
    Split `s` at first instance of `delim` and returns the resulting substrings

    :param str s: The input string.
    :param str delim: The delimiter at which to The concern to be recorded.
    :return: A tuple containing the substrings split at the first instance
        the delimiter. If the delimiter was not found, the second element of
        the tuple is `None`.

    :meta private:
    """
    res = s.split(delim, 1)
    if len(res) != 2:
        res.append(None)
    return tuple(res)


def parse_target_str(target: str) -> Tuple[str, str, str]:
    """
    Return a tuple of (publisher, plugin, endpoint_name) from parsed from
    a query target string (e.g. "mitre/example/query"). If the string contains
    only one slash return "" for the endpoint name, indicating the default
    query endpoint.

    :param str target: A string of the form `<PUBLISHER>/<PLUGIN>/<ENDPOINT>`,
        where `/<ENDPOINT>` may be omitted if targeting a default endpoint.
    :return: A tuple of strings representing the publisher, plugin, and
        endpoint name

    :meta private:
    """
    publisher, rest = split_once(target, "/")
    if rest is None:
        raise InvalidQueryTargetFormat()
    plugin, name = split_once(rest, "/")
    if name is None:
        name = ""
    return (publisher, plugin, name)


def deserialize_key(json_str: str, target_type: type) -> object:
    """
    Try to turn a JSON string into a given type

    :param str json_str: The JSON string to be read
    :param type target_type: The type to produce
    :return: An instance of `target_type`

    :meta private:
    """
    if issubclass(target_type, pydantic.BaseModel):
        return target_type.model_validate_json(json_str)
    elif target_type in [int, bool, float, list, dict]:
        return json.loads(json_str)
    else:
        return json.loads(json_str, cls=target_type)


class QueryBuilder:
    """
    An alternative to calling `PluginEngine.batch_query()`, an instance of
    this class is returned by `PluginQuery.batch()`, and allows plugin
    authors to build up a list of keys for a batch query over time, then call
    `QueryBuilder.send()` to send them in a single message to Hipcheck.
    """

    def __init__(self, engine, target: str):
        """
        :meta private:
        """
        self.engine = engine
        # The keys that will be added to a batch query
        self.keys = []
        # The target endpoint that all keys will be used to query
        self.target = target

    def query(self, key: object) -> int:
        """
        Adds a key to the batch query being built by this object

        :param object key: The key to add to the query batch.
        :return: The index of the list at which the key was added. Can be used
            to index the output of `send()` to get the corresponding output for
            that key.
        """
        l = len(self.keys)
        self.keys.append(key)
        return l

    async def send(self) -> List[object]:
        """
        Sends all keys aggregated with `query()` in a single batch query to
        Hipcheck core.

        :return: The list of output objects for the keys aggregated in this object
        :raises SdkError:
        """
        return await self.engine.batch_query(self.target, self.keys)


MockResponses = List[Tuple[Tuple[str, object], object]]


def find_response(m: MockResponses, key: Tuple[str, object]) -> Optional[object]:
    """
    Find a tuple `y` in `m` such that y[0] == key, and return y[1]. This acts
    like a pseudo-dictionary for storing mock responses to deal with the fact
    that lists/dicts/objects are not hashable by default.

    :param MockResponses m: The list of tuples to search
    :param tuple key: The key to check elements of `m` against
    :return: The second element of a tuple whose first element matches `key`. If none found, returns `None`.

    :meta private:
    """
    i = map(lambda x: x[1], filter(lambda y: y[0] == key, m))
    try:
        return next(i)
    except StopIteration:
        return None


class PluginEngine:
    """
    Manages a particular query session.

    An instance of this class invokes a query endpoint, passing a handle to
    itself. This allows the query endpoint to request information from other
    Hipcheck plugins as part of its logic.
    """

    def __init__(
        self,
        session_id: int = 0,
        tx: asyncio.Queue = None,
        rx: asyncio.Queue = None,
        drop_tx: asyncio.Queue = None,
        mock_responses: Optional[MockResponses] = None,
    ):
        """
        :meta private:
        """
        nones = [v is None for v in [tx, rx, drop_tx]]
        if any(nones) and not all(nones):
            raise UnspecifiedConfigError(
                msg="tx, rx, and drop_tx must all be None or all be asyncio.Queue objects"
            )

        self.id: int = session_id
        self.tx: asyncio.Queue = tx
        self.rx: asyncio.Queue = rx
        # So that we can remove ourselves when we get dropped
        self.drop_tx: asyncio.Queue = drop_tx
        self.concerns: List[str] = []
        # When unit testing, this enables the user to mock plugin responses to various inputs
        self.mock_responses = mock_responses

    def mock(mock_responses: List[Tuple[Tuple[str, object], object]] = []):
        """
        For unit testing purposes, construct a PluginEngine with a set of
        mock responses

        :param tuple mock_responses: A list of key, value pairs that maps
            queries to mock responses. Does not use a dict since many
            relevant types are not hashable and thus cannot be used as
            keys. The query is a tuple of a target string and a key
            object, the response is the output object for that query.

        :return: An instance of `PluginEngine`
        """
        # In `PluginEngine.query()` if mock_responses is None we try to query Hipcheck core
        #   which will obviously fail in a unit-testing context. Try to defend here against
        #   the user making a mistake; if `mock()` called we expect to always mock responses.
        if mock_responses is None:
            mock_responses = []
        return PluginEngine(mock_responses=mock_responses)

    # Convenience function to expose a `QueryBuilder` to make it easy to dynamically build
    # up multi-key queries against a single target and send them over gRPC in as few
    # gRPC calls as possible.
    def batch(self, target: str) -> QueryBuilder:
        """
        Create a `QueryBuilder` instance to dynamically aggregate keys for a
        batch query against `target` as opposed to using `PluginEngine.batch_query()`
        which requires having all keys available immediately

        :param str target: A string of the form `<PUBLISHER>/<PLUGIN>/<ENDPOINT>`,
            where `/<ENDPOINT>` may be omitted if targeting a default endpoint.
            Indicates the remote plugin endpoint to query.

        :return: An instance of `QueryBuilder`
        """
        return QueryBuilder(self, target)

    async def query_inner(self, target: str, keys: List[object]) -> List[object]:
        """
        :raises: SdkError

        :meta: private
        """
        # If using a mock engine, look to the `mock_responses` field for the query answer
        if self.mock_responses is not None:
            results = []
            for i in keys:
                opt_val = find_response(self.mock_responses, (target, i))
                if opt_val is None:
                    raise UnknownPluginQuery()
                else:
                    results.append(opt_val)
            return results

        else:
            # Normal execution, send messages to Hipcheck core to query other plugins

            publisher, plugin, name = parse_target_str(target)

            query = Query(
                id=self.id,
                direction=QueryDirection.REQUEST,
                publisher=publisher,
                plugin=plugin,
                query=name,
                key=keys,
                output=[],
                concerns=[],
            )

            await self.send(query)
            resp: Query = await self.recv()
            return list(resp.output)

    async def query(self, target: str, key: object) -> object:
        """
        Query another Hipcheck plugin endpoint `target` with key `input`

        :param str target: A string of the form `<PUBLISHER>/<PLUGIN>/<ENDPOINT>`,
            where `/<ENDPOINT>` may be omitted if targeting a default endpoint.
            Indicates the remote plugin endpoint to query.
        :param object key: The key for the query
        :return: The deserialized result

        :raises: SdkError
        """
        outputs = await self.query_inner(target, [key])
        return outputs[0]

    async def batch_query(self, target: str, keys: List[object]) -> List[object]:
        """
        Query another Hipcheck plugin endpoint `target` with a list of keys

        :param str target: Indicates the remote plugin endpoint to query. Has
            the same format requirements as `query()`
        :param list keys: The list of keys to send as a single batch query to
            Hipcheck core.
        :return: A list of output values corresponding to each element of key
            being applied to the target endpoint.

        :raises: SdkError
        """
        return await self.query_inner(target, keys)

    async def recv_raw(self) -> Optional[List[gen.Query]]:
        """
        :meta private:
        """
        out = []

        try:
            first = await self.rx.get()
        except Exception as e:
            # Underlying gRPC channel closed
            # @Todo - tighten this exception
            print(f"Recv exception: {e}")
            return None

        out.append(first)

        # If more messages in the queue, opportunistically read more
        while True:
            try:
                msg = self.rx.get_nowait()
            except asyncio.QueueEmpty:
                break
            except Exception as e:
                # @Todo - tighten this exception
                print(f"Recv exception: {e}")
                break
            out.append(msg)

        return out

    async def send_session_error(self, plugin):
        """
        :meta private:
        """
        query = gen.Query(
            id=self.id,
            state=gen.QUERY_STATE_UNSPECIFIED,
            publisher_name=plugin.publisher,
            plugin_name=plugin.name,
            query_name="",
            concern=self.take_concerns(),
            split=False,
        )
        await self.tx.put(query)

    async def recv(self) -> Optional[Query]:
        """
        :raises: SdkError
        :meta private:
        """
        synth = QuerySynthesizer()
        res: Optional[Query] = None
        while res is None:
            opt_msg_chunks = await self.recv_raw()
            if opt_msg_chunks is None:
                return None
            msg_chunks = opt_msg_chunks
            res = synth.add(msg_chunks)
        return res

    def record_concern(self, concern: str):
        """
        Records a concern that will be emitted in the final Hipcheck report.
        Intended for use within a `@query`-decorated endpoint function.

        :param str concern: The concern to be recorded
        """
        self.concerns.append(concern)

    def take_concerns(self):
        """
        :meta private:
        """
        out = self.concerns
        self.concerns = []
        return out

    async def send(self, query: Query):
        """
        Send a gRPC query from plugin to the hipcheck server

        :raises SdkError:
        :meta private:
        """
        query.id = self.id  # incoming id value is just a placeholder
        for pq in prepare(query):
            await self.tx.put(pq)

    async def handle_session_fallible(self, plugin):
        """
        :raises SdkError:
        :meta private:
        """
        query: Query = await self.recv()

        if query.direction == QueryDirection.RESPONSE:
            raise ReceivedReplyWhenExpectingSubmitChunk()

        name = query.query

        # Per RFD 0009, there should only be one query key per query
        if len(query.key) != 1:
            raise UnspecifiedQueryState()
        key = query.key[0]

        query = next((x for x in plugin.queries() if x.name == name), None)
        if query is None:
            raise UnknownPluginQuery()

        # None key type means they used an explicit key_schema so we leave as dict
        if query.key_type is not None:
            # Convert query as dict to target object schema
            try:
                key = deserialize_key(json.dumps(key), query.key_type)
            except Exception as e:
                logger.error(f"{e}")
                raise InvalidJsonInQueryKey()

        value = await query.func(self, key)

        out = Query(
            id=self.id,
            direction=QueryDirection.RESPONSE,
            publisher=plugin.publisher,
            plugin=plugin.name,
            query=name,
            key=[],
            output=[value],
            concerns=self.take_concerns(),
        )

        await self.send(out)

        # Notify HcSessionSocket that session is closed
        await self.drop_tx.put(self.id)

    async def handle_session(self, plugin):
        """
        :meta private:
        """
        try:
            await self.handle_session_fallible(plugin)
        # Errors that we raise intentionally
        except SdkError as e:
            logger.error(f"{e}")
            await self.send_session_error(plugin)
        # Other errors, such as syntactical ones
        except Exception as e:
            logger.error(f"{e}")
            await self.send_session_error(plugin)
        # except asyncio.QueueShutDown:
        #     return
