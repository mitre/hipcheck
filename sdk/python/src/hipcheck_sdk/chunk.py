# SPDX-License-Identifier: Apache-2.0

# This file is roughly a direct port of `library/hipcheck-common/src/chunk.rs`
# and `library/hipcheck-common/src/types.rs`.

from typing import List, Optional, Tuple
from enum import Enum
from dataclasses import dataclass

import json

import hipcheck_sdk.gen as gen
from hipcheck_sdk.error import SdkError, InvalidJsonInQueryKey
from hipcheck_sdk import enabled

# Max size of a single GRPC message (4 MB)
GRPC_MAX_SIZE_BYTES: int = 1024 * 1024 * 4
GRPC_EFFECTIVE_MAX_SIZE: int = GRPC_MAX_SIZE_BYTES - 1024  # Minus one KB


class QueryDirection(Enum):
    REQUEST = 1
    RESPONSE = 2

    def try_from(value: gen.QueryState):
        match value:
            case gen.QUERY_STATE_UNSPECIFIED:
                raise UnspecifiedQueryState()
            case gen.QUERY_STATE_SUBMIT_IN_PROGRESS:
                raise UnexpectedRequestInProgress()
            case gen.QUERY_STATE_SUBMIT_COMPLETE:
                return QueryDirection.REQUEST
            case gen.QUERY_STATE_REPLY_IN_PROGRESS:
                raise UnexpectedReplyInProgress()
            case gen.QUERY_STATE_REPLY_COMPLETE:
                return QueryDirection.RESPONSE

    def into(self) -> gen.QueryState:
        match self:
            case QueryDirection.REQUEST:
                return gen.QUERY_STATE_SUBMIT_COMPLETE
            case QueryDirection.RESPONSE:
                return gen.QUERY_STATE_REPLY_COMPLETE


@dataclass
class Query:
    id: int
    direction: QueryDirection
    publisher: str
    plugin: str
    query: str
    key: List[object]
    output: List[object]
    concerns: List[str]

    def try_from(raw: gen.Query):
        # assert(len(raw.key) == len(raw.output))
        direction = QueryDirection.try_from(raw.state)

        def get_fields_rfd9(v: gen.Query) -> Tuple[List[object], List[object]]:
            keys = []
            for x in v.key:
                try:
                    val = json.loads(x)
                except Exception:
                    raise InvalidJsonInQueryKey()
                keys.append(val)

            outputs = []
            for x in v.output:
                try:
                    val = json.loads(x)
                except Exception:
                    raise InvalidJsonInQueryOutput()
                outputs.append(val)

            return (keys, outputs)

        def get_fields_compat(v: gen.Query) -> Tuple[List[object], List[object]]:
            try:
                s = "".join([k for k in v.key])
                json_key = json.loads(s)
            except Exception:
                raise InvalidJsonInQueryKey()
            try:
                s = "".join([k for k in v.output])
                json_out = json.loads(s)
            except Exception:
                raise InvalidJsonInQueryOutput()
            return ([json_key], [json_out])

        try:
            rfd9_res = get_fields_rfd9(raw)
        except SdkError as e:
            if enabled("rfd9_compat"):
                rfd9_res = get_fields_compat(raw)
            else:
                raise e

        key, output = rfd9_res

        return Query(
            id=raw.id,
            direction=direction,
            publisher=raw.publisher_name,
            plugin=raw.plugin_name,
            query=raw.query_name,
            key=key,
            output=output,
            concerns=list(raw.concern),
        )

    def try_into(self) -> gen.Query:
        state: gen.QueryState = self.direction.into()

        keys = []
        for key in self.key:
            try:
                keys.append(json.dumps(key))
            except Exception:
                raise InvalidJsonInQueryKey()

        outputs = []
        for output in self.output:
            try:
                outputs.append(json.dumps(output))
            except Exception:
                raise InvalidJsonInQueryOutput()

        raw = gen.Query(
            id=self.id,
            state=state,
            publisher_name=self.publisher,
            plugin_name=self.plugin,
            query_name=self.query,
            key=keys,
            output=outputs,
            concern=self.concerns,
            split=False,
        )
        return raw


# Try to drain `max_bytes` bytes from `buf`, or the full string, whichever is shortest.
# If `max` bytes is somewhere within `buf` but lands inside a multi-byte char, walk
# backwards to the end of the previous char. Returns a tuple containing the substring
# drained from `buf`, and (optionally) the un-drained substring of buf.
def drain_at_most_n_bytes(buf: str, max_bytes: int) -> Tuple[str, Optional[str]]:
    buf_bytes = [x.encode("utf-8") for x in buf]
    buf_len = sum([len(x) for x in buf_bytes])

    to_drain = min(buf_len, max_bytes)
    if buf_len <= to_drain:
        return (buf, None)
    accum = 0
    for i in range(0, len(buf_bytes)):
        accum += len(buf_bytes[i])
        if accum > to_drain:
            break
    drained_portion = buf[0:i]
    remainder = buf[i:]
    return (drained_portion, remainder)


