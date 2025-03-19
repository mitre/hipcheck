# SPDX-License-Identifier: Apache-2.0

import asyncio
import argparse
import os

from typing import Optional

from hipcheck_sdk.error import *
from hipcheck_sdk.engine import PluginEngine
from hipcheck_sdk.server import Plugin, PluginServer, query, Union

DETECTOR = None


@query(default=True)
async def my_query(engine: PluginEngine, key: int) -> int:
    reduced_num = key % 7

    engine.record_concern("This is a test")

    value = await engine.query("dummy/sha256/sha256", [reduced_num])

    engine.record_concern("This is a test2")

    return value[0]


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
    parser = argparse.ArgumentParser(prog="ExamplePlugin")
    parser.add_argument("-p", "--port", type=int)
    args = parser.parse_args()
    PluginServer.register(ExamplePlugin()).listen(args.port)
