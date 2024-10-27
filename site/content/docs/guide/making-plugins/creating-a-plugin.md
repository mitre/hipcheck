---
title: Creating a Plugin
weight: 1
---

# Creating a Plugin

A Hipcheck plugin is a separate executable artifact that Hipcheck downloads,
starts, and communicates with over a gRPC protocol to request data. A plugin's
executable artifact is the binary, set of executable program files, Docker
container, or other artifact which can be run as a command line interface
program through a singular "start command" defined in the plugin's
manifest file.

The benefit of the executable-and-gRPC plugin design is that plugins can be
written in any of the many languages that have a gRPC library. One drawback is
that plugin authors have to at least be aware of the target platform(s) they
compile their plugin for, and more likely will need to support a handful of
target platforms. This can be simplified through the optional use of container
files as the plugin executable artifact.

Once a plugin author writes their plugin, compiles, packages, and
distribute it, Hipcheck users can specify the plugin in their policy file for
Hipcheck to fetch and use in analysis.

## Plugin CLI

Hipcheck requires that plugins provide a CLI which accepts a `--port <PORT>`
argument, enabling Hipcheck to centrally manage the ports plugins are listening
on. The port provided via this CLI argument must be the port the running plugin
process listens on for gRPC requests, and on which it returns responses.

Once started, the plugin should continue running, listening for gRPC requests
from Hipcheck, until shut down by the Hipcheck process.
