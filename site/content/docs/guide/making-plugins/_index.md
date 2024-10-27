---
title: Making Plugins
weight: 6
template: docs.html
page_template: docs_page.html
sort_by: weight
---

# Making Plugins

The following is a guide for making plugins for Hipcheck. Plugins can add new
data source and new analyses, and can be written using an SDK or by hand. The
rest of this section details the protocols plugins are expected to follow.

<div class="grid grid-cols-2 gap-8 mt-8">

{% waypoint(title="Creating a Plugin", path="@/docs/guide/making-plugins/creating-a-plugin.md", icon="box") %}
How to start making a new Hipcheck plugin.
{% end %}


{% waypoint(title="The Rust Plugin SDK", path="@/docs/guide/making-plugins/rust-sdk.md", icon="tool") %}
How to use the Rust SDK to create a plugin.
{% end %}

</div>
