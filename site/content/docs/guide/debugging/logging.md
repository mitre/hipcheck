---
title: Logging
weight: 2
---

# Logging

Hipcheck logging is controlled with two environment variables:

* `HC_LOG` configures what should be logged.
* `HC_LOG_STYLE` configures the format of the log output.

## Filtering Log Messages

Every log entry in Hipcheck is accompanied by a "target" and a "level":

* __Target__: The module in which the log message originates.
* __Level__: One of `error`,  `warn`, `info`, `debug`, or `trace`.

### Filtering By Level

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

### Filtering by Target

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

### Filtering by Content

Log messages may also be filtered based on their contents, by appending `/` followed by
a regular expression to the end of the `HC_LOG` to match specific messages. If the
regex matches any part of a log message, that message is printed.

For example, to only print the `Message` values printed out in the prior example, you
could run:

```sh
$ # The "/message" indicates to search for the "message" string
$ HC_LOG=hc::shell=trace,salsa=off/message hc check -t npm express
```

## Controlling Log Style

Log style is controlled with the `HC_LOG_STYLE` environment variable. The acceptable
values are `always`, `auto`, or `never`, and they control whether to try outputting color codes
with the log messages.

## Where do Logs Write?

Log messages output to `stderr`. They can be redirected using standard shell redirection
techniques.
