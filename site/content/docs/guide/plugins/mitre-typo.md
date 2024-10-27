---
title: "mitre/typo"
extra:
  nav_title: "<code>mitre/typo</code>"
---

# `mitre/typo`

Identifies possible typosquatted dependencies. Currently only supports NPM
packages.

## Configuration

| Parameter         | Type      | Explanation   |
|:------------------|:----------|:--------------|
| `typo-file-path`  | `String`  | Path to file specifying how to match for typos. |
| `count-threshold` | `Integer` | How many possible-typo dependencies to permit. |

## Default Policy Expression

```
(lte
  (count (filter (eq #t) $))
  {config.count-threshold or 0})
```

## Default Query: `mitre/typo`

Checks for possible typosquatted dependencies in a package's list of
dependencies; returns an array of booleans indicating whether each dependency
is a possible typosquatted dependency.

## Explanation

Typo analysis attempts to identify possible typosquatting attacks in the
dependency list for any projects which are analyzed and use a supported
language (currently: JavaScript w/ the NPM package manager).

The analysis works by identifying a programming language based on the presence
of a dependency file in the root of the repository, then attempting to get the
full list of direct and transitive dependencies for that project. It then
compares that list against a list of known popular repositories for that
language to see if any in the dependencies list are possible typos of popular
package name.

Typo detection is based on the generation of possible typos for known names,
according to a collection of typo possibilities, including single-character
deletion, substitution, swapping, and more.

## Limitations

* __Only works for some languages__: Right now, this analysis only supports
  JavaScript projects. It requires the implementation of language-specific code
  to work with different dependency files and generate the full list of
  dependencies, and requires legwork to produce the list of popular package
  names, which are not currently pulled from any external API or authoritative
  source.
