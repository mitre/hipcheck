---
title: CLI Reference
template: docs.html
page_template: docs_page.html
sort_by: slug
weight: 3
---

# CLI Reference

If you are interested in a quick guide to getting started with Hipcheck,
we recommend checking out the [Getting Started guide](@/docs/getting-started/_index.md)
first! For a more thorough explanation of Hipcheck's Command Line Interface
(CLI), please continue with this section!

<div class="grid grid-cols-2 gap-8 mt-8">

{% waypoint(title="General Flags", path="@/docs/guide/cli/general-flags.md", icon="flag") %}
Flags which apply to all Hipcheck subcommands.
{% end %}

{% waypoint(title="hc cache", path="@/docs/guide/cli/hc-cache.md", icon="database", mono=true) %}
Inspect and control Hipcheck's local data cache.
{% end %}

{% waypoint(title="hc check", path="@/docs/guide/cli/hc-check.md", icon="check", mono=true) %}
Run analyses against specified targets.
{% end %}

{% waypoint(title="hc ready", path="@/docs/guide/cli/hc-ready.md", icon="loader", mono=true) %}
Check if Hipcheck is ready to run.
{% end %}

{% waypoint(title="hc schema", path="@/docs/guide/cli/hc-schema.md", icon="hash", mono=true) %}
Get a JSON schema for Hipcheck's JSON output.
{% end %}

{% waypoint(title="hc scoring", path="@/docs/guide/cli/hc-scoring.md", icon="star", mono=true) %}
Get a visualization of Hipcheck's scoring tree based on your policy.
{% end %}

{% waypoint(title="hc setup", path="@/docs/guide/cli/hc-setup.md", icon="briefcase", mono=true) %}
Complete post-installation setup of Hipcheck.
{% end %}

{% waypoint(title="hc update", path="@/docs/guide/cli/hc-update.md", icon="refresh-ccw", mono=true) %}
Have Hipcheck update itself.
{% end %}

</div>
