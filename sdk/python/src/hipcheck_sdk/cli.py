# SPDX-License-Identifier: Apache-2.0

import argparse

from hipcheck_sdk.server import Plugin, PluginServer


def get_parser_for(plugin_name: str) -> argparse.ArgumentParser:
    """
    Get the default argument parser for a Hipcheck plugin

    :return: An ArgumentParser configured to capture Hipcheck plugin CLI
        arguments
    """
    parser = argparse.ArgumentParser(prog=plugin_name)
    parser.add_argument("-p", "--port", type=int)
    parser.add_argument("-l", "--log-level", type=str, default="error")
    return parser


def run_server_for(plugin: Plugin):
    """
    Parse CLI arguments and start the server for the plugin. Does not return
    until the gRPC connection closes.

    :param Plugin plugin: An instance of a subclass of `Plugin`
    """
    plugin_name = type(plugin).__name__
    parser = get_parser_for(plugin_name)
    args = parser.parse_args()
    PluginServer.register(plugin, args.log_level).listen(args.port)
