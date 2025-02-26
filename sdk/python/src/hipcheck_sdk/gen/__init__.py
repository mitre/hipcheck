# SPDX-License-Identifier: Apache-2.0

import os
import sys
dir_path = os.path.dirname(os.path.realpath(__file__))
sys.path.append(dir_path)

from hipcheck_pb2 import *
from hipcheck_pb2_grpc import *
