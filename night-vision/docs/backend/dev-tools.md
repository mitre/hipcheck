
# Dev Tools

## `nvdb`

`nvdb` is the Night Vision backend debugger for local database inspection,
migration, and entity-generation workflows. See the
[`nvdb` command reference](./nvdb-command-reference.md) for current commands,
flags, examples, expected output, and secret-safe usage guidance.

## `xtask unit-graph`

The backend `xtask` crate includes a `unit-graph` command that turns Cargo's
unit graph output into a build visualization.

Cargo exposes the unit graph through the unstable `--unit-graph` flag, which
also requires `-Z unstable-options`. Night Vision uses a pinned stable Rust
toolchain, so don't use `cargo +nightly` for this command. Instead, enable the
unstable Cargo flags for the single invocation with `RUSTC_BOOTSTRAP=1`.

`RUSTC_BOOTSTRAP` is a permanently-unstable escape hatch that bypasses Rust's
normal stability guarantees. Use it carefully, and keep its use limited to
narrow tooling commands like this one rather than regular development,
testing, or production build flows.

From `backend/`, run:

```sh
RUSTC_BOOTSTRAP=1 cargo -Z unstable-options <BUILD_CMD> --unit-graph | cargo xtask unit-graph -
```

Replace `<BUILD_CMD>` with the Cargo build command you want to inspect, such as
`check`, `test --no-run`, or `build`.
