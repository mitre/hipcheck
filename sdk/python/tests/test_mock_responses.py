# SPDX-License-Identifier: Apache-2.0

import pytest
import asyncio

from hipcheck_sdk import *
from hipcheck_sdk.error import UnknownPluginQuery


def test_mock_responses():
    expected = 2
    engine = PluginEngine.mock([(("mitre/example/nondefault", 1), expected)])
    try:
        res = asyncio.run(engine.query("mitre/example", 1))
        assert false
    except UnknownPluginQuery:
        pass
    try:
        res = asyncio.run(engine.query("mitre/example/nondefault", 2))
        assert false
    except UnknownPluginQuery:
        pass

    res = asyncio.run(engine.query("mitre/example/nondefault", 1))
    assert res == expected

    engine = PluginEngine.mock(
        [
            (("mitre/example", 1), 1),
            (("mitre/example", 2), 2),
            (("mitre/example", 3), 3),
        ]
    )

    async def run(engine):
        builder = engine.batch("mitre/example")
        builder.query(1)
        builder.query(2)
        builder.query(3)

        return await builder.send()

    res = asyncio.run(run(engine))
    assert res == [1, 2, 3]
