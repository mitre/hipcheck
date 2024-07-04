---
title: Configuration
---

# Configuration

Hipcheck's configuration is used to describe:

1. What analyses to run.
2. How to run those analyses.
3. How to weight the analyses when scoring.
4. How to turn the overall risk score into a recommendation.

This section describes the overall structure and some of the repeated
configuration keys found in the Hipcheck configuration files. For guidance
on how to configure individual analyses, we recommend reading the
[Analyses](@/docs/guide/analyses.md) documentation.

## What Analyses to Run

All analyses in Hipcheck can be turned off. Every grouping of analyses,
and every individual analyses, has an `active` key which can be `true`
or `false`. If `true`, the group or analysis is active and will be run,
and its results will be part of scoring. If `false`, the group or analysis
will _not_ be run, and it will have no results.

Analyses can also be set to run with `active = true`, but have the weight
of their results set to `0`. In this case, the analysis will be run, and
any [Concerns](@/docs/guide/concepts/index.md#concerns) it identifies will
be reported, but it will be ignored for the purpose of scoring.

## Configuring Individual Analyses

Individual analyses each have their own configuration which is specific to
them. For full details on this, see the [Analyses](@/docs/guide/analyses.md)
documentation. Several analyses define their own additional TOML files which
contain more complex configuration.

## Weighting Analyses for Scoring

All analyses have an associated weight which can be modified to change how
the results of the analysis are considered for scoring. The full details
of the scoring algorithm can be found in the [Scoring](@/docs/guide/concepts/index.md#scoring)
documentation. To configure the weight of an analysis or analysis group,
use the `weight` key. This is expected to be a non-negative integer.

By default, all weights are equal to `1`.

## Setting a Risk Tolerance

The overall risk tolerance determines whether the risk scores calculated
from the results of individual analyses result in a final recommendation of
"PASS" or "INVESTIGATE." The risk tolerance is set with the key
`risk.tolerance`, and must be a floating-point value between 0 and 1,
inclusive.

Note that risks less than or equal to the tolerance will result in a "PASS"
recommendation. If you want a risk score of `0.5`, for example, to result
in an "INVESTIGATE" recommendation, the risk tolerance must be set to less
than `0.5`.

{{ button(link="@/docs/guide/debugging.md", text="Debugging") }}
