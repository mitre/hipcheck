---
title: Complete Guide
weight: 2
template: docs.html
page_template: docs_page.html
sort_by: weight
---

# Complete Guide

Welcome to the Complete Guide to Hipcheck! This guide is intended to explain:

- What Hipcheck is
- How to use Hipcheck effectively
- The key concepts underlying Hipcheck's design
- How to configure Hipcheck
- How to interpret Hipcheck's output
- How to debug Hipcheck if you encounter an error

Since we intend this to be a __complete__ guide, if you encounter questions
you don't feel are adequately answered by this guide, please let us know
by opening an issue on our [issue tracker](https://github.com/mitre/hipcheck/issues)!

<div class="grid grid-cols-2 gap-8 mt-8">

{% waypoint(title="Key Concepts", path="@/docs/guide/concepts/_index.md", icon="key") %}
An explanation of the ideas underpinning Hipcheck's design.
{% end %}

{% waypoint(title="Configuration", path="@/docs/guide/config/_index.md", icon="settings") %}
How to configure Hipcheck and describe your policies to apply.
{% end %}

{% waypoint(title="CLI Reference", path="@/docs/guide/cli/_index.md", icon="terminal") %}
Reference for all CLI commands and arguments.
{% end %}

{% waypoint(title="Debugging", path="@/docs/guide/debugging/_index.md", icon="target") %}
How to identify errors during Hipcheck execution.
{% end %}

{% waypoint(title="Plugins", path="@/docs/guide/plugins/_index.md", icon="box") %}
Index of existing plugins for Hipcheck, both for data and analyses.
{% end %}

{% waypoint(title="Making Plugins", path="@/docs/guide/making-plugins/_index.md", icon="tool") %}
A guide for making new Hipcheck plugins.
{% end %}

</div>
