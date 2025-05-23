Welcome to the Python SDK for writing plugins for the [Hipcheck][hipcheck-site] analysis tool.

# User / Plugin Developer Information

The guide for developing plugins with this SDK can be found [here][python-sdk-guide].

The API documentation for this package can be found [here][python-sdk-api-docs].

A dummy example plugin using the SDK can be found [here][sdk-example-plugin].

# Python SDK Developer Information

You must have `uv` installed. Installation instructions can be found
[here](https://docs.astral.sh/uv/getting-started/installation/).

## Running unit tests

From `sdk/python`, run
```bash
uv run pytest ./tests
```

## Regenerating auto-generated code

The contents of `src/hipcheck_sdk/gen` is generated Python code from multiple
sources. Some files are generated by the `grpcio` Python library using the
Hipcheck protobuf protocol spec. An additional file contains Python classes
automatically derived from the Hipcheck `Target` object schema. To regenerate
these files, you can run the following:

```bash
uv run scripts/update-gen.py
```

## Testing example plugin listening on port

From `sdk/python`, run
```bash
uv run tests/example-plugin/main.py --port <PORT>`.
```

Or from `hipcheck` repository root, run
```bash
uv run --project sdk/python sdk/python/tests/example-plugin/main.py --port <PORT>
```

## Testing example plugin in Hipcheck analysis

From the Hipcheck repository root, use a policy with the following `plugins`
entry:

```
    plugin "mitre/example" version="0.0.0" manifest="sdk/python/tests/example-plugin/local-plugin.kdl"
```

Currently, this will get through configuration and receive the query from
Hipcheck core, but the plugin will appear as an error in the final report
because it is not currently written to accept `Target` objects.

## Building the HTML

From `docs/`, run the following:

```bash
uv run make html # Build the html in the build/ directory
cp -r build/html <DESTINATION_DIR>
```

[hipcheck-site]: https://hipcheck.mitre.org/
[python-sdk-guide]: https://hipcheck.mitre.org/docs/guide/making-plugins/python-sdk/
[python-sdk-api-docs]: https://hipcheck.mitre.org/sdk/python/hipcheck_sdk.html
[sdk-example-plugin]: https://github.com/mitre/hipcheck/blob/main/sdk/python/tests/example-plugin/test_main.py
