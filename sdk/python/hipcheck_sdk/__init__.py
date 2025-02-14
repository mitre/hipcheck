
# Plugin class that users can subclass to describe their plugin
# Query class

# PluginServer that subclasses PluginService and takes a Plugin object
# to interface with PluginService functions more easily

import os
import sys
dir_path = os.path.dirname(os.path.realpath(__file__))
sys.path.append(dir_path)

from gen import *