# Is there any remaining data in any of the `repeated` string fields
def all_chunkable_data_consumed(msg: gen.Query) -> bool:
    return len(msg.key) == 0 and len(msg.output) == 0 and len(msg.concern) == 0


# Drain as much from a List[str] as possible. The first output bool indicates if forward progress was made. For
# the second bool:
#
# `True` -> a partial string was written to sink, indicating `split=true` for this message and no more data can
#   be fit into this gRPC message
# `False` - > only complete strings were written to sink, indicating `split=false` for this message
def drain_vec_string(
    source: List[str], sink: List[str], remaining: int, made_progress: bool
) -> Tuple[bool, bool]:
    while len(source) > 0:
        s_to_drain = source.pop(0)
        drained_portion, remainder = drain_at_most_n_bytes(s_to_drain, remaining)
        if remainder is None:
            made_progress = True
            remaining -= len(drained_portion.encode("utf-8"))
            sink.append(drained_portion)
        else:
            # if any amount was drained, then a split was required
            split = len(drained_portion) > 0
            if split:
                made_progress = True
                remaining -= len(drained_portion.encode("utf-8"))
                sink.append(drained_portion)
            # since the string being processed was pulled from the front via `source.remove(0)`,
            # source.insert(0,...) needs to be used to maintain ordering
            source.insert(0, remainder)
            return (made_progress, split)
    return (made_progress, False)


# Chunk a single `gen.Query` into one or more such that none have
# sum(key+output+concern) bytes greater than `max_est_size`. Useful for
# testing the chunking algorithm without having to generate messages > 4MB,
# the default max message size in gRPC.
def chunk_with_size(msg: gen.Query, max_est_size: int) -> List[gen.Query]:
    match msg.state:
        case gen.QUERY_STATE_UNSPECIFIED:
            raise UnspecifiedQueryState()
        case gen.QUERY_STATE_SUBMIT_IN_PROGRESS | gen.QUERY_STATE_SUBMIT_COMPLETE:
            in_progress_state = gen.QUERY_STATE_SUBMIT_IN_PROGRESS
            completion_state = gen.QUERY_STATE_SUBMIT_COMPLETE
        case gen.QUERY_STATE_REPLY_IN_PROGRESS | gen.QUERY_STATE_REPLY_COMPLETE:
            in_progress_state = gen.QUERY_STATE_REPLY_IN_PROGRESS
            completion_state = gen.QUERY_STATE_REPLY_COMPLETE

    null_key = len(msg.key) == 0
    null_output = len(msg.output) == 0

    out: List[gen.Query] = []
    base: gen.Query = msg

    # Track whether we did anything on each iteration to avoid infinite loop
    made_progress: bool = True
    while not all_chunkable_data_consumed(base):
        if not made_progress:
            raise UnspecifiedQueryState()
        made_progress = False

        # For this loop, we want to take at most MAX_SIZE bytes because that's all that can
        # fit in a PluginQuery
        remaining = max_est_size
        chunked_query = gen.Query(
            id=base.id,
            state=in_progress_state,
            publisher_name=base.publisher_name,
            plugin_name=base.plugin_name,
            query_name=base.query_name,
            split=False,
        )

        for source, sink in [
            (base.key, chunked_query.key),
            (base.output, chunked_query.output),
            (base.concern, chunked_query.concern),
        ]:
            made_progress, split_occurred = drain_vec_string(
                source, sink, remaining, made_progress
            )
            if split_occurred:
                chunked_query.split = True
                break
            if remaining == 0:
                break

        # @Compatibility - pre-RFD9 will expect exactly 1 field, if empty, need to increase to empty
        if enabled("rfd9_compat"):
            # if a key was empty in this query, then insert a null placeholder
            if len(chunked_query.key) == 0:
                chunked_query.key.append("")
            # if an output was empty in this query, then insert a null placeholder
            if len(chunked_query.output) == 0:
                chunked_query.output.append("")

        out.append(chunked_query)

    # ensure the last message in the chunked messages is set to the appropriate Complete state
    out_len = len(out)
    if out_len > 0:
        out[out_len - 1].state = completion_state

    # @Compatibility - pre-RFD9 expects concatenation of all `key` fields to be a valid JSON
    # string, same with `output`. This ensures if either were all blank, at least the first says
    # "null"
    if enabled("rfd9_compat") and (null_key or null_output):
        if null_key:
            out[0].key[0] = "null"
        if null_output:
            out[0].output[0] = "null"

    return out


def chunk(msg: gen.Query) -> List[gen.Query]:
    return chunk_with_size(msg, GRPC_EFFECTIVE_MAX_SIZE)


def prepare(query: Query) -> List[gen.Query]:
    return chunk(query.try_into())


