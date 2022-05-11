
# Architecture Overview

Hipcheck is built as a pipeline, which takes in configuration, prepares the
repository to be analyzed, prepares the output directory in which results will be
placed, and then runs its analyses.

The analyses are supported by a data provider, to which the analyses make requests
for data. This provider caches data, and the analyses work with references to
the cached data, to reduce duplication. When a request is made and the cache
has not yet been filled, the provider will perform the appropriate action to get
the relevant data.

The analyses' results are then handed off for scoring, and then output.

This is all facilitated by a query system, powered by Salsa, a Rust crate for
incremental computation.

## Why a Query Architecture?

Note: this section was written prior to the implementation of the query architecture.

### Inter-Phase Dependencies in a Pipeline Architecture

One of the challenges of Hipcheck's current architecture is that it needs to be known, at each
step, what processes to run. Hipcheck supports a wide variety of analyses, all of which may be
turned off by the user. These analyses also have differing dependencies on the underlying data
sourced from Hipcheck's data providers. In the pipelined architecture, this means that Hipcheck
needs to know 1) what the user's configuration is, 2) what analyses are required
to be run, 3) what underlying data those analyses require, all at the beginning of a run. The
tracking of these dependencies needs to be done manually, or else data required at a later phase
of the pipeline won't have been produced by an earlier phase.

While Hipcheck had few forms of analysis, this structure was tenable, but the intent for Hipcheck
is for the number and complexity of analyses to grow substantially. We expect that the management
of these phase-dependencies would become a likely source of complexity and defects in Hipcheck.

A query system, by comparison, does not require explicit manual tracking of dependencies between
phases. Rather, because it runs "end-first" (initiated by a query for the final output: the report),
each phase can function by querying for the data it requires. In the query system, if that data has
already been requested prior in a run (or in state restored from a prior run) it will be reused
without recomputation, if the data has _not_ been collected already, it will be computed lazily when
needed and cached for reuse.

### Caching Intermediate Computations

Hidden in this explanation are a couple of additional benefits of the query system. First, the
elimination of the need for manual, bespoke caching solutions for intermediate computations. Currently,
key analysis results are stored in Hipcheck's `Provider` type (which we expect to sunset), and manual
methods are provided to access them. Already, as the data stored in the `Provider` has become more
complex, and more interdependent, the number of unique (and fairly long / error prone) methods to
cache and access that data has increased substantially. With a query-based system which handles caching
transparently, there's no need for these bespoke caching solutions.

### Resumption

The other major benefit of this new architecture is _resumption_. Prior designs of
Hipcheck imagined is as a single-run system, used largely to analyze never-seen-before open source
software. Used in this manner, there's not much benefit to storing intermediate data on-disk and
restoring from it in future runs. However, Hipcheck's use cases have grown to encompass internal development,
where support for incremental analysis is key for performance. Additionally, currently a failed
Hipcheck run does not store intermediate computations determined successfully prior to an error. In the
case of more "expensive" analyses, this means that a late-stage failure in Hipcheck can result in substantial
time-wastage due to recomputation.

Transitioning to a query-based architecture which aggressively caches intermediate data, automatically
handles cache invalidation and cycle detection, and centralizes cached data, will make it easier to
implement support for resumption within Hipcheck, both in cases of failure (to retain results calculated
prior to the failure) and cases of re-analysis.

## The "Layers" of Hipcheck

Even in this query architecture, it's useful to conceptually split out "layers" of queries which Hipcheck
may make. In each layer, the expectation is that queries may make their own queries to layers below their
own, but don't make queries "going up" or laterally within their layer. This helps ensure freedom from query
cycles (which would be a source of bugs / failures for Hipcheck).

The planned layers are (from "top" to "bottom"):

- [Reporting](./reporting.md): outputting risk reports on targets of analysis.
- [Scoring](./scoring.md): combining analysis outcomes with weights to produce a final risk score.
- [Analyses](./analyses.md): combining metrics with user-configured thresholds to produce a pass/fail outcome.
- [Metrics](./metrics.md): producing data derived from the raw data.
- [Data](./data.md): original data about the target of analysis, taken from external data providers.
- [Sources](./sources.md): the target of analysis.
- [Configuration](./configuration.md): the user's preferred setup of Hipcheck.
