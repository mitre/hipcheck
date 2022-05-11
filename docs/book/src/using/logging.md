
# Logging

Hipcheck logging is controlled with two environment variables:

* `HC_LOG` configures what should be logged.
* `HC_LOG_STYLE` configures the format of the log output.

## Controlling what to log

### Setting target and level filter

Logging in Hipcheck is implemented using the `env_logger` Rust crate, which
has a defined format for setting what should be logged. In short, each log message
is accompanied by a "target" and a "level filter."

* __Target__: The module in which the log message originates.
* __Level Filter__: One of `off`, `error`,  `warn`, `info`, `debug`, or `trace`.
  each level displays the messages of any level "above" it in the hierarchy. So `off` shows
  nothing, while `trace` shows everything.

These are combined with the `HC_LOG` environment variable to specify exactly
what modules to print errors from, and what level of messages should be printed.

For example, to print all messages of any severity from the `hc_shell` crate, ignoring
any log messages from `salsa` (a core Hipcheck dependency which often appears in the
logs), you run:

```shell
$ # Replace <repo_url> with the URL of the repo to analyze.
$ HC_LOG=hc_shell=trace,salsa=off hipcheck <repo_url>
```

### Filtering messages based on content

Log messages may also be filtered based on their contents, by appending `/` followed by
a regular expression to the end of the `HC_LOG` to match specific messages. If the
regex matches any part of a log message, that message is printed.

For example, to only print the `Message` values printed out in the prior example, you
could run:

```shell
$ # The "/message" indicates to search for the "message" string
$ HC_LOG=hc_shell=trace,salsa=off/message hipcheck <repo_url>
```

## Controlling log style

Log style is controlled with the `HC_LOG_STYLE` environment variable. The acceptable
values are `always`, `auto`, or `never`, and they control whether to try outputting color codes
with the log messages.

## Where do logs write?

Log messages output to `stderr`. They can be redirected to a file or to `stdout` using
standard file redirection techniques.
