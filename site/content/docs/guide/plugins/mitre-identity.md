---
title: "mitre/identity"
extra:
  nav_title: "<code>mitre/identity</code>"
---

# `mitre/identity`

Checks if a project's contributions have the same author and committer.

## Configuration

| Parameter           | Type    | Explanation   |
|:--------------------|:--------|:--------------|
| `percent-threshold` | `Float` | Percentage of "merge-by-self" contributions to accept. |

## Default Policy Expression

```
(lte
  (divz
    (count (filter (eq #t) $))
    (count $))
  {config.percent-threshold or 0.2})
```

## Default Query: `mitre/identity`

Returns an array of booleans indicating if commits have matching committer
and author.

## Explanation

Identity analysis looks at whether the author and committer identities for
each commit are the same, as part of gauging the likelihood that commits
are receiving some degree of review before being merged into a repository.

When author and committer identity are the same, that may indicate that a
commit did _not_ receive review, which could be a cause for concern. At the
larger level, having a large percentage of commits with the same author
and committer identities may indicate a project that lacks code review.

## Limitations

* __Not every project uses a workflow that accords with this analysis__:
  While some Git projects may use a workflow that involves the generation
  of patchfiles to then be reviewed and applied by project maintainers,
  many may not. In some cases, a workflow may produce final commits where
  the author and committer identity are the same, even though the commit
  received review.
