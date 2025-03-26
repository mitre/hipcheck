# SPDX-License-Identifier: Apache-2.0

# Runs protoc to regenerate the content of the src/hipcheck_sdk/gen folder.
# This script should be run after any changes are made to the Hipcheck
# protobuf definition in `hipcheck-common/proto`.
#
# Usage: uv run scripts/update-gen.py

import os
import subprocess
from pathlib import Path

from grpc_tools import protoc

# if run with uv, the venv is always sdk/python/.venv, so we want to go up a level
venv_path = Path(os.environ.get("VIRTUAL_ENV"))
sdk_path = venv_path.parent

rel_proto_def_dir = Path("../../library/hipcheck-common/proto/hipcheck/v1")

# The include dir for protoc generation
sdk_rel_proto_def_dir = sdk_path / rel_proto_def_dir

# The .proto source
sdk_rel_proto_def_file = sdk_rel_proto_def_dir / "hipcheck.proto"

# Where the protoc-generated files go
sdk_gen_dir = sdk_path / "src/hipcheck_sdk/gen"

# Run the protoc command to regenerate the gRPC files
protoc_args = [
    "protoc",
    f"-I={sdk_rel_proto_def_dir}",
    f"--python_out={sdk_gen_dir}",
    f"--pyi_out={sdk_gen_dir}",
    f"--grpc_python_out={sdk_gen_dir}",
    f"{sdk_rel_proto_def_file}",
]
protoc.main(protoc_args)

sdk_parent = sdk_path.parent
schema_src_path = sdk_parent / "schema" / "hipcheck_target_schema.json"
schema_gen_path = sdk_gen_dir / "types.py"

datamodel_args = [
    "datamodel-codegen",
    "--input",
    f"{schema_src_path}",
    "--input-file-type",
    "jsonschema",
    "--output-model-type",
    "pydantic_v2.BaseModel",
    "--output",
    f"{schema_gen_path}",
    "--disable-timestamp",
]
schema_gen_res = subprocess.run(datamodel_args, capture_output=True)
if schema_gen_res.returncode != 0:
    print("types.py generation failed: ", schema_gen_res.stderr)

# Prepend the SPDX License header to the relevant .py files
py_files = ["hipcheck_pb2.py", "hipcheck_pb2_grpc.py", "types.py"]
line = "# SPDX-License-Identifier: Apache-2.0"

for py_file in py_files:
    file_path = sdk_gen_dir / py_file
    with open(file_path, "r+") as f:
        content = f.read()
        f.seek(0, 0)
        f.write(line.rstrip("\r\n") + "\n" + content)
