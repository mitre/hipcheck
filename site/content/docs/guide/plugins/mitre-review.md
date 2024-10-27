---
title: "mitre/review"
extra:
  nav_title: "<code>mitre/review</code>"
---

# `mitre/review`

Checks if GitHub PRs receive an approving review prior to merge.

## Configuration

| Parameter           | Type    | Explanation   |
|:--------------------|:--------|:--------------|
| `percent-threshold` | `Float` | Percentage of merged PRs without a review which is permissible. |

## Default Policy Expression

```
(lte
  (divz
    (count (filter (eq #f) $))
    (count $))
  {config.percent-threshold or 0.05})
```

## Default Query: `mitre/review`

Returns an array of booleans, indicating true for each PR if an approving review
was received.

## Explanation

Review analysis looks at whether pull requests on GitHub (currently the
only supported remote host for this analysis) receive at least one
review prior to being merged.

If too few pull requests receive review prior to merging, then this
analysis will flag that as a supply chain risk.

This works with the GitHub API, and requires a token in the configuration.
Hipcheck only needs permissions for accessing public repository data, so
those  are the only permissions to assign to your generated token.

## Limitations

* __Not every project uses GitHub__: While GitHub is a very popular host
  for Git repositories, it is by no means the _only_ host. This analysis'
  current limitation to GitHub makes it less useful than it could be.
* __Projects which do use GitHub may not use GitHub Reviews for code review__:
  GitHub Reviews is a specific GitHub feature for performing code reviews
  which projects may not all use. There may be repositories which are older
  than the availability of this feature, and so don't have reviews on older
  pull requests.
