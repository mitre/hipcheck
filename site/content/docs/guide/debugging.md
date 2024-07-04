---
title: Debugging
---

# Debugging

- [Using `hc ready`](#using-hc-ready)
- [Logging](#logging)
  - [Filtering Log Messages](#filtering-log-messages)
    - [Filtering by Level](#filtering-by-level)
    - [Filtering by Target](#filtering-by-target)
    - [Filtering by Content](#filtering-by-content)
  - [Controlling Log Style](#controlling-log-style)
  - [Where do Logs Write?](#where-do-logs-write)
- [Using a Debugger](#using-a-debugger)

## Using `hc ready`

The `hc ready` command prints a variety of information about how Hipcheck is
currently configured, including Hipcheck's own version, the versions of tools
Hipcheck may need to run its analyses, the configuration of paths Hipcheck will
use during execution, and the presence of API tokens Hipcheck may need.

This is a very useful starting point when debugging Hipcheck. While Hipcheck
can only automatically check basic information like whether configured paths
are present and accessible, you should also review whether the paths `hc ready`
reports are the ones you intend for Hipcheck to use.

Similarly, for any API tokens, it's good to make sure those tokens are valid
to use, and have tha appropriate permissions required to access the
repositories or packages you are trying to analyze.

See the [`hc ready`](@/docs/guide/how-to-use.md#hc-ready) documentation for more
information on its specific CLI.

## Logging

Hipcheck logging is controlled with two environment variables:

* `HC_LOG` configures what should be logged.
* `HC_LOG_STYLE` configures the format of the log output.

### Filtering Log Messages

Every log entry in Hipcheck is accompanied by a "target" and a "level":

* __Target__: The module in which the log message originates.
* __Level__: One of `error`,  `warn`, `info`, `debug`, or `trace`.

#### Filtering By Level

You may use a "level filter" to control what log messages to show:

- __Off__: Do not show log messages.
- __Error__: Show only `error`-level log messages.
- __Warn__: Show `error` and `warn`-level log messages.
- __Info__: Show `error`, `warn`, and `info`-level log messages.
- __Debug__: Show `error`, `warn`, `info`, and `debug`-level log messages.
- __Trace__: Show `error`, `warn`, `info`, `debug`, and `trace`-level log messages.

As you can see, even successive filter shows _more_ log messages, increasing
in granularity. At the Debug or higher log levels, we try to ensure log
messages are consistently single-line. At the Trace level, messages may split
across multiple lines. Debug and Trace should only be used when debugging problems
with Hipcheck.

Level filters look like this:

```sh
$ # See all "error"-level log messages.
$ HC_LOG="error" hc check -t npm express
$
$ # See all log messages.
$ HC_LOG="trace" hc check -t npm express
```

#### Filtering by Target

You can also filter by the target module which produced the error. In general,
you'll start with printing messages from _all_ targets, then observe in the log
messages which targets appear to be most relevant, and then filter to only show
messages from that target.

Note that the targets which produce messages are not only Hipcheck's own modules,
but also modules from Hipcheck's third-party dependencies.

To filter by target, prefix the log-level filter with `<target_name>=`. This
looks like:

```sh
$ # See all "error"-level log messages from the `cli` module of Hipcheck.
$ HC_LOG="hc::cli=error" hc check -t npm express
$
$ # See all log messages from the `analysis` module of Hipcheck.
$ HC_LOG="hc::analysis=trace" hc check -t npm express
```

#### Filtering by Content

Log messages may also be filtered based on their contents, by appending `/` followed by
a regular expression to the end of the `HC_LOG` to match specific messages. If the
regex matches any part of a log message, that message is printed.

For example, to only print the `Message` values printed out in the prior example, you
could run:

```sh
$ # The "/message" indicates to search for the "message" string
$ HC_LOG=hc::shell=trace,salsa=off/message hc check -t npm express
```

### Controlling Log Style

Log style is controlled with the `HC_LOG_STYLE` environment variable. The acceptable
values are `always`, `auto`, or `never`, and they control whether to try outputting color codes
with the log messages.

### Where do Logs Write?

Log messages output to `stderr`. They can be redirected using standard shell redirection
techniques.

## Using a Debugger

Hipcheck can be run under a debugger like `gdb` or `lldb`. Because Hipcheck is
written in Rust, we recommend using the Rust-patched versions of `gdb` or `lldb`
which ship with the Rust standard tooling. These versions of the tools include
specific logic to demangle Rust symbols to improve the experience of debugging
Rust code.

You can install these tools by following the standard [Rust installation
instructions](https://www.rust-lang.org/tools/install).

With one of these debuggers installed, you can then use them to set breakpoints
during Hipcheck's execution, and do all the normal program debugging processes
you're familiar with if you've used a debugger before. Explaining the use of
these tools is outside of the scope of the Hipcheck documentation, so we defer
to their respective documentation sources.
