---
title: Plugins
template: docs.html
page_template: docs_page.html
weight: 5
sort_by: title
---

# Plugins

This section covers the plugins currently produced by MITRE.

<div class="grid grid-cols-2 gap-8 mt-8">

{% waypoint(title="mitre/activity", path="@/docs/guide/plugins/mitre-activity.md", icon="box") %}
Plugin for checking whether a project is actively maintained.
{% end %}

{% waypoint(title="mitre/affiliation", path="@/docs/guide/plugins/mitre-affiliation.md", icon="box") %}
Plugin for detecting contributors affiliated with an organization of concern.
{% end %}

{% waypoint(title="mitre/binary", path="@/docs/guide/plugins/mitre-binary.md", icon="box") %}
Plugin for detecting binaries checked into source repositories.
{% end %}

{% waypoint(title="mitre/churn", path="@/docs/guide/plugins/mitre-churn.md", icon="box") %}
Plugin for detecting unusually large changes in a project's history.
{% end %}

{% waypoint(title="mitre/entropy", path="@/docs/guide/plugins/mitre-entropy.md", icon="box") %}
Plugin for detecting textually unusual changes in a project's history.
{% end %}

{% waypoint(title="mitre/fuzz", path="@/docs/guide/plugins/mitre-fuzz.md", icon="box") %}
Plugin for checking if a project uses fuzz testing.
{% end %}

{% waypoint(title="mitre/git", path="@/docs/guide/plugins/mitre-git.md", icon="git-pull-request") %}
Plugin for accessing Git commit history data.
{% end %}

{% waypoint(title="mitre/github", path="@/docs/guide/plugins/mitre-github.md", icon="github") %}
Plugin for accessing data from the GitHub API.
{% end %}

{% waypoint(title="mitre/identity", path="@/docs/guide/plugins/mitre-identity.md", icon="box") %}
Plugin for accessing Git contributor identity data.
{% end %}

{% waypoint(title="mitre/linguist", path="@/docs/guide/plugins/mitre-linguist.md", icon="box") %}
Plugin for detecting text file language data.
{% end %}

{% waypoint(title="mitre/npm", path="@/docs/guide/plugins/mitre-npm.md", icon="box") %}
Plugin for accessing package data from the NPM API.
{% end %}

{% waypoint(title="mitre/review", path="@/docs/guide/plugins/mitre-review.md", icon="box") %}
Plugin for checking if a project practices code review.
{% end %}

{% waypoint(title="mitre/typo", path="@/docs/guide/plugins/mitre-typo.md", icon="box") %}
Plugin for detecting possible typosquatting in dependencies.
{% end %}

</div>
