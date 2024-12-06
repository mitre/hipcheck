---
title: Key Concepts
template: docs.html
page_template: docs_page.html
sort_by: weight
weight: 1
---

# Key Concepts

To understand Hipcheck, it's useful to understand some of the key concepts
underlying its design, which we'll explore here.


<div class="grid grid-cols-2 gap-8 mt-8">

{% waypoint(title="Targets", path="@/docs/guide/concepts/targets.md", icon="target") %}
How Hipcheck identifies what package or project to analyze.
{% end %}

{% waypoint(title="Data", path="@/docs/guide/concepts/data/index.md", icon="database") %}
How Hipcheck collects data from external sources.
{% end %}

{% waypoint(title="Analyses", path="@/docs/guide/concepts/analyses.md", icon="alert-triangle") %}
What kinds of analyses Hipcheck is focused on.
{% end %}

{% waypoint(title="Scoring", path="@/docs/guide/concepts/scoring/index.md", icon="activity") %}
How Hipcheck converts individual analysis results into a risk score.
{% end %}

{% waypoint(title="Concerns", path="@/docs/guide/concepts/concerns.md", icon="list") %}
How plugins report extra information to support manual analysis.
{% end %}

</div>
