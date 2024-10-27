---
title: Concerns
weight: 6
---

# Concerns

Besides a risk score and a recommendation, Hipcheck's other major output
is a list of "concerns" from individual analyses. "Concerns" are Hipcheck's
mechanism for analyses to report specific information about what they found
that they think the user may be interested in knowing and possibly
investigating further. For example, some of Hipcheck's analyses that work
on individual commits will produce the hashes of commits they find concerning,
so users can audit those commits by hand if they want to do so.

Concerns are the most flexible mechanism Hipcheck has, as they are essentially
a way for analyses to report freeform text out to the user. They do not have
a specific structured format, and they are not considered at all for the
purpose of scoring. The specific concerns which may be reported vary from
analysis to analysis.

In general, we want analyses to report concerns wherever possible. For
some analyses, there may not be a reasonable type of concern to report;
for example, the "activity" analysis checks the date of the most recent
commit to a project to see if the project appears "active," and the
_only_ fact that it's assessing is also the fact which results in the
measure the analysis produces, so there's not anything sensible for
the analysis to report as a concern.

However, many analysis _do_ have meaningful concerns they can report, and if an
analysis _could_ report a type of concern but _doesn't_, we consider that
something worth changing. Contributions to make Hipcheck report more concerns,
or to make existing concerns more meaningful, are [always appreciated](@/docs/contributing/_index.md)!
