# SPDX-License-Identifier: Apache-2.0

from pytest import fixture

from hipcheck_sdk import *


@fixture
def options_disable_rfd9_compat():
    opts_before = get_options()
    set_option("rfd9_compat", False)
    yield
    set_options(opts_before)


@fixture
def options_enable_rfd9_compat():
    opts_before = get_options()
    set_option("rfd9_compat", True)
    yield
    set_options(opts_before)
