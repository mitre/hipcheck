---
title: Hipcheck's Ramp of Maximum Configurability
authors:
  - Andrew Lilley Brinker
extra:
  author_img: "images/authors/andrew.jpg"
---

Hipcheck offers different levels of configurability, to provide you with an easy and useful Day 0 experience while also empowering full control over what analyses run and how they're scored.

<!-- more -->

---

{% info(title="What is Hipcheck?") %}
Hipcheck is a CLI tool for assessing open source software you use or are considering using. It investigates contribution history, package metadata, and more to understand the _practices_ used to develop the software.
{% end %}

A key belief we have for Hipcheck is that [_you_ should control what policies to apply when assessing your software dependencies][values]. Many analysis tools, including common static code analyzers or best practice checkers, check compliance against a hard-coded list of rules. Although these can be useful, they enforce expectations around what is "good" based on the beliefs of the tool makers, not the tool users.

Hipcheck started this way too. We created it by combining analyses we believed were useful for deciding whether to use an open source software dependency. Over time that set of analyses grew, and we continued to talk with open source software users in government, industry, and open source software communities to understand how they decide when to use a dependency.

We discovered there is no single accepted way to assess the risk of a dependency. Different organizations face different threats and have different tolerances for risk. In response, we worked to make each Hipcheck analysis more configurable by growing our configuration files and making more parts of our hard-coded analyses tunable.

We could do better. Our vision went from having a fixed set of configurable analyses to having a _configurable set of configurable analyses_. Not only could you decide _how_ the analyses run, you could decide _what_ analyses to run.

With [Hipcheck 3.8.0][3.8.0], we stabilized support for third-party plugins, and that vision of __maximum configurability__ became a reality. As of 3.8.0, you can use a ["policy file"][policy_file] to specify what plugins to run and how to configure them; but it goes deeper than that.

To balance maximum configurability with a good out-of-the-box user experience, we've developed a ramp of increasingly powerful options for configuring your analyses.

In this post we'll walk through each of these options, explaining what they can do and when to use them.

## Plugin Configuration with Default Policy Expressions

When you specify a plugin to run as an analysis in your policy file, you're running that plugin's "default query." This is a query that takes in a ["target"][target] (Hipcheck's term for the source repository and possibly package that you're trying to analyze) and returns some structured data.

Any plugin can specify a default ["policy expression,"][policy_expr] which specifies a policy used to produce a pass / fail decision for the default query. This policy expression can rely on configuration which is set in the user's policy file, and which Hipcheck passes to the plugin during startup.

For example, the plugin [`mitre/activity`][mitre_activity] returns structured JSON data that looks like:

```json
24
```

In this plugin, the default policy expression is:

```
(lte $ P71w)
```

This checks if the weeks returned by the plugin are less than or equal to (`lte`) 71 weeks. The `P71w` syntax is a standard syntax for expressing durations, taken from the Rust [`jiff`][jiff] crate.

As the user, you can configure the default threshold to be something different from 71 weeks in your policy file, like so:

```kdl
analysis "mitre/activity" {
	weeks 52
}
```

Not all plugins will provide default policy expressions, and those that do may not expose configurable items which modify that default policy. That said, enabling a configurable default policy expression provides the easiest form of configurability for users, and it's something we do for all of [our first-party plugins](@/docs/guide/plugins/_index.md).

## Custom Policy Expressions

If a plugin's default policy expression is different from what you want, you can override it in your policy file using the `policy` key on any `analysis` entry, like so:

```kdl
analysis "mitre/review" \
	policy="(lte (divz (count (filter (eq #f) $)) (count $)) 0.05)"
```

In this example, we're overriding the default policy expression for the [`mitre/review`][mitre_review] plugin. To make it a little clearer, let's add some light formatting.

```
(lte
	(divz
		(count (filter (eq #f) $))
		(count $))
	0.05)
```

This analysis works by checking Pull Requests on a target's GitHub repository (if one is found) to see if they receive an approving review prior to being merged. This policy calculates the percentage of PRs which _do not_ receive an approving review, and validates that it is less than 5% of all PRs.

Note that all policy expressions are required to return a boolean result, and that policy expressions must work with the types specified in a plugin's JSON schemas. All plugins are required by Hipcheck to provide JSON schemas at run-time which are used to typecheck all policy expressions.

## Write Your Own Plugin

In cases where the data returned by a plugin is insufficient, or the logic to express for reducing it to a pass / fail is too cumbersome for a policy expression, you can instead [create your own plugin][create_plugin].

Creating a plugin means implementing the plugin gRPC protocol, and providing one or more queries which follow the query protocol. Today, we provide a [Rust SDK][rust_sdk] which implements the gRPC and query protocol logic for you. In the future we plan both to provide SDKs in other popular languages and to fully document how to implement both protocols yourself.

In the Rust SDK, you define a type which implements the `hipcheck_sdk::Plugin` type, which maps to the gRPC operations your plugin must support, and you use the `hipcheck_sdk::query` macro which takes async functions and wires them up to be usable in our query protocol.

The `Plugin` trait looks like this:

