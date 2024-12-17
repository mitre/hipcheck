---
title: Developer Docs
template: docs.html
sort_by: weight
page_template: docs_page.html
weight: 3
---

# Hipcheck Developer Docs

<div class="grid grid-cols-2 gap-8 mt-8">

{% waypoint(title="Architecture", path="@/docs/contributing/developer-docs/architecture.md") %}
Hipcheck's distributed architecture and how plugins get started.
{% end %}

{% waypoint(title="Query System", path="@/docs/contributing/developer-docs/plugin-query-system.md") %}
The life of a plugin query from inception, through gRPC, to SDK, and back.
{% end %}

</div>
