# SPDX-License-Identifier: Apache-2.0

import copy
from dataclasses import dataclass


@dataclass
class SdkOptions:
    """
    Options that can be set before starting a server.PluginServer to
    control behavior. Designed to offer roughly similar behavior to the
    feature flags available in the Rust Hipcheck SDK.
    """

    rfd9_compat: bool = True


_options = SdkOptions()


def set_option(option_name: str, val: bool):
    """
    Turn an option on or off.

    :param str option_name: The option name to set
    :param bool val: The value to which to set the option
    """
    global _options
    setattr(_options, option_name, val)


def enabled(option_name: str) -> bool:
    """
    :param str option_name: The option name to check
    :return: True if the option is enabled, else False
    """
    global _options
    return getattr(_options, option_name)


def set_options(options: SdkOptions):
    """
    Set the entire global `SdkOptions` instance at once instead of toggling
    individual options.

    :param SdkOptions options: The options instance to override the current
        global value with
    """
    global _options
    _options = options


def get_options() -> SdkOptions:
    """
    :return: A copy of the global `SdkOptions` instance. Note that this is a
        deep copy so changes to the returned value will not affect the global
        state.
    """
    global _options
    return copy.copy(_options)
