---
title: Architecture
weight: 2
---

# Architecture and Plugin Startup

This document describes the distributed architecture of Hipcheck and how plugins
get started.

Hipcheck is a relatively simple multiprocessed tool that follows a star topology
(there is a single central node to which all other nodes connect). Users invoke
the main Hipcheck binary, often referred to as "Hipcheck core" or `hc`, on the
command line, and provide a [policy file][policy_file] which specifies the set
of top-level plugins to use during analysis. Once Hipcheck resolves these
plugins and their dependencies, it starts each plugin in a separate child
process. Once all plugins are started and initialized, Hipcheck
core enters the analysis phase. During this phase it acts as a simple hub for
querying top-level plugins and relaying queries between plugins, as plugins are
intended to only communicate with each other through the core.

This design enables `hc` to perform query result caching using a function
memoization crate called `salsa` that records the input and output of `hc`'s
central `query()` function. If a query against a particular plugin endpoint with
the same key is made again, `salsa` will return the cached result and thus avert
recomputation of a known output. By requiring all inter-plugin queries to go
through Hipcheck core, we can ensure that all plugins may benefit from any
information that has already been computed. As an example, many plugins want
in-memory commit objects for the Git repository being analyzed, but it is
expensive to generate these objects. `salsa` ensures that `hc` only requests the
`git` plugin to generate these commit objects on a given repository once.

## Plugin Startup

Hipcheck core uses the `plugins/manager.rs::PluginExecutor` struct to start
plugins. The `PluginExecutor` has fields like `max_spawn_attempts` and
`backoff_interval` for controlling the startup process. These fields can be
configured using the `Exec.kdl` file.

The main function in `PluginExecutor` is `start_plugin()`, which takes a
description of a plugin on file and returns a `Result` containing a handle to
the plugin process, called `PluginContext`.

In `start_plugin()`, once the `PluginExecutor` has done the work of locating the
plugin entrypoint binary on disk, it moves into a loop of attempting to start
the plugin, at most `max_spawn_attempts` times. For each spawn attempt, it will
call `PluginExecutor::get_available_port()` to get a valid local port to tell
the plugin to listen on. The executor creates a `std::process::Command` object
for the child process, with `stdout/stderr` forwarded to Hipcheck core.

Within each spawn attempt, `PluginExecutor` will try to connect to the port on
which the plugin should be listening. Since process startup and port
initialization can take differing amounts of time, the executor does a series of
up to `max_conn_attempts` connection attempts. For each failed connection, the
executor waits `backoff_interval`, which increases linearly with the number of
failed connections. The calculated backoff is modulated by a random `jitter`
between 0 and `jitter_percent`.

Overall, the sleep duration between failed connections is equal to

	(backoff * conn_attempts) * (1.0 +/- jitter)

As soon as `PluginExecutor::start_plugin()` successfully starts and connects to the child
process, it stores the process and plugin information in a `PluginContext` and
returns it to the caller. It however returns an error if `max_spawn_attempts` is reached.

[policy_file]: @/docs/guide/config/policy-file.md
