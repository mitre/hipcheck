---
title: Plugin Architecture Vision
weight: 3
slug: 0003
extra:
  rfd: 3
  primary_author: Andrew Lilley Brinker
  primary_author_link: https://github.com/alilleybrinker
  status: Accepted
  pr: 71
---

# Plugin Architecture Vision

{% info(title="Partially Superseded") %}
Parts of this RFD have been superseded by RFD #4, which provides a more
detailed description of the design of the plugin system and the API
between Hipcheck and plugins.
{% end %}

Currently, all analyses and data sources in Hipcheck are part of Hipcheck
itself, built into the `hc` binary. This places some constraints on the
evolution and growth of Hipcheck's capabilities, including:

- __Upstreaming__: Outside contributors who want to add capabilities to
  Hipcheck have to either make upstream contributions to the project or
  maintain a fork which includes their changes and must remain synced with
  upstream.
- __Intellectual Property Constraint__: Data sources and analyses built
  into Hipcheck must be open source, because Hipcheck itself is open
  source.
- __Language Choice__: Data sources and analyses must be written in Rust,
  because Hipcheck is written in Rust.
- __Coordination Cost__: Improvements to data sources and analyses must
  be coordinated with the Hipcheck project, incurring coordination cost
  and raising the barrier to contribution.

To address these issues, we'd like to expose a plugin mechanism for
Hipcheck, so third-parties can add new data sources and new analyses
without needing to contribute them to Hipcheck directly. These plugins
can be licensed separately and even remain closed source, they ought to
be able to be written in languages besides Rust, and for coordination
the only requirement will be that they comply with the API defined for
plugins to interoperate with Hipcheck.

## Plugin Contents

The key file for any Hipcheck plugin will be a manifest, which specifies
the information necessary for Hipcheck to load and run the plugin. This
manifest ought to include the following information:

- __Publisher__: a string identifying the individual or organization that
  created the plugin.
- __Namespace__: a string identifying a namespace owned by the publisher,
  so publishers can organize their plugins as they see fit.
- __Name__: a string specifying the name of the plugin.
- __Version__: a string containing a Semantic Versioning version number.
- __Dependent Plugins__: The plugins whose data this plugin relies on.
  - Publisher
  - Namespace
  - Name
  - Version
- __Kind__: "analysis" or "data"
- __Interface__: a string indicating what kind of interface ought to be
  used to interact with this plugin (see the "Interface" section below).
- __Plugin Interface Version__: a string indicating the version of the
  Hipcheck plugin interface this plugin is compatible with.
- __Schema Files__: a map between strings identifying plugin function
  calls and JSON schema files specifying the schemas of the data
  returned by those calls.
- __Entrypoint__: a string with the file path of the file to run to
  execute the plugin.
- __Entrypoint Hashes__: a map of hash types to hashes which can be used
  to validate that the entrypoint file has not been modified from its
  original source during download.

The specific format of this manifest file (JSON, YAML, KDL, etc.) has
not yet been decided and is not specified here.

## Interface

One of the open questions for the design of the plugin system is what
kind of interface ought to be used for Hipcheck to interact with the
plugins. Currently, we see three possibilities:

- __WebAssembly (WASM)__: Plugins would be compiled WebAssembly files,
  run by a WebAssembly runtime embedded within Hipcheck.
- __Inter-Process Communication (IPC)__: Plugins would be run as
  independent processes, and Hipcheck would use platform-specific IPC
  mechanisms to communicate with those processes.
- __C Foreign Function Interface (FFI)__: Plugins would be compiled into
  ABI-compatible dynamically-loadable libraries, which would be loaded by
  Hipcheck and then called within the Hipcheck process.

Each of these options represents likely trade-offs in security, stability
of the communication interface, cross-platform support, performance, and
more.

One of the follow-on tasks will be to experiment with these options to
better understand their tradeoffs and limitations. Hipcheck may end up
supporting more than one option, depending on the outcome of that
investigation.

## Security

Since Hipcheck, with plugins, would be running potentially untrusted code,
some precautions ought to be in place to protect users against potentially
insecure or malicious plugins.

These mechanisms ought to include:

- Checking against hashes provided by users in their configuration file
  which specifies what plugins to download.
- Within plugins, checking against hashes for the entrypoint files before
  executing them.
- To the greatest extent possible, sandboxing plugins to limit their
  ability to access the data of other plugins outside of normal Hipcheck-
  provided API calls, including memory sandboxing and file system
  sandboxing.

While these will be mitigations, Hipcheck can't feasibly ensure total
security against malicious plugins, and will still recommend to users that
they assess plugins before trusting them.

## Hipcheck / Plugin Interaction Flow

The Hipcheck / plugin interaction will be initiated by the user providing
a configuration file which specifies the plugins it requires to run, and
the analyses to run from those plugins.

Plugins may themselves be configurable, and part of the flow will include
asking plugins themselves to validate that the configuration they've been
provided is usable.

The full flow would run as follows:

- __Configuration Schema Check__: Hipcheck first checks if the configuration
  file itself meets the schema, and reports errors if not.
- __Plugin Download__: Hipcheck downloads any requested plugins which are not
  already downloaded.
- __Plugin Download Validation__: Hipcheck checks the plugins against their
  user-provided hashes, and produces errors and halts if the hashes do not
  match.
- __Analysis Availability Check__: Hipcheck checks if the analyses requested
  can all be found (can find the plugin by that producer, with that analysis
  and that version number), and reports errors if not.
- __Plugin Configuration Check__: Hipcheck checks if any plugin configuration
  is correct by passing it to the plugin for validation and reports errors if
  not.
- __Data Source Plugin Check__: Hipcheck finds out the data needed from data
  plugins for all the requested analyses, as reported by them based on their
  configurations, and reports errors if those data sources can't be found.
- __Analysis Execution__: Hipcheck then runs the analyses requested by the
  user, coordinating their execution along with data collection to produce
  answers as quickly as possible and with minimal data duplication. As it runs
  each plugin, it will also collect "hints" from plugins to indicate additional
  information users may want to investigate.
- __Score Reporting__: Finally, when the final score is reported, it's reported
  alongside a hash produced from the configuration to limit the degree to which
  people try to compare scores made from different configurations. This hash is
  done with the help of plugins, by hashing their fully-initialized
  configuration value, including any default values.

## User Experience

We do not want the presence of plugins to degrade the user experience. We still
want to provide rich guidance to users on how to take action based on the
results of their analyses, and supporting this high quality user experience
will require interoperation with plugins.

A key mechanism will be the ability for plugins to produce "hints" alongside
their analysis results, which Hipcheck can provide to users to inform any
further investigation they wish to perform.

Plugins' configuration will also be used to contextualize scores, as scores in
Hipcheck are strongly dependent on the configuration of the analyses being run,
and should not be compared to each other without that context. Scores are an
expression of _consistency with policy_, and not of an objective universal
reality.
