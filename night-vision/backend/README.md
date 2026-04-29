
# Night Vision Backend

This is the backend server for Night Vision. It is written in Rust, and includes
a REST API to be used by the frontend.

## Getting Started

After [installing Rust via `rustup`](https://rustup.rs/), run:

```sh
$ cargo xtask deps install
```

This will install dependencies used by the project for development.

## Backend Crates

- `nv-server`: The actual Night Vision backend server.
- `nv-server-api`: Library crate that defines the `NvServerApi` trait.
- `nvdb`: Night Vision Debugger, a tool for debugging the backend server.
- `xtask`: Task runner, used for project-internal tasks.

## Rust Guidelines

The following are guidelines to follow when contributing Rust code to this
backend. These are intended to make it easier to maintain the codebase over
time, including by keeping compile times reasonable.

- __Avoid Procedural Macros if Possible__: Procedural macros are a powerful
  feature when writing Rust, but they interfere with the parallelization of
  Rust builds and can therefore _severely_ negatively impact compilation
  performance. Minimize their usage here. For example, when using `clap` for
  CLI argument parsing, use the builder API, not the derive-based API.
- __Minimize Build Scripts__: especially for intermediate dependencies, if you
  need to introduce a build script, ensure it minimally impacts compilation by
  using the `rerun-if-changed` and `rerun-if-env-changed` directives to avoid
  recompilation when possible. Keep build scripts small and fast to compile.
- __Minimize Monomorphization__: Rust's compile-time generics are powerful, but
  code generation can be slow. Don't lean on compile time guarantees for parts
  of the codebase that don't need it, and pay attention to how new generic code
  impacts compilation performance.
