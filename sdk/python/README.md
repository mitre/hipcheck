**NOTE**: This package is not ready for public use. We have published this
package on PyPI to reserve the package name.

You must have `uv` installed. Installation instructions can be found
[here](https://docs.astral.sh/uv/getting-started/installation/).

# Running unit tests

From `sdk/python`, run
```bash
uv run pytest ./tests
```

# Testing example plugin listening on port

From `sdk/python`, run
```bash
uv run tests/example-plugin/main.py --port <PORT>`.
```

Or from `hipcheck` repository root, run
```bash
uv run --project sdk/python sdk/python/tests/example-plugin/main.py --port <PORT>
```

# Testing example plugin in Hipcheck analysis

From the Hipcheck repository root, use a policy with the following `plugins`
entry:

```
    plugin "mitre/example" version="0.0.0" manifest="sdk/python/tests/example-plugin/local-plugin.kdl"
```
