
# Dev Tools

## `nvdb`

`nvdb` is the Night Vision backend debugger for local database inspection,
migration, and entity-generation workflows. See the
[`nvdb` command reference](./nvdb-command-reference.md) for current commands,
flags, examples, expected output, and secret-safe usage guidance.

For normal local startup, health checks, logs, PostgreSQL checks, and recovery
steps, see the [backend operations runbook](./operations-runbook.md).

For the end-to-end database development flow, including adding migrations,
applying them locally, regenerating SeaORM entities, test database setup,
rollback expectations, and schema review, see the
[database workflow guide](../../backend/migration/README.md).

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

## `cargo mutants`

[`cargo mutants`](https://mutants.rs/) checks whether the backend test suite
catches small behavior changes. It is a deeper, slower quality check, so it is
not part of `cargo xtask ci` or normal Merge Request pipelines.

The shared configuration in `backend/.cargo/mutants.toml` limits default runs
to `nv-common` and uses its test suite. To assess the `package.json` parser,
run this from the repository root:

```sh
flox activate --dir . --command \
  'cd backend && mkdir -p target/cargo-mutants && \
  cargo mutants --package nv-common --file nv-common/src/npm/package_json.rs \
  --output target/cargo-mutants/package-json'
```

Review surviving mutants before adding tests: a survivor may reveal a missing
assertion, but it can also represent behavior that is intentionally not covered
by the current contract. Local runs that use the default output directory write
to `backend/mutants.out/`, which is ignored by version control.