```rust
pub trait Plugin: Send + Sync + 'static
{
    const PUBLISHER: &'static str;
    const NAME: &'static str;

    // Required methods
    fn set_config(&self, config: JsonValue) -> StdResult<(), ConfigError>;
    fn default_policy_expr(&self) -> Result<String>;
    fn explain_default_query(&self) -> Result<Option<String>>;
    fn queries(&self) -> impl Iterator<Item = NamedQuery>;

    // Provided methods
    fn default_query(&self) -> Option<DynQuery> { ... }
    fn schemas(&self) -> impl Iterator<Item = QuerySchema> { ... }
}
```

For the `mitre/activity` plugin, `PUBLISHER` is `"mitre"` and `NAME` is `"activity"`. The `set_config` call accepts a JSON structure representing any configuration data provided in the user's policy file. `default_policy_expression` returns a policy expression string, if one exists. `explain_default_query` provides a human explanation of what the default query is doing, to assist in providing friendly messages post-analysis. Finally, `queries` returns an iterator over queries, and is filled using the `queries` macro, which generates an implementation by filling in the function based on use of the `query` macro to annotate functions.

An async function for a query looks like this:

```rust
async fn [FUNC_NAME](engine: &mut PluginEngine, input: [INPUT_TYPE]) -> Result<[OUTPUT_TYPE]>
```

Where `[FUNC_NAME]` is the name of the query, and where`[INPUT_TYPE]` and `[OUTPUT_TYPE]` are types implementing the `hipcheck_sdk::deps::JsonSchema` trait. For example, returning to the `mitre/activity` plugin, we have:

```rust
use hipcheck_sdk::{prelude::*, types::Target};

#[query(default)]
async fn activity(engine: &mut PluginEngine, target: Target) -> Result<String>;
```

This is a default query (annotated with `query(default)`) so by convention we make the query name match the name of the plugin itself. The `PluginEngine` is the type that enables a plugin to send queries to _other_ plugins (because plugins in Hipcheck compose), and `Target` comes from our SDK, representing our target of analysis. The final data is a string indicating the number of weeks.

We can then fill in the body of this function to implement the logic of the plugin.

The final piece of making your own plugin is distributing it.

Currently, Hipcheck does not have a plugin registry for distributing plugins. Instead, producers of plugins should make a "plugin discovery manifest" which is hosted at a known and broadly-accessible URL. This manifest specifies where to find artifacts for each version and target platform for a plugin. For example, the manifest for the `mitre/activity` plugin is hosted on the Hipcheck website, and you can see the source for it on the [Hipcheck Github repository][activity_dl_manifest].

Along with this discovery manifest, you'll also need a plugin manifest for each version of a plugin. This specifies basic metadata for a plugin, and any dependencies that plugin may have based on queries it makes to other plugins. For example, the [plugin manifest for `mitre/activity`][activity_manifest] specifies a dependency on the `mitre/git` plugin, as it needs Git metadata.

When shipping your plugin, you bundle any prebuilt binaries for your target architecture alongside the plugin manifest. This is what Hipcheck downloads when a user specifies your plugin.

For a more detailed explanation of the process of shipping plugins, check out our [plugin release guide](@/docs/guide/making-plugins/release.md)!

## Conclusion

We truly believe in Hipcheck's goal of maximum configurability. A tool is most helpful to you when it can express _your_ needs, not the needs of the tool's creators. Our intent with Hipcheck is to build a powerful analysis toolkit with strong and useful defaults of the box, but also many mechanisms to tune, replace, and configure what's in it and how it runs.

With each of the options presented here—default policy expressions, custom policy expressions, and custom plugins—you trade off greater power for more ceremony to specify the exact policies you'd like to apply. Our intent over time is to continue to smooth this ramp by making every option clearer and easier, especially plugin creation.

If you try any of these features and hit problematic edge cases, unclear instructions, or confusing error messages, please [let us know so we can fix them][get_help]!

If you're interested in contributing to Hipcheck, [we'd love to work with you][contribute]!


[values]: https://hipcheck.mitre.org/docs/rfds/0002/#product-values
[3.8.0]: https://hipcheck.mitre.org/blog/hipcheck-3-8-0-release/
[policy_file]: https://hipcheck.mitre.org/docs/guide/config/policy-file/
[target]: https://hipcheck.mitre.org/docs/guide/concepts/targets/
[policy_expr]: https://hipcheck.mitre.org/docs/guide/config/policy-expr/
[mitre_activity]: https://hipcheck.mitre.org/docs/guide/plugins/mitre-activity/
[jiff]: https://docs.rs/jiff/latest/jiff/
[mitre_review]: https://hipcheck.mitre.org/docs/guide/plugins/mitre-review/
[create_plugin]: https://hipcheck.mitre.org/docs/guide/making-plugins/creating-a-plugin/
[rust_sdk]: https://hipcheck.mitre.org/docs/guide/making-plugins/rust-sdk/
[activity_dl_manifest]: https://github.com/mitre/hipcheck/blob/main/site/static/dl/plugin/mitre/activity.kdl
[activity_manifest]: https://github.com/mitre/hipcheck/blob/main/plugins/activity/plugin.kdl
[get_help]: https://github.com/mitre/hipcheck/discussions
[contribute]: https://hipcheck.mitre.org/docs/contributing/
