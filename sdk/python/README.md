
You must have `uv` installed.

# Testing example plugin listening on port

From `sdk/python`, run `uv run tests/example-plugin/main.py --port <PORT>`

# Testing example plugin in Hipcheck analysis

From the Hipcheck repository root, use a policy with the following `plugins`
entry:

```
    plugin "mitre/example" version="0.0.0" manifest="sdk/python/tests/example-plugin/local-plugin.kdl"
```
