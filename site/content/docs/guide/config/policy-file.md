---
title: Policy Files
weight: 1
---

# Policy Files

When running Hipcheck, users provide a "policy file", which is a
[KDL](https://kdl.dev/)-language configuration file that describes everything
about how to perform the analysis.  It specifies which top-level plugins to
execute, how to configure them, how to interpret their output using [policy
expressions](#policy-expressions), and how to weight the pass/fail result of
each analysis when calculating a final score. In this way, Hipcheck provides
users extensive flexibility in both defining risk and the set of measurements
used to evaluate it.

While editing the policy file, it may be useful to check it by running
[`hc ready --policy <POLICY_FILE_PATH>`](@/docs/guide/cli/hc-ready.md).

Let's now look at an example policy file to examine its parts more closely:

```
plugins {
    plugin "mitre/activity" version="^0.1"
    plugin "mitre/binary" version="^0.1"
    plugin "mitre/fuzz" version="^0.1"
    plugin "mitre/review" version="^0.1"
    plugin "mitre/typo" version="^0.1"
    plugin "mitre/affiliation" version="^0.1"
    plugin "mitre/entropy" version="^0.1"
    plugin "mitre/churn" version="^0.1"
}

analyze {
    investigate policy="(gt 0.5 $)"
    investigate-if-fail "mitre/typo" "mitre/binary"

    category "practices" {
        analysis "mitre/activity" policy="(lte $ 52)" weight=3
        analysis "mitre/binary" policy="(eq 0 (count $))" {
            binary-file "./config/Binary.toml"
        }
        analysis "mitre/fuzz" policy="(eq #t $)"
        analysis "mitre/review" policy="(lte $ 0.05)"
    }

    category "attacks" {
        analysis "mitre/typo" policy="(eq 0 (count $))" {
            typo-file "./config/Typos.toml"
        }

        category "commit" {
            analysis "mitre/affiliation" policy="(eq 0 (count $))" {
                orgs-file "./config/Orgs.toml"
            }

            analysis "mitre/entropy" policy="(eq 0 (count (filter (gt 8.0) $)))" {
                langs-file "./config/Langs.toml"
            }
            analysis "mitre/churn" policy="(lte (divz (count (filter (gt 3) $)) (count $)) 0.02)" {
                langs-file "./config/Langs.toml"
            }
        }
    }
}
```

As you can see, the file has two main sections: a `plugins` section, and an
`analyze` section. We can explore each of these in turn.

## The `plugin` Section

This section defines the plugins that will be used to run the analyses
described in the file. These plugins are defined with a name, version, and an
optional manifest field (not shown in the example above) which provides a link
to the plugin's download manifest. For an example of the manifest field, see
[here](@/docs/guide/making-plugins/release.md). In the future, when a Hipcheck
plugin registry is established, the manifest field will become optional. In the
immediate term it will be practically required.

The `version` field adheres to [SemVer](https://semver.org/) syntax for version
strings. We've provided additional support for requirement syntax, with version
prefaced with requirement operators (e.g., `^0.2`) and multiple requirements
permitted, separated by commas (e.g., `>=1.2.5, <1.5.0`). In the policy file
example above, the version requirement `^0.1` specifies that any plugin version
between `0.1.0` (inclusive) and `0.2.0` (exclusive) is acceptable. *NOTE:* If
an exact version is specified in the `version` field, e.g., `0.2.1`, then a
plugin with that specific version is required. While the comparison operators
are the same as those offered by
[Cargo](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
, this exact version handling deviates from how Cargo specify dependencies.

The `manifest` field can be either a URL to the plugin's **download manifest**,
or a local path to the plugin's **plugin manifest**. The latter option exists
to allow for local testing of plugins without requiring the changes first be
published.

At runtime, each plugin that does not have a local-path-type `manifest` field
will be downloaded by Hipcheck, its size and checksum verified, and the plugin
contents decompressed and unarchived to produce the plugin executable artifacts
which will be stored in a local plugin cache.  Hipcheck will do the same
recursively for all plugins.

In the future Hipcheck will likely add some form of dependency resolution to
minimize duplication of shared dependencies, similar to what exists in other
more mature package ecosystems. For now the details of this mechanism are left
unspecified.

## The `analysis` Section

Whereas the `plugin` section is simply a flat list telling Hipcheck which
plugins to resolve and start up, the `analysis` section composes those
analyses into a score tree.

The score tree is comprised of a series of nested "categories" that eventually
terminate in analysis leaf nodes. Whether an analysis appears in this tree
determines whether Hipcheck actually queries it, so although you can list
plugins in the `plugins` section that do not appear in the `analysis` section,
they will not be run. On the contrary, specifying a plugin in the `analysis`
section that is not in the `plugins` section is an error.

### The Score Tree

Each category and analysis node in the tree has an optional `weight` field,
which is an integer that dictates how much or little that node's final score of
0 or 1 (pass and fail, respectively) should count compared to its neighbors at
the same depth of the tree. If left unspecified, the weight of a node defaults
to `1`.

Once all the weights are normalized, an individual analysis's contribution to
Hipcheck's final score for a target can be calculated by multiplying its own
weight and the weight of all its parent categories up to the top of the
`analysis` section. As each analysis produces a pass/fail result, the
corresponding `0` or `1` is multiplied with that analysis's contribution
percentage and added to the overall score.

Users may also run `hc scoring --policy <FILE_PATH>` to see a version of the
score tree with normalized weights for a given policy file.

See [the Complete Guide to Hipcheck's section on scoring][hipcheck_scoring]
for more information on how Hipcheck's scoring mechanism works.

### Configuration

A plugin author may choose to provide a set of parameters so that users may
configure the plugin's behavior. These can be set inside the corresponding
brackets for each analysis node. For example, see the `binary-file`
configuration inside `mitre/binary`. The provided key-value pairs are passed to
their respective plugins at startup.

### Policy Expressions

Hipcheck plugins return data or measurements on data in JSON format, such that
other plugins could be written to consume and process their output. However,
the scoring system of Hipcheck relies on turning the output of each top-level
plugin into a pass/fail evalution. In order to facilitate transforming plugin
data into a boolean value, Hipcheck provides "policy expressions", which are a
small expression language. See [here](@/docs/guide/config/policy-expr.md) for a
reference on the policy expression language.

Users can define the pass/fail policy for an analysis node in the score tree
with a `policy` key. As described in more detail in the policy expression
reference, a policy used in analysis ought to take one or more JSON pointers
(operands that start with `$`) as entrypoints for part or all of the JSON object
returned by the analysis to be fed into the expression. Additionally,
all policies should ultimately return a boolean value, with `true`
meaning that the analysis passed.

Instead of users always having to define their own policy expressions, plugins
may define a default pass/fail policy that may depend on configuration items
that the plugin accepts in the `analysis` section of the policy file. If a
plugin's default policy is acceptable, the user does not need to provide a
`policy` key when placing that plugin into a scoring tree in their policy file.
If the default policy is configurable, the user can configure it by setting the
relevant configuration item for the plugin. Note that any user-provided policy
will always override the default policy.

Finally, if the policy expression language is not powerful enough to express a
desired policy for a given analysis, users may define their own plugin which
takes the analysis output, performs some more complicated computations on it,
and use that as their input for the score tree.

### Final Scoring and Investigation

Once the policies for each top-level analysis has been evaluated, the score
tree produces the final score. Hipcheck now looks at the `investigate` field of
the policy file.

This node accepts a `policy` key-value pair, which takes a policy expression as
a string. The input to the policy expression is the numeric output of the
scoring tree reduction, therefore a floating pointer number between 0.0 and 1.0
inclusive. This defines the policy used to determine if the "risk score"
produced by the score tree should result in Hipcheck flagging the target of
analysis for further investigation.

The `investigate-if-fail` node enables users of Hipcheck to additionally mark
specific analyses such that if those analyses produce a failed result, the
overall target of analysis is marked for further investigation regardless of
the risk score. In this case, the risk score is still calculated and all other
analyses are still run.

## Macros

The policy file parsing system supports a few simple macros to increase
flexibility. Macros start with a `#`, followed by a name of two or more
characters, and then an optional open and closed parentheses containing data.

- `#rel("<PATH>")` - The `#rel` macro takes a KDL string object as a parameter.
	At parse time, the contained string is interpreted as a path, and that path
	is interpreted as relative to the directory from which the policy file was
	loaded. Without using `#rel()`, paths specified in policy files will be
	interpreted as relative to the directory from which `hc` is	run. Example:
	`binary-file #rel("Binary.toml")`
- `#env("<VAR>")` - The `#env` macro takes a KDL string object as a parameter
 	and allows for parse-time environment variable resolution. For instance,
	`api-token #env("API_TOKEN")`
