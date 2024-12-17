---
title: Repo Structure
weight: 1
---

# The Hipcheck Repository

This document describes the overall layout of the Hipcheck repository, in an
effort to help new developers become acquainted with where different
functionality resides.

## The Repository Root

The repository is a cargo workspace, containing multiple crates and
organizational directories.

Some of these directories include:
- `hipcheck/` - The main `hc` binary crate.
- `sdk/` - Contains the plugin [software development kits (SDKs)][plugin_sdk] maintained by the Hipcheck
	team for various languages, which each language's SDK in a separate subdirectory.
	- `rust/` - The Hipcheck [Rust SDK][rust_sdk] crate.
- `hipcheck-common/` - An internal library crate containing functionality shared between the `hc` binary crate and the Rust SDK.
	- `proto/` - The Protobuf protocol definition for communication between the `hc` binary and plugins.
- `hipcheck-macros/` - An internal library crate of proc macros for the `hc` binary.
- `hipcheck-sdk-macros/` - An internal library crate of proc macros for the Rust SDK.
- `plugins/` - Contains each plugin supported directly by the Hipcheck team as a
	separate crate.
- `site/` - Source for the Hipcheck website.
- `dist/` - Items related to distributing Hipcheck as a container.
- `xtask/` - A crate containing custom commands that can be invoked via `cargo xtask <CMD>` within the Hipcheck workspace.
	`src/task/` - Contains each module corresponding to a single `xtask` subcommand.

## The Hipcheck binary crate

Important modules within the `hipcheck/` binary crate include:
- `cache/` - Implements the `hc cache` subcommand for manipulating the
	repository and plugin caches.
- `cli.rs` - Defines the Hipcheck command line interface.
- `config.rs` - Functionality for calculating the Hipcheck score tree from a
	policy file.
- `engine.rs` - Entrypoint for interacting with Hipcheck plugins.
- `init/` - Code to be run as part of Hipcheck's startup.
- `main.rs` - Entrypoint for executing any of the subcommands defined by
	`cli.rs`.
- `plugin/` - All code related to retreiving, managing, and starting plugins.
- `policy/` - Defines policy files and their parsing.
- `policy_expr/` - Defines the [policy expression language][policy_expr] parsing and execution.
- `report/` - Functionality for building a report from the results of an
	analysis.
- `score.rs` - Combining score tree and analysis results to produce an overall
	risk score for the analysis.
- `session/` - Managing a given Hipcheck `check` execution from start to finish,
	including plugin retrieval and execution, policy file parsing, analysis,
	scoring, and report building.
- `setup.rs` - Implements the `hc setup` subcommand that does one-time actions
	as part of a Hipcheck installation.
- `shell/` - Managing the terminal output of the Hipcheck `hc` process.
- `source/` - Code for manipulating Git repositories.
- `target/` - Defines the various types of Hipcheck analysis targets (e.g.
	SBOMs, packages, GitHub repos, local repos, etc.), how they are identified
	from a user-supplied string, and how they resolve to a particular repo and
	commit for analysis.

### The `policy_expr` Module

- `token.rs` - Definition of the tokens that make up the policy expression
	language using the `logos` crate
- `bridge.rs` - Code for making `logos` interoperable with `nom` parser crate.
- `expr.rs` - Definitions of language objects (functions, primitives, etc.) and
	the `nom` parsers that transform token streams into them.
- `error.rs` - Definitions of errors related to parsing and executing policy
	expressions.
- `json_pointer.rs` - Code for injecting JSON data into policy expressions.
- `env.rs` - Definition and standard impl of the `Env` struct, which defines the
	implementation of functions used in the policy expression language.
- `pass.rs` - Visitor or mutating operations on an entire `expr.rs::Expr` tree,
	such as resolving functions and type checking/fixing.
- `mod.rs` - Definition of expression execution and standard pre-execution pass
	groupings.

[plugin_sdk]: @/docs/rfds/0006-rust-plugin-sdk.md
[rust_sdk]: @/docs/guide/making-plugins/rust-sdk.md
[policy_expr]: @/docs/guide/config/policy-expr.md
