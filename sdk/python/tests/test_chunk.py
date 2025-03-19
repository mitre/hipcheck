# SPDX-License-Identifier: Apache-2.0

import pytest
import copy

import hipcheck_sdk
from hipcheck_sdk.chunk import *
import hipcheck_sdk.gen as gen

from context import *


def test_bounded_char_draining():
    orig_key = "aこれは実験です"
    max_size = 10
    drained_portion, remainder = drain_at_most_n_bytes(orig_key, max_size)
    drained_bytes = drained_portion.encode("utf-8")
    assert remainder is not None
    assert len(drained_bytes) in range(0, max_size + 1)

    reassembled = drained_portion + remainder
    assert reassembled == orig_key


def test_draining_vec():
    source = ["123456"]
    sink = []

    while len(source) > 0:
        made_progress = False
        _, partial = drain_vec_string(source, sink, 1, made_progress)
        assert partial == (len(source) > 0)
    assert len(sink) == 6
    assert len(source) == 0

    source = ["123456"]
    sink = []
    while len(source) > 0:
        made_progress = False
        _, partial = drain_vec_string(source, sink, 3, made_progress)
        assert partial == (len(source) > 0)
    assert len(sink) == 2
    assert len(source) == 0


def test_char_boundary_respected():
    source = ["実"]
    sink = []
    made_progress = False
    drain_vec_string(source, sink, 1, made_progress)
    assert not made_progress


def test_non_ascii_drain_vec_string_makes_progress():
    source = ["1234", "aこれ", "abcdef"]
    sink = []

    while len(source) > 0:
        remaining = 4
        made_progress = False
        made_progress, split = drain_vec_string(source, sink, remaining, made_progress)
        assert made_progress

    assert sink[0] == "1234"
    assert len(source) == 0


def test_drain_vec_string_split_detection():
    max_len = 3
    source = ["1234"]
    sink = []
    made_progress, split = drain_vec_string(source, sink, max_len, False)
    assert split
    assert source == ["4"]
    assert made_progress
    assert len(source) == 1
    assert len(sink) == 1

    max_len = 10
    source = ["123456789"]
    sink = []
    made_progress, split = drain_vec_string(source, sink, max_len, False)
    assert not split
    assert len(source) == 0
    assert made_progress
    assert len(sink) == 1


@pytest.mark.parametrize(
    "opts", [options_disable_rfd9_compat, options_enable_rfd9_compat]
)
def test_chunking_and_query_reconstruction(opts):
    states = [
        (gen.QUERY_STATE_SUBMIT_IN_PROGRESS, gen.QUERY_STATE_SUBMIT_COMPLETE),
        (gen.QUERY_STATE_REPLY_IN_PROGRESS, gen.QUERY_STATE_REPLY_COMPLETE),
    ]

    for intermediate_state, final_state in states:
        output = ["null"] if hipcheck_sdk.enabled("rfd9_compat") else []
        orig_query = gen.Query(
            id=0,
            state=final_state,
            publisher_name="",
            plugin_name="",
            query_name="",
            key=[json.dumps("aこれは実験です")],
            output=output,
            concern=["< 10", "0123456789", "< 10#2"],
            split=False,
        )
        assert len(orig_query.key) == 1
        assert len(orig_query.output) == 1

        try:
            res = chunk_with_size(copy.deepcopy(orig_query), 10)
        except hipcheck_sdk.error.SdkError as e:
            assert False

        for i in range(0, len(res) - 1):
            assert res[i].state == intermediate_state

        assert res[len(res) - 1].state == final_state

        synth = QuerySynthesizer()
        synthesized_query = synth.add(res)
        assert synthesized_query is not None

        synthesized_plugin_query: gen.Query = synthesized_query.try_into()
        assert orig_query == synthesized_plugin_query