# Determine whether or not the given 'QueryState' represents an intermediate state
def in_progress_state(state: gen.QueryState) -> bool:
    return state in [
        gen.QUERY_STATE_REPLY_IN_PROGRESS,
        gen.QUERY_STATE_SUBMIT_IN_PROGRESS,
    ]


# Represents the 3 fields in a `gen.Query` that hold repeated string data
class QueryVecField(Enum):
    KEY = 1
    OUTPUT = 2
    CONCERN = 3


# Determines which field of `gen.Query` is the "latest" one with data
def last_field_to_have_content(query: gen.Query) -> QueryVecField:
    if len(query.concern) > 0:
        return QueryVecField.CONCERN

    # @Compatibility
    if enabled("rfd9_compat"):
        if not ((len(query.output) == 1) and (query.output[0] in ["", "null"])):
            return QueryVecField.OUTPUT
    else:
        if len(query.output) > 0:
            return QueryVecField.OUTPUT

    return QueryVecField.KEY


class QuerySynthesizer:
    def __init__(self):
        self.raw: Optional[gen.Query] = None

    def add(self, iterable) -> Optional[Query]:
        iterator = iter(iterable)

        if self.raw is None:
            try:
                self.raw = next(iterator)
            except StopIteration:
                return None

        # @Invariant - self.raw is now not None

        initial_state: gen.QueryState = self.raw.state
        current_state: gen.QueryState = initial_state

        # holds whether the last message was split, if it was then it holds the "latest" field
        # with data that should have the first element of the next message appended to it
        last_message_split = (
            last_field_to_have_content(self.raw) if self.raw.split else None
        )

        # If response is the first of a set of chunks, handle
        if in_progress_state(current_state):
            while in_progress_state(current_state):
                try:
                    next_chunk = next(iterator)
                except StopIteration:
                    return None

                current_state = next_chunk.state

                match (initial_state, current_state):
                    case (
                        (gen.QUERY_STATE_UNSPECIFIED, _)
                        | (gen.QUERY_STATE_REPLY_COMPLETE, _)
                        | (gen.QUERY_STATE_SUBMIT_COMPLETE, _)
                    ):
                        raise UnspecifiedQueryState()
                    case (_, gen.QUERY_STATE_UNSPECIFIED):
                        raise UnspecifiedQueryState()
                    # error out if expecting a Submit messages and a Reply is received
                    case (
                        gen.QUERY_STATE_SUBMIT_IN_PROGRESS,
                        gen.QUERY_STATE_REPLY_IN_PROGRESS,
                    ) | (
                        gen.QUERY_STATE_SUBMIT_IN_PROGRESS,
                        gen.QUERY_STATE_REPLY_COMPLETE,
                    ):
                        raise ReceivedReplyWhenExpectingSubmitChunk()
                    # error out if expecting a Submit messages and a Reply is received
                    case (
                        gen.QUERY_STATE_REPLY_IN_PROGRESS,
                        gen.QUERY_STATE_SUBMIT_IN_PROGRESS,
                    ) | (
                        gen.QUERY_STATE_REPLY_IN_PROGRESS,
                        gen.QUERY_STATE_SUBMIT_COMPLETE,
                    ):
                        raise ReceivedSubmitWhenExpectingReplyChunk()
                    # otherwise we got an expected message type
                    case (_, _):
                        if current_state in [
                            gen.QUERY_STATE_REPLY_COMPLETE,
                            gen.QUERY_STATE_SUBMIT_COMPLETE,
                        ]:
                            self.raw.state = current_state

                        next_message_split = (
                            last_field_to_have_content(next_chunk)
                            if next_chunk.split
                            else None
                        )

                        # if the last message set `split = true`, then the first element in the
                        # "next" message must be appended to the last message of the "latest"
                        # field that has content (per RFD #0009)
                        match last_message_split:
                            case None:
                                pass
                            case QueryVecField.KEY:
                                self.raw.key[len(self.raw.key) - 1] += (
                                    next_chunk.key.pop(0)
                                )
                            case QueryVecField.OUTPUT:
                                self.raw.output[len(self.raw.output) - 1] += (
                                    next_chunk.output.pop(0)
                                )
                            case QueryVecField.CONCERN:
                                self.raw.concern[len(self.raw.concern) - 1] += (
                                    next_chunk.concern.pop(0)
                                )

                        self.raw.key.extend(next_chunk.key)
                        self.raw.output.extend(next_chunk.output)
                        self.raw.concern.extend(next_chunk.concern)

                        # save off whether or not the message that was just processed was split
                        last_message_split = next_message_split

            # Sanity check - after we've left this loop, there should be no leftover message
            try:
                next_chunk = next(iterator)
                raise MoreAfterQueryComplete(self.raw.id)
            except StopIteration:
                pass

        return Query.try_from(self.raw)
