---
title: "mitre/git"
extra:
  nav_title: "<code>mitre/git</code>"
---

# `mitre/git`

Provides access to Git commit history data. Does not define a default query
and can't be used as a top-level plugin in a policy file.

## Configuration

| Parameter           | Type    | Explanation   |
|:--------------------|:--------|:--------------|
| `commit-cache-size` | `Integer` | Optional number of repositories to retain in cache. Defaults to one. |
