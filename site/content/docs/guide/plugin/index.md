---
title: Plugins
---

# Introduction

After Hipcheck resolves a user's desired analysis target, it moves to the main
analysis phase. This involves Hipcheck passing the target description to a set of
user-specified, top-level analyses which measure some aspect of the target and
produce a pass/fail result. These tertiary data sources often rely on
lower-level measurements about the target to produce their results.

To facilitate the integration of third-party data sources and analysis
techniques into Hipcheck's analysis phase, data sources are split out into
plugins that Hipcheck can query. In order to produce their result, plugins can
in turn query information from other plugins, which Hipcheck performs on their
behalf.

The remainder of this section of the documentation is split in two. The [first
section](for-users) is aimed at users. It covers how they can specify analysis
plugins and control the use of their data in producing a pass/fail determination
for a given target. The [second section](for-developers) is aimed at plugin
developers, and explains how to create and distribute your own plugin.


## Table of Contents

- [Using Plugins](@/docs/guide/plugin/for-users.md)
- [Developing Plugins](@/docs/guide/plugin/for-developers.md)
- [Policy Expressions](@/docs/guide/plugin/policy-expr.md)
