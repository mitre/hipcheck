# SPDX-License-Identifier: Apache-2.0

import asyncio
import argparse
import os
import logging
import pytest

from typing import Optional

from hipcheck_sdk.error import *
from hipcheck_sdk import (
    PluginEngine,
    Plugin,
    query,
    run_server_for,
    Target,
    LocalGitRepo,
    MockResponses,
)

DETECTOR = None


@query
async def dummy_rand_data(engine: PluginEngine, key: int) -> int:
    reduced_num = key % 7

    engine.record_concern("This is a test")

    value = await engine.query("dummy/sha256/sha256", [reduced_num])

    engine.record_concern("This is a test2")

    return value[0]


@query(default=True)
async def take_target(engine: PluginEngine, key: Target) -> int:
    return len(key.local.path)


class ExamplePlugin(Plugin):
    name = "example"
    publisher = "mitre"

    def set_config(self, config: dict):
        global DETECTOR

        if getattr(self, "binary-file-threshold", None) or DETECTOR is not None:
            raise UnspecifiedConfigError("plugin was already configured")

        opt_threshold = config.get("binary-file-threshold", None)
        if opt_threshold is not None:
            if type(opt_threshold) != int:
                raise InvalidConfigValue(
                    "binary-file-threshold",
                    opt_threshold,
                    "must be an unsigned integer",
                )
        self.opt_threshold = opt_threshold

        binary_file = config.get("binary-file", None)
        if binary_file is None:
            raise MissingRequiredConfig("binary-file", "string", [])
        if not type(binary_file) is str:
            raise InvalidConfigValue(
                "binary-file", binary_file, "must be a string path"
            )
        if not os.path.exists(binary_file):
            raise InvalidConfigValue("binary-file", binary_file, "path does not exist")
        try:
            with open(binary_file, "r") as f:
                data = f.read()
        except Exception as e:
            raise InvalidConfigValue("binary-file", binary_file, f"{e}")

        DETECTOR = data

    def default_policy_expr(self) -> Optional[str]:
        if self.opt_threshold is None:
            return None
        else:
            return f"(lte $ {self.opt_threshold})"


if __name__ == "__main__":
    run_server_for(ExamplePlugin())


@pytest.mark.asyncio
async def test_endpoint():
    t = Target(
        local=LocalGitRepo(git_ref="", path="this/is/a/file/path"),
        specifier="this/is/a/file/path",
    )

    t2 = Target(
        local=LocalGitRepo(git_ref="", path="this/is/a/file/path"),
        specifier="this/is/a/file/path",
    )

    # Ensure that our matching logic in `mock_engine` handling will work
    assert t == t2

    mock_responses: MockResponses = [
        (("dummy/sha256/sha256", [1]), [0xBE]),
        (("dummy/sha256/sha256", t), None),
    ]
    engine = PluginEngine.mock(mock_responses)

    res = await dummy_rand_data(engine, 8)
    assert res == 0xBE

    # Demonstrate that an empty `mock()` call will still use an empty list
    # If not we would get a networking related error
    engine = PluginEngine.mock()
    try:
        res = await dummy_rand_data(engine, 8)
        assert False
    except UnknownPluginQuery:
        pass

    res = await take_target(engine, t)
    assert res == 19
