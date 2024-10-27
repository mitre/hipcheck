---
title: hc scoring
extra:
  nav_title: "<code>hc scoring</code>"
---

# `hc scoring`

Hipcheck's scoring system works by calculating percentages for how much each
analysis in the user's configured analysis tree contributes to the overall
score, based on weights users set for each analysis and category.

The `hc scoring` command takes that configured tree and weights, calculates
scoring percentages, and displays them to the user to make it clear how their
current policies will be converted to scores based on the results of a run
of analyses.

The help text looks like:

```
Print the tree used to weight analyses during scoring

Usage: hc scoring [OPTIONS]

Options:
  -h, --help  Print help (see more with '--help')

Output Flags:
  -v, --verbosity <VERBOSITY>  How verbose to be [possible values: quiet, normal]
  -k, --color <COLOR>          When to use color [possible values: always, never, auto]
  -f, --format <FORMAT>        What format to use [possible values: json, human]

Path Flags:
  -C, --cache <CACHE>    Path to the cache folder
  -p, --policy <POLICY>  Path to the policy file
```

The following is an example output:

```
risk
|-- practices
|   |-- mitre::activity: 10.00%
|   |-- mitre::binary: 10.00%
|   |-- mitre::fuzz: 10.00%
|   |-- mitre::identity: 10.00%
|   `-- mitre::review: 10.00%
`-- attacks
    |-- mitre::typo: 25.00%
    `-- commit
        |-- mitre::affiliation: 8.33%
        |-- mitre::churn: 8.33%
        `-- mitre::entropy: 8.33%
```
