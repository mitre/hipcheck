---
title: Testing Changes
weight: 2
---

# Testing Changes

All changes to Hipcheck must pass continuous integration (CI) tests prior
to being merged. You can simulate this test suite, at least on your own
operating system and architecture, using the following command:

```sh
$ cargo xtask ci
```

Passing this command is not a _guarantee_ of passing the official CI suite
on GitHub, but is a good way to approximate things locally.

If you want faster tests locally, we also recommend installing `cargo-nextest`.
The `cargo xtask ci` command will use it instead of `cargo test` if it's
installed.
