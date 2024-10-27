---
title: Plugin API
weight: 4
slug: 0004
extra:
  rfd: 4
  primary_author: Andrew Lilley Brinker
  primary_author_link: https://github.com/alilleybrinker
  status: Accepted
  pr: 149
---

# Plugin API

While [RFD #3][rfd_3] addressed the overall vision for how the planned plugin
architecture for Hipcheck will work, the intent of this RFD is to describe in
greater detail the specifics of the interface between the `hc` binary and
Hipcheck plugins.

The goals in writing this RFD are:

- To flesh out details of the design of Hipcheck's plugin system amongst
  the Hipcheck dev team itself, by providing a point of coordination and
  discussion.
- To provide guidance to prospective plugin authors about what will be
  required to write a Hipcheck plugin, and simultaneously to empower those
  authors with enough knowledge to give feedback to the Hipcheck team about
  what they think won't work or will be challenging in the planned
  architecture.
- To inform the Hipcheck development team in some detail about what they need
  to build into Hipcheck to realize the goal of supporting third-party plugins.

Thus, moving forward this RFD has two audiences: __Hipcheck's Developers__,
and __Potential Plugin Authors__. The remainder of the RFD will be written
with _both_ of these audiences in mind.

The remainder of this RFD is written with the assumption that you have already
read RFD #3.

## Defining a Plugin

### Plugin Materials

One of the goals of Hipcheck's plugin system is that anyone can create a
Hipcheck plugin in _any_ programming language, not just Rust. This creates
challenges around distributing and executing plugins which can run on any
of Hipcheck's supported architectures. While some programming languages
are intended to be platform-agnostic (where the code as-distributed can
be run on any platform that has a usable runtime for the language), others
must be specifically compiled for the target architecture. We intend for
Hipcheck plugins to be usable in either case.

Defining a Hipcheck plugin means defining three components:

- The plugin's executable artifact
- The plugin manifest
- The plugin discovery manifest

Let's cover each of these in turn.

The plugin's __executable artifact__ is the binary, set of executable
program files, Docker container, or other artifact which can be run as a
command line interface program through a singular "start command" defined
in the plugin's manifest.

The __plugin manifest__ describes the start command for the plugin, along
with additional metadata necessary for Hipcheck to run the plugin. The
set of metadata intended for plugins was initially defined in RFD #3, but
is updated and superseded here. The manifests will be defined using [KDL], and
should be named `plugin.kdl`.

- __Publisher__ (`String`): The name of the individual or organization that
  created the plugin.
- __Name__ (`String`): The name of the plugin.
- __Version__ (`String`): The version number of the plugin, following
  [Semantic Versioning][semver] conventions of the `<MAJOR>.<MINOR>.<PATCH>`
  format.
- __Entrypoint__ (`Node`): The command to run the plugin's executable
  artifact from the command line.
  - (Child) __On__ (`Node`): An entry for a specific architecture.
    - (Named Attribute) __Arch__ (`String`): The target triple for the
      architecture whose entrypoint is being specified.
    - (Positional Attribute) __Operation__ (`String`): The entrypoint CLI
      program to run.
- __License__ (`String`): An [SPDX license expression][spdx_license_expr]
  specifying all licenses which apply to the plugin. See the
  [SPDX license list][spdx_license_list] for the full list of known named
  licenses.
- __Dependencies__ (`Node`): A node containing all dependency information
  for the plugin. This is the set of plugins whose outputs the current plugin
  consumes in the course of its own execution.
  - (Child) __Plugin__ (`Node`): An individual plugin node, specifying the
    following attributes:
    - (Positional Attribute) __Name__ (`String`): The name of the plugin, of
      the form `<PUBLISHER>/<NAME>`.
    - (Named Attribute) __Version__ (`String`): The version, specified as a
      SemVer version constraint of [the forms accepted by Rust's Cargo
      ecosystem][cargo_semver]. Namely, the following operators are supported
      (note that this list explicitly _does not_ include
      [hyphen-ranges][npm_hyphen_ranges] or [x-ranges][npm_x_ranges], both of
      which are supported in the NPM ecosystem.)
      - _Caret Requirements_: Permits "SemVer-compatible" updates. Example:
        `^1.2.3`.
      - _Tilde Requirements_: Specifies a minimal version with some ability to
        update. Example: `~1.2.3`.
      - _Wildcard Requirements_: Permits any version in the place of the
        wildcard. Example: `1.2.*`
      - _Comparison Requirements_: Manually specify a version constraint using
        less than, less than or equal to, equal to, greater than or equal to,
        or greater than operators. Example: `>= 1.2.3`.
      - _Multiple Version Requirements_: Permits combining any of the above
        forms of constraints by separating them with a comma.
        Example: `>= 1.2.3, < 2.0.0`.
    - (Named Attribute) __Manifest__ (Optional / `String`): The download
      location of the __plugin discovery manifest__ for this plugin. Until a
      plugin registry is established for Hipcheck plugins, this attribute will
      effectively be required, as otherwise Hipcheck will have no mechanism to
      discover the location of the download metadata for a plugin.

Given the above definitions, an example plugin manifest may look like:

```kdl
publisher "mitre"
name "affiliation"
version "0.1.0"
license "Apache-2.0"
entrypoint {
  on arch="aarch64-apple-darwin" "./hc-mitre-affiliation"
  on arch="x86_64-apple-darwin" "./hc-mitre-affiliation"
  on arch="x86_64-unknown-linux-gnu" "./hc-mitre-affiliation"
  on arch="x86_64-pc-windows-msvc" "./hc-mitre-affiliation"
}

dependencies {
  plugin "mitre/git" version="0.1.0" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl"
}
```

The __plugin discovery manifest__ is an additional manifest which tells
Hipcheck where to find the correct plugin artifact for a specific target
architecture. In the future, Hipcheck's plugin system may support one or
more plugin registries which ease discovery of Hipcheck plugins. In the
short term, the standard mechanism for discovery of Hipcheck plugins
will be for plugin producers to distribute a per-plugin discovery manifest
which contains version and target architecture information necessary for
Hipcheck to download the _correct_ plugin executable artifact to run
on the end-user's host.

The format of the plugin discovery manifest is as follows:

- __Plugin__ (many `Node`s): describes the download information for a
  specific plugin version and architecture.
  - (Named Attribute) __Version__ (`String`): A `SemVer` version of the plugin.
    _Not_ a version _requirement_ as in the plugin manifest file, but only a
    specific concrete version.
  - (Named Attribute) __Arch__ (`String`): The target architecture, given as a
    target triple. (See "Target Triples" appendix below for more information on
    supported target triples).
  - (Child) __URL__ (`Node`): The URL of the archive file to download
    containing the plugin executable artifact and plugin manifest.
    - (Positional Attribute) __URL__ (`String`): The actual URL test.
  - (Child) __Hash__ (`Node`): Information for the hash algorithm and digest
    the consumer should use to validate the downloaded plugin archive.
    - (Named Attribute) __Alg__ (`String`): The hash algorithm used to validate
      the downloaded plugin archive. Supported hash algorithm can be
      `"SHA256"` or `BLAKE3`. More algorithms may be added in the future. No
      support for older, weaker algorithms such as SHA-1, SHA-1CD, or MD5 is
      planned.
    - (Named Attribute) __Digest__ (`String`): The hexadecimal digest produced
      by the hash algorithm, to compare against the result of the consumer
      hashing the downloaded plugin archive with the prescribed hash algorithm.
  - (Child) __Compress__ (`Node`): Defines how to handle decompressing the
    downloaded plugin archive.
    - (Named Attribute) __Format__ (`String`): The name of the compression
      and archive format used for the downloaded artifact. Because plugins must
      include both the executable artifact and a plugin manifest, the
      downloaded artifact linked in a plugin download manifest must _always_ be
      an archive in one of the following formats. The supported formats are:
      - `tar.xz`: Archived with `tar` and compressed with the XZ algorithm.
      - `tar.gz`: Archived with `tar` and compressed with the Gzip algorithm.
      - `tar.zst`: Archived with `tar` and compressed with the zstd algorithm.
      - `tar`: Archived with `tar`, not compressed.
      - `zip`: Archived and compressed with `zip`.
  - (Child) __Size__ (`Node`): Describes the size of the downloaded artifact.
    This is used to validate the download was successful by comparing the size
    of the artifact before decompression to the specified size. This also helps
    protect against attempts at distributing malformed artifacts with hashes
    that collide with the original artifact by requiring the new artifact to
    match the size in bytes of the original. Note this _does not_ protect the
    user in the case of an attacker being able to modify _both_ the distributed
    artifact(s) and the plugin download manifest.
    - (Named Attribute) __Bytes__ (`Number`): The size of the downloaded
      artifact in bytes.

An example plugin download manifest with two entries would look like this:

```kdl
plugin version="0.1.0" arch="aarch64-apple-darwin" {
  url "https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-aarch64-apple-darwin.tar.xz"
  hash alg="SHA256" digest="b8e111e7817c4a1eb40ed50712d04e15b369546c4748be1aa8893b553f4e756b"
  compress format="tar.xz"
  size bytes=2_869_896
}

plugin version="0.1.0" arch="x86_64-apple-darwin" {
  url "https://github.com/mitre/hipcheck/releases/download/hipcheck-v3.4.0/hipcheck-x86_64-apple-darwin.tar.xz"
  hash alg="SHA256" digest="ddb8c6d26dd9a91e11c99b3bd7ee2b9585aedac6e6df614190f1ba2bfe86dc19"
  compress format="tar.xz"
  size bytes=3_183_768
}
```

With the plugin download manifest in place, Hipcheck has the information
necessary to download, validate, and run plugins requested by the user.

### Plugin CLI

Plugins for Hipcheck run as their own processes, started as Command Line
Interface programs, which communicate with the main Hipcheck process via
[gRPC]. In the future, other interfaces may be added, and indeed RFD #3
contemplated other interfaces for communication between Hipcheck and its
plugins. gRPC has been chosen in the immediate-term because it is a
well-understood mechanism with implementations in many popular languages.

To enable communication over gRPC, Hipcheck requires that plugins provide
a CLI which accepts a `--port <PORT>` argument, enabling Hipcheck to centrally
manage the ports plugins are listening on. The port provided via this
CLI argument must be the port the running plugin process listens on for
gRPC requests, and on which it returns responses.

Plugins are also expected to produce log information by outputting to
`stderr`, and leave redirection, rotation, and other log management
tasks to Hipcheck. Logs do not have a required format, as we recognize
various ecosystems and existing tools and libraries will have their own
formats.

The __plugin manifest__ includes an __entrypoint__ key, the value of
which must be the CLI argument to start the plugin executable artifact,
such that appending the `--port <PORT>` flag succeeds in setting the
port on the process which is then started.

Once started, the plugin should continue running, listening for gRPC
requests from Hipcheck, until shut down by the Hipcheck process.

### Plugin gRPC Interface

The plugin gRPC interface is described in the following ProtoBuf version 3
file, with comments. For more details on the interaction between plugins
and Hipcheck, see the section ["Analysis"](#analysis) of this RFD.

```proto
syntax = "proto3";

service Plugin {
    /**
    * Get schemas for all supported queries by the plugin.
    *
    * This is used by Hipcheck to validate that:
    *
    * - The plugin supports a default query taking a `target` type if used
    *   as a top-level plugin in the user's policy file.
    * - That requests sent to the plugin and data returned by the plugin
    *   match the schema during execution.
    */
    rpc GetQuerySchemas () returns (stream Schema);

    /**
     * Hipcheck sends all child nodes for the plugin from the user's policy
     * file to configure the plugin.
     */
    rpc SetConfiguration (Configuration) returns (ConfigurationResult);

    /**
     * Get the default policy for a plugin, which may additionally depend on
     * the plugin's configuration.
     */
    rpc GetDefaultPolicyExpression () returns (PolicyExpression);

    /**
     * Open a bidirectional streaming RPC to enable a request/response
     * protocol between Hipcheck and a plugin, where Hipcheck can issue
     * queries to the plugin, and the plugin may issue queries to _other_
     * plugins through Hipcheck.
     *
     * Queries are cached by the publisher name, plugin name, query name,
     * and key, and if a match is found for those four values, then
     * Hipcheck will respond with the cached result of that prior matching
     * query rather than running the query again.
     */
    rpc InitiateQueryProtocol (stream Query) returns (stream Query);
}

message Configuration {
    // JSON string containing configuration data expected by the plugin,
    // pulled from the user's policy file.
    string configuration = 0;
}

enum ConfigurationStatus {
    // An unknown error occured.
    ERROR_UNKNOWN = 0;
    // No error; the operation was successful.
    ERROR_NONE = 1;
    // The user failed to provide a required configuration item.
    ERROR_MISSING_REQUIRED_CONFIGURATION = 2;
    // The user provided a configuration item whose name was not recognized.
    ERROR_UNRECOGNIZED_CONFIGURATION = 3;
    // The user provided a configuration item whose value is invalid.
    ERROR_INVALID_CONFIGURATION_VALUE = 4;
}

message ConfigurationResult {
    // The status of the configuration call.
    ConfigurationStatus status = 0;
    // An error message, if there was an error.
    optional string message = 1;
}

message PolicyExpression {
    // A policy expression, if the plugin has a default policy.
    // This MUST be filled in with any default values pulled from the plugin's
    // configuration. Hipcheck will only request the default policy _after_
    // configuring the plugin.
    optional string policy_expression = 0;
}

message Schema {
    // The name of the query being described by the schemas provided.
    //
    // If either the key and/or output schemas result in a message which is
    // too big, they may be chunked across multiple replies in the stream.
    // Replies with matching query names should have their fields concatenated
    // in the order received to reconstruct the chunks.
    string query_name = 0;

    // The key schema, in JSON Schema format.
    string key_schema = 1;

    // The output schema, in JSON Schema format.
    string output_schema = 2;
}

enum QueryState {
    // Something has gone wrong.
    QUERY_UNSPECIFIED = 0;

    // We are submitting a new query.
    QUERY_SUBMIT = 1;

    // We are replying to a query and expect more chunks.
    QUERY_REPLY_IN_PROGRESS = 2;

    // We are closing a reply to a query. If a query response is in one chunk,
    // just send this. If a query is in more than one chunk, send this with
    // the last message in the reply. This tells the receiver that all chunks
    // have been received.
    QUERY_REPLY_COMPLETE = 3;
}

message Query {
    // The ID of the request, used to associate requests and replies.
    // Odd numbers = initiated by `hc`.
    // Even numbers = initiated by a plugin.
    int32 id = 1;

    // The state of the query, indicating if this is a request or a reply,
    // and if it's a reply whether it's the end of the reply.
    QueryState state = 2;

    // Publisher name and plugin name, when sent from Hipcheck to a plugin
    // to initiate a fresh query, are used by the receiving plugin to validate
    // that the query was intended for them.
    //
    // When a plugin is making a query to another plugin through Hipcheck, it's
    // used to indicate the destination plugin, and to indicate the plugin that
    // is replying when Hipcheck sends back the reply.
    string publisher_name = 3;
    string plugin_name = 4;

    // The name of the query being made, so the responding plugin knows what
    // to do with the provided data.
    string query_name = 5;

    // The key for the query, as a JSON object. This is the data that Hipcheck's
    // incremental computation system will use to cache the response.
    string key = 6;

    // The response for the query, as a JSON object. This will be cached by
    // Hipcheck for future queries matching the publisher name, plugin name,
    // query name, and key.
    string output = 7;
}
```

## Using Plugins

The RFD so far has described how plugins are defined by plugin creators, and
the API expected of plugins in order for Hipcheck to interact with them. What
has _not_ been described is how end-users of Hipcheck should use plugins
themselves. This section answers that question.

### Policy Files

Today, Hipcheck determines what analyses to run and how to configure those
analyses with a "configuration file," and when you first install Hipcheck you
must run the `hc setup` command to get a copy of the default configuration
files and helper scripts necessary for Hipcheck to run. This approach has
several problems:

- Requiring users to run `hc setup` after installation is an extra step
  and source of complexity. If users forget to do it, their first
  interactions with Hipcheck come in the form of an error message.
- The configuration files are large, not easy to scan and understand,
  and distribute total configuration of the tool across multiple files.
  While the main `Hipcheck.toml` file defines the central scoring
  configuration and many basic configuration items, it _does not_
  configure all individual analyses, many of which delegate their
  configuration to an additional file.
- The TOML format, chosen mostly out of familiarity, turns out to be
  an awkward fit for many of the kinds of data we want to represent
  in Hipcheck's configuration files.
- The use of the term "configuration" itself can be confusing, especially
  since we will likely want in the future for users to be able to
  configure Hipcheck in ways which are necessary for operation but not
  meaningful for analysis.

Given all of the above, we have reached a new design for how to handle
what we call "configuration" today in Hipcheck.

Today's "configuration files" which are installed by `hc setup` will be
replaced with a "policy file" which the user writes. This policy file will
be called `Hipcheck.kdl` by default.

The policy file will be in KDL format, not TOML. The schema will be
designed to be easier to scan and understand than the one used today.

The policy file will not live in a central per-user or per-system directory,
as it is intended to today. Instead it can be in any location, and by default
Hipcheck will check the current directory. Otherwise, the user will provide
a command line flag to the `hc` command to indicate the location of the
policy file.

The `hc setup` step will be eliminated (this also requires changing Hipcheck
to no longer need the helper script it requires for some analyses today).

It's worth explaining in more detail the particular choice to move away from
the term "configuration" and toward the term "policy" for this file. One of
the central ideas in Hipcheck is there there is no universal "risk" metric
that Hipcheck is uncovering with its analyses; we as the makers of Hipcheck
do not have special insight into the one true way to discern the security
risk of using specific third-party dependencies. Instead, we see Hipcheck as
a tool to empower open source maintainers and consumers of open source to
better understand what they're making and using. Any score produced by Hipcheck
is an expression of the _policy_ of the user running it. While we will always
try to provide sensible defaults for users out of the box, we also want to
encourage users to tune their policies to better reflect what they care about.

The name change also has a more practical purpose of deconflicting the word
"configuration" to in the future be used for configuring Hipcheck in ways
that aren't relevant for analyses. For example, we will likely in the future
want a way to tell Hipcheck to use specific certificates for HTTP requests,
and having a configuration file to enable this would be helpful.

With all of this now said, let's look at an example of a future policy file
for Hipcheck, and explain its parts:

```kdl
plugins {
    plugin "mitre/activity" version="0.1.0"
    plugin "mitre/binary" version="0.1.0"
    plugin "mitre/fuzz" version="0.1.0"
    plugin "mitre/review" version="0.1.0"
    plugin "mitre/typo" version="0.1.0"
    plugin "mitre/affiliation" version="0.1.0"
    plugin "mitre/entropy" version="0.1.0"
    plugin "mitre/churn" version="0.1.0"
}

analyze {
    investigate policy="(gt 0.5 $)"
    investigate-if-fail "mitre/typo" "mitre/binary"

    category "practices" {
        analysis "mitre/activity" policy="(lte 52 $/weeks)" weight=3
        analysis "mitre/binary" policy="(eq 0 (count $))"
        analysis "mitre/fuzz" policy="(eq #t $)"
        analysis "mitre/review" policy="(lte 0.05 $/pct_reviewed)"
    }

    category "attacks" {
        analysis "mitre/typo" policy="(eq 0 (count $))" {
            typo-file "./config/typo.kdl"
        }

        category "commit" {
            analysis "mitre/affiliation" policy="(eq 0 (count $))" {
                orgs-file "./config/orgs.kdl"
            }

            analysis "mitre/entropy" policy="(eq 0 (count (filter (gt 8.0) $)))"
            analysis "mitre/churn" policy="(eq 0 (count (filter (gt 8.0) $)))"
        }
    }
}
```

As you can see, the file has two main sections: a `plugins` section, and an
`analyze` section. We can explore each of these in turn.

#### The `plugins` Section

This section defines the plugins that will be used to run the analyses
described in the file. These plugins are defined in the same way dependent
plugins are defined in the plugin manifest, with a plugin name, version,
and an optional `manifest` field (not shown in the example above) which
provides a link to the plugin's download manifest. In the future, when a
Hipcheck plugin registry is established, the `manifest` field will become
optional. In the immediate term it will be practically required.

Each plugin will be downloaded by Hipcheck, its size and checksum
verified, and the plugin contents decompressed and unarchived to produce
the plugin executable artifacts and plugin manifest which will be stored
in a local plugin cache. Hipcheck will do the same recursively for all
plugins.

In the future Hipcheck will likely add some form of dependency resolution
to minimize duplication of shared dependencies, similar to what exists in
other more mature package ecosystems. For now the details of this mechanism
are left unspecified.

#### The `analyze` Section

This section defines the set of analyses that Hipcheck should use to assess
any targets of analysis, and just as importantly constructs the "score tree"
which will take the output of those analyses and use them to produce an overall
risk score. See [the Complete Guide to Hipcheck's section on scoring][hipcheck_scoring]
for more information on how Hipcheck's scoring mechanism works.

Today, the Hipcheck score tree is static in any specific version. The structure
of the current `Hipcheck.toml` file contains this structure, which must
exactly match the specified structure expected by Hipcheck. Analyses are grouped
into categories, and both analyses and categories have weights attached to them
which change the degree to which the results of individual analyses or
categories of analyses contribute to the overall risk score. While this has
worked fine for Hipcheck so far, it obviously will not work in the presence of
plugins, with which the user could run any arbitrary set of analyses.

The need to gracefully represent this is part of the appeal of KDL over TOML.
TOML makes handling successive degrees of nesting fairly difficult; while it
_can_ support nested sections, it does so awkwardly and is best suited instead
for mostly-flat hierarchies. KDL, by contrast, is designed to easily support
nested structures, but with a more human-readable syntax that JSON, and some
other niceties like avoiding all numbers being implicitly encoded as floats,
which JSON suffers from, or easy-to-mess-up indenting rules or other special
cases like the Norway country code evaluating to `false` which are part of
YAML. Experience with many "cloud native" tools and Continuous Integration
configuration systems has made many, the Hipcheck maintainers included, wary
of YAML as a configuration language choice.

For the overall recommendation, there are two important values to understand:
the `investigate` node, and the `investigate-if-fail` node.

The `investigate` node accepts a `policy` key-value pair, which takes a
policy expression as a string, with the input to the policy expression being
the numeric output of the scoring tree reduction, a floating pointer number
between `0.0` and `1.0` inclusive. This defines the policy used to determine
if the "risk score" produced by the score tree should result in Hipcheck
flagging the target of analysis for further investigation.

The `investigate-if-fail` node enables users of Hipcheck to additionally
mark specific analyses such that if those analyses produce a failed result,
the overall target of analysis is marked for further investigation regardless
of the risk score. In this case, the risk score is still calculated and all
other analyses are still run.

The rest of the `analyze` section is composed of any number of nested
`analysis` and `category` nodes. `category` is used to group any number of
`analysis` nodes for grouped weighting. All `analysis` and `category` nodes
may receive a `weight` key-value attribute which sets their numeric weight
as an integer greater than 0. `analysis` nodes may additional receive a
`policy` field as a key-value attribute containing a policy expression string.

The default weight for all `analysis` and `category` nodes is `1`.

`analysis` nodes take a positional string attribute which is the name of the
plugin, which must match the name defined in the `plugins` section of the
policy file, with the producer name and plugin name separated by a `/`.

Any child nodes of an `analysis` node are passed to the associated plugin
during the beginning of Hipcheck's execution, for validation and to configure
the analysis to be run.

### Policy Expressions

"Policy expressions" are a small expression language in Hipcheck, as part of
policy files, which are intended to let users of Hipcheck express policies
for how the output of individual plugins should be used to produce a pass/fail
determination as an input for scoring. The scoring system relies on converting
these per-analysis pass/fail results, with weights per-analysis and
per-category, into an overall risk score from which we derive the "pass" or
"investigate" recommendation.

In the new plugin system, it is possible for plugins to depend on other
plugins, using the results output by a dependent plugin as inputs, performing
additional work, and then producing new outputs. At the same time, any plugin
may be listed in a user's policy file, in which case its output, which may
be arbitrary JSON data, must be reduced down to a pass/fail result. Policy
expressions exist to define that reduction.

The policy expression language is limited. It does not permit user-defined
functions, assignment to variables, or the retention of any state. Any
policy expression which does _not_ result in a boolean output will produce
an error. The primitive types accepted in policy expressions are only
integers, floating point numbers, booleans, datetimes, and spans of time.

- `Integer`: 64-bit signed integers.
- `Float`: 64-bit IEEE double-precision floating point numbers.
- `Boolean`: True (`#t`) or false (`#f`).
- `DateTime`: A datetime value with a date, optional time, and optional UTC
  offset. This follows a modified version of ISO8601 (see below for details).
- `Span`: A span of time, in some combination of weeks, days, hours, minutes,
  seconds. This follows a modified version of ISO8601 (see below for details).

Policy expressions also accept arrays, which are ordered sequences of
values of the same type from the list of primitive types above. Arrays may
not currently be nested. Array syntax is a list of primitives separated
by spaces, surrounded by open and close square brackets. Example: `[1 2 3]`.

The following functions are currently defined for policy expressions:

- Comparison Functions
  - __`gt`__: Greater than comparison. Example: `(gt 1 2)` is `#f`. This
    function can be partially-evaluated when passed to a higher-order array function,
    in which case the order of the parameters is switched to match the expected
    result. So `(filter (gt 4) [0 2 4 6 8 10])` evaluates to `[6 8 10]`.
  - __`lt`__: Less than comparison. Example: `(lt 1 2)` is `#t`. This function
    can be partially-evaluated when passed to a higher-order array function, in which
    case the order of the parameters is switched to match the expected result.
    So `(filter (lt 4) [0 2 4 6 8 10])` evaluates to `[0 2]`.
  - __`gte`__: Greater than or equal to comparison. Example: `(gte 1 2)` is
    `#f`. This function can be partially-evaluated when passed to `filter` or
    `foreach`, in which case the order of the parameters is switched to match
    the expected result. So `(filter (gte 4) [0 2 4 6 8 10])` evaluates to
    `[4 6 8 10]`.
  - __`lte`__: Less than or equal to comparison. Example: `(lte 1 2)` is `#t`.
    This function can be partially-evaluated when passed to `filter` or
    `foreach`, in which case the order of the parameters is switched to match
    the expected result. So `(filter (lte 4) [0 2 4 6 8 10])` evaluates to
    `[0 2 4]`.
  - __`eq`__: Equality comparison. Example: `(eq 1 1)` is `#t`. This function
    can be partially-evaluated when passed to a higher-order array function.
  - __`neq`__: Not-equal comparison. Example: `(neq 1 1)` is `#f`. This
    function can be partially-evaluated when passed to a higher-order array function.
- Mathematical Functions
  - __`add`__: Adds two integers, floats, or spans together. Adds a datetime and a
   span to get new datetime. This function can be partially-evaluated when passed to
   a higher-order array function.
  - __`sub`__: Subtracts one integer, float, or span from another. Subtracts a
    span from a datetime to get a new datetime. This function can be
    partially-evaluated when passed to `foreach`, in which case the order of
    the parameters is switched to match the expected result. So
    `(foreach (sub 1) [1 2 3 4 5])` becomes `[0 1 2 3 4]`.
- DateTime Specific Function
  - __`duration`__: Returns the span representing the difference between two
    datetimes.
- Logical Functions
  - __`and`__: Performs the logical "and" of two booleans. Example:
    `(and #t #f)` becomes `#f`.
  - __`or`__: Performs the logical "or" of two booleans. Example:
    `(or #t #f)` becomes `#t`.
  - __`not`__: Performs the logical negation of a boolean. Example:
    `(not #f)` becomes `#t`.
- Numeric Array Functions
  - __`max`__: Finds the maximum value in an array of integers or floats.
    Example: `(max [0 1 2 3 4 5])` becomes `5`.
  - __`min`__: Finds the minimum value in an array of integers or floats.
    Example: `(min [0 1 2 3 4 5])` becomes `0`.
  - __`avg`__: Finds the average value in an array of integers or floats.
    Example: `(avg [0 1 2 3 4 5)` become `2.5`.
  - __`median`__: Finds the median value in an array of integers or floats.
    Sorts the array as part of this calculation. Example:
    `(median [5 1 2 4 3])` becomes `3`.
  - __`count`__: Counts the number of elements in an array. Example:
    `(count [0 1 2 3 4 5])` becomes `6`.
- Logical Array Functions
  - __`all`__: Checks a logical function to see if it is true for all
    elements in an array. Example: `(all (gt 0) [0 1 2 3 4 5])` becomes
    `#f`.
  - __`nall`__: Checks a logical function to see if it is not true for
    at least one element in an array. Example: `(nall (gt 0) [0 1 2 3 4 5])`
    becomes `#t`.
  - __`some`__: Checks a logical function to see if it is true for at
    least one element in an array. Example: `(some (eq 0) [0 1 2 3 4 5])`
    becomes `#t`.
  - __`none`__: Checks a logical function to see if it is not true for
    all elements in an array. Example: `(none (eq 0) [0 1 2 3 4 5])` becomes
    `#f`.
- Map / Filter Array Functions
  - __`filter`__: Filter the elements out of an array based on whether
    they pass a boolean test function. Example:
    `(filter (gt 5) [1 2 3 4 5 6 7 8 9])` becomes `[6 7 8 9]`.
  - __`foreach`__: Apply a function to each element of an array. Example:
    `(foreach (add 1) [0 1 2 3 4 5])` becomes `[1 2 3 4 5 6]`.
- Helper Functions
  - __`dbg`__: Print the expression passed to it, and the result of
    evaluating that expression. Used for debugging. Example: `(dbg (add 1 1))`
    prints a log message saying `(add 1 1) => 2`.

In policy expressions, the `$` character stands for the input object, in
JSON format, and in fact any "substitution pointer" can be used. `$` is
the root JSON object, which may be of any valid JSON type. If it's an
object, the `/[field_name]` can be used to select a named field of the
object, and `/[index]` can be used to select any numbered index of an
array. So `$/items/0`, for example, would select into the `items` field
of the provided object, and then into the first (`0`-th) element of that
array.

#### DateTime format details
The policy expression DateTime primitive uses a modified version of ISO8601.
A given datetime must include a date in the format `<YYYY>-<MM>-<DD>`.

An optional time in the format `T<HH>:[MM]:[SS]` can be provided after the date.
**Decimal fractions of hours and minutes are not allowed**; use smaller time units
instead (e.g. T10:30 instead of T10.5). Decimal fractions of seconds are allowed.

The timezone is always set to UTC, but you can set an offeset from UTC by
including `+{HH}:[MM]` or `-{HH}:[MM]` after the time. All datetimes are
treated as offsets ofUTC and do not include other timezone information (e.g.
daylight savings time adjustments).

Example of valid datetimes are:
- `2024-09-25`
- `2024-09-25T08`
- `2024-09-25T08:28:35`
- `2024-09-25T08:30-05`
- `2024-09-25T08:28:35-03:30`

All comparison functions work on datetimes, with earlier datetimes
considered to be "lesser than" later ones. A span can be added to or
subtratced from a datetime to get a new datetime. The __`duration`__
function is used to find the difference in time between two datetimes,
returned as a span.

#### Span format details
The policy expression Span primitive uses a modified version of ISO8601. It can
include weeks, days, hours, minutes, and seconds. The smallest provided unit of
time (i.e. hours, minutes, or seconds) can have a decimal fraction.

While spans with months, years, or both are valid under IS08601, we do not allow
them in Hipcheck policy expressions. This is because spans greater than a day
require additional zoned datetime information (to determine e.g. how many days
are in a year or month) before we can do time arithmetic with them.
Spans with weeks, days, or both **are** allowed. Hipcheck handles this by treating
all days as periods of exactly 24 hours (without worrying about leap seconds)
and converting a week to a period of seven 24-hour days.

Spans are preceded by the letter "P" with any optional time units separated from
optional date units by the letter "T" (not whitespace, as is also allowed under
ISO8601). All units of dates and times are represented by single case-agnostic
letter abbreviations after the number.

Examples of valid spans are:
- `P4w`
- `P3d`
- `P1W2D`
- `PT4h15.25m`
- `PT5s`
- `P1w2dT3h4m5.6s`

All comparison functions work on spans. This is handled the "smart way," with
spans that have different representations but equalling the same amount of time
considred to be equal. e.g. `(eq PT1h PT60m)` will return `#t`. Spans can be added
to or subtracted from each other to return a new span.

#### Limitations of JSON Pointer syntax relative to RFC

The [JSON Pointer spec (RFC 6901)](https://datatracker.ietf.org/doc/html/rfc6901)
allows almost any character to appear in the path string. This includes control
characters, whitespace, and NULLs. But to keep the syntax compatible with Policy
expression syntax, JSON Pointers are currently limited to alphanumerics, '/'
(forward slash), '~' (tilde), and '_' (underscore).

#### Limitations of JSON value interpretation into Policy expression language

JSON can contain a number of different types, but not all of them are compatible
with the Policy expression type system. Currently, only booleans, floats, and
homogenous arrays of booleans or floats are supported. This excludes strings,
objects, and arrays containing anything but supported primitive types.

It's also useful to understand _how_ policy expressions were reached as a
design. Previous versions of the design for plugins in Hipcheck, including
RFD #3, imagined a distinction between "data" plugins which could produce
arbitrary data, and "analysis" plugins which could only produce a boolean
result to use for scoring. This distinction complicated the plugin system
design, but was trying to address a real challenge: how do you permit some
degree of flexibility for converting the results of analyses into a pass/fail
determination to use for scoring?

Policy expressions were invented to permit end-users of Hipcheck some degree
of flexibility in post-processing the results of an analysis to produce a
pass/fail result. They also nicely fit with Hipcheck's ethos of putting
users in control of determining what is considered too risky according to
their own policy.

Finally, it's worth understanding the three options that are intended to exist
for setting policy for scoring analysis results.

1. Plugins may define a default policy as a policy expression which may also
   depend on configuration items the plugin accepts in the policy file. If a
   default policy is fine, the user does not need to provide one when placing
   that plugin into a scoring tree in their policy file. If the default policy
   _is_ configurable, the user can configure it via setting the relevant
   configuration item for the plugin.
2. Users may write a policy expression for the plugin. If the plugin has a
   default policy expression, the one provided by the user overrides it.
   These policy expressions are not as powerful as a normal programming
   language, but they allow for expression of many common reductions of
   analysis output to a pass/fail determination.
3. Users may define their own plugin which takes the output of another
   plugin, performs some more complicated computations on its result, and use
   that as their input for the score tree. This option offers the greatest
   power, but creating a plugin is also the greatest amount of work of these
   options.

## Running Plugins

The following section describes how Hipcheck itself, the `hc` binary, goes
about running and interacting with plugins through its flow. It is the
operational accompaniment to the description of the plugin gRPC interface
required of plugin authors, and should help to contextualize _why_ that
interface is the way it is.

### Phases of Execution

When a user runs Hipcheck today, there are several phases to execution, and
several "flows" for information. The phases are:

- __Target Resolution__, where Hipcheck determines _what_ the user is trying
  to analyze (the "target"). This is generally either a Git repository, which
  may be associated with a remote host like a GitHub repository, or a package
  on a known package host like NPM, PyPI, or Maven. If given a package host, for
  example as a package name and version number, Hipcheck investigates metadata
  for that package to also identify any associated Git repository.
- __Policy Loading__, where Hipcheck loads the user's policy file,
  parses it, reports errors and halts if any are found, and then proceeds.
- __Analysis__, where Hipcheck runs any active analyses based on the user's
  configuration files. As analyses run, they produce one of three results:
  success, failure, or "errored." An "errored" analysis is one that failed to
  run to completion, and which produces an error message. Successful analyses
  mean that the analysis is considered benign and does _not_ contribute to a
  higher risk score; a failed analysis means that the analysis identified some
  concerning issues and _does_ contribute to a higher risk score. When analyses
  run, their results are cached in memory by Hipcheck's incremental computation
  system, which is designed to ensure that Hipcheck does not compute expensive
  things twice. For example, both the "church" and "entropy" analyses rely on
  Git diffs, which are expensive to compute, for _all_ commits in the history
  of a project. Today, the scheduling of analyses is single-threaded, and much
  of the execution of individual analyses is _also_ single-threaded, though we
  are working to change that. We also do not currently cache any data on disk,
  meaning we may be subject to memory pressure, especially when analyzing
  targets with large amounts of Git history. Analyses, besides their main
  outcome, may additionally report "concerns," which are additional pieces of
  guidance for end-users intended to help assist them in any manual evaluation
  of a target project they may undertake based on Hipcheck's overall
  recommendation.
- __Scoring__: Hipcheck takes the results (pass or fail) of each analysis,
  along with the weight provided for each analysis, and uses them to produce
  an overall score which is compared to the user's configured risk threshold.
  If the overall score ("risk score") is higher than the user's risk threshold,
  then Hipcheck makes an "investigate" recommendation to the user, indicating
  the user should manually review the target project for possible supply chain
  security risk.

Of the above steps, __target resolution__ will continue to run _before_
Hipcheck begins running plugins, so we can ignore it for the purposes of this
RFD. By the time plugin execution of any kind has begun, Hipcheck has resolved
the user's provided target specifier into a full target which at minimum
includes a Git repository cloned locally, and may include additional data like
a remote package host or remote Git repository.

The remainder of this section breaks down the interaction with plugins at
each step of Hipcheck's execution.

#### Policy Loading

This stage has several subparts:

- Parsing the user's policy file.
- Identifying, resolving, and downloading all dependent plugins for the
  policy file, if they're not already downloaded.
- Starting all policy plugin processes on specific ports to prepare for
  interaction via gRPC.
- Validating policy-specific configuration with the policy plugins,
  reporting errors out to the user.

We'll likely want to offer an `--offline` flag and/or configuration
item which the user could use to tell Hipcheck _not_ to download
plugins itself. In that case, it would be up to the user to place
plugins into the Hipcheck plugin cache themselves.

At the end of this stage, all of Hipcheck's plugins are running, their
configurations have been validated and set, and Hipcheck is prepared to
execute the requested analyses.

The policy file is described in detail above in this RFD, so we will not
re-address the format of the file or  the schema of its contents.

Plugin resolution is also not yet fully described, though the end goal
is that all requested plugins have been installed locally and are prepared
to run.

Hipcheck will manage all port assignments and any future networking
configuration of plugins. All plugins must accept a CLI input to configure
their assigned port number. This is the number that Hipcheck will use
to communicate with each plugin over gRPC.

Each `analysis` block in the policy file may have child attributes, which
are passed to the associated plugin via a gRPC call for validation. Before
being passed, they will be converted to JSON. Each plugin is then expected
to validate that the provided configuration items matched the plugin's
own requirements.

At this stage, Hipcheck will also use a gRPC call to request each plugin
provide a JSON schema for the results of its analysis. This will be used
to validate and typecheck any policy expression provided by the user,
and during execution of the analyses will be used by Hipcheck to validate
data returned by the plugin.

#### Analysis

The analysis phase is the main plugin-relevant phase during Hipcheck
execution. In this phase, Hipcheck begins, in parallel, the execution of
each analysis requested by the user in their policy file.

This is done with `hc` starting a bidirectional streaming RPC call with
each plugin. The protocol followed within that RPC call is what we'll call the
"query protocol," which is what's described in this section.

The query protocol begins with Hipcheck issuing default queries to each
of the plugins specified in the user's policy file, providing the plugin with
a JSON representation of the "target" resolved by Hipcheck in the target
resolution phase.

In the query protocol, plugins may also make queries to other plugins
through Hipcheck. This is done by sending `Query` messages back with
the `publisher_name` and `plugin_name` indicating how Hipcheck should
route the plugin, and `query_name` and `key` indicating the query to make
and the input parameters to pass. Those four values together are used
by Hipcheck to cache the results of queries so that queries with
matching keys are only computed once. This is especially helpful for
expensive queries like computing Git diffs for all commits, on which
some existing analyses in Hipcheck rely.

Chunking for queries is handled with a `QueryState` enum; a query response
which is chunked can use the `QUERY_REPLY_IN_PROGRESS` state to indicate
that further chunks are expected, and then complete the reply with
the `QUERY_REPLY_COMPLETE` state.

Queries made in the protocol have an `id` associated with them. Odd-numbered
`id`s are used by Hipcheck itself, even-numbered IDs are used by plugins. This
is used to associate query responses to query requests based on matching `id`.

The query protocol completes when the initial default query issued by Hipcheck
receives a complete response. When all top-level queries have completed,
the analysis phase is complete and Hipcheck may move on to scoring.

#### Scoring

The construction of the scoring tree is defined by the user's nesting of
`analysis` and `category` blocks inside the `analyze` block of their policy
file. This scoring tree is then filled with the results of running the
listed analyses, applying either the default or user-provided policy
expression to their results, and then combining that score tree with the
user's configured weights to reduce to an overall risk score. That risk score
is then evaluated based on the user's overall "investigate" policy, to produce
a pass / investigate recommendation.

This stage does not involve any interaction with plugins, which are expected
to have completed their execution by this point.

## Appendix A: Target Triples

__Target Triples__ are a mechanism used by many tools across language
ecosystems to specify the ABI (Application Binary Interface) compatibility of
programs with platforms. The ABI determines whether two separately-compiled
code artifacts will be able to be linked together, and is based on the
hardware, operating system, and sometimes additional information like
configuration or toolchain of the system being used. In fact, the "triple"
in the name "target triple" is misleading, as these "triples" may include
more than three segments in order to fully specify ABI-compatibility-relevant
information about target platforms.

There is no singular canonical list of consistent target triples across tools
and ecosystems, unfortunately. In our case, we do not need to solve the problem
of identifying arbitrary target triples in general, and can instead worry about
defining target triples for the architectures where Hipcheck itself can run.
Since Hipcheck is written in Rust, we adopt the Rust list of target triple
names. While today Hipcheck provides pre-built binaries for some platforms, it
is theoretically able to run on any platform for which Rust can compile.

The list of officially-supported target triples for Hipcheck today is:

- `aarch64-apple-darwin`: Used for macOS running on "Apple Silicon" running on
  a 64-bit ARM Instruction Set Architecture (ISA).
- `x86_64-apple-darwin`: Used for macOS running on the Intel 64-bit ISA.
- `x86_64-pc-windows-msvc`: Used for Windows running on the Intel 64-bit ISA
  with the Microsoft Visual Studio Code toolchain for compilation.
- `x86_64-unknown-linux-gnu`: Used for Linux operating systems running on the
  Intel 64-bit ISA with a GNU toolchain for compilation.

The full list of targets supported by Rust is updated over time. The Rust
project officially documents its [list of supported platforms][rust_platforms],
and you can see what targets are supported by your currently-installed Rust
toolchain with the command `rustc --print target-list`.

In theory, plugins for Hipcheck may be built for any of these specific
architectures.

[semver]: https://semver.org/
[spdx_license_expr]: https://spdx.github.io/spdx-spec/v2-draft/SPDX-license-expressions/
[spdx_license_list]: https://spdx.org/licenses/
[KDL]: https://kdl.dev/
[npm_hyphen_ranges]: https://github.com/npm/node-semver?tab=readme-ov-file#hyphen-ranges-xyz---abc
[npm_x_ranges]: https://github.com/npm/node-semver?tab=readme-ov-file#x-ranges-12x-1x-12-
[cargo_semver]: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#version-requirement-syntax
[rust_platforms]: https://doc.rust-lang.org/beta/rustc/platform-support.html
[gRPC]: https://grpc.io/
[hipcheck_scoring]: https://mitre.github.io/hipcheck/docs/guide/concepts/#scoring
[rfd_3]: ./003-plugin-architecture-vision.md
[grpc]: https://grpc.io/
