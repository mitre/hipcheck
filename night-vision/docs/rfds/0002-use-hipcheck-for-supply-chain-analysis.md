# RFD 0002: Use Hipcheck for Supply Chain Analysis

## Status

Accepted

## Date

2026-07-02

## Table of Contents

[[_TOC_]]

## Summary

Night Vision should stop treating supply chain analysis as a native Night
Vision subsystem. Instead, Night Vision should use Hipcheck as the analysis
engine for supply chain risk signals, and should translate Hipcheck results
into the upgrade safety assessments described in
[RFD 0001](./0001-mvp-upgrade-safety-assessments.md).

Where Night Vision needs risk signals that Hipcheck does not yet provide, the
Night Vision team should build new Hipcheck plugins rather than equivalent
analyzers inside Night Vision.

## Background

RFD 0001 reoriented Night Vision around upgrade safety assessment. In that
model, supply chain risk signals affect whether a candidate upgrade is
`recommended`, `caution`, `avoid`, or `unknown`. That RFD intentionally left
open how those supply chain signals would be generated.

After discussion with our government sponsor, we should use Hipcheck for this
part of the system. Hipcheck already models supply chain analysis as a set of
policy-driven plugin measurements. Its core starts plugins as child processes,
communicates with them over gRPC, caches plugin query results, evaluates plugin
outputs against policy expressions, and reports a final `PASS` or
`INVESTIGATE` recommendation for each target.

This division lets Night Vision focus on upgrade assessment while reusing and
extending an existing analysis platform built for open source supply chain
risk.

## Goals

- Use Hipcheck as Night Vision's supply chain analysis engine.
- Avoid building a second plugin framework or duplicate supply chain analyzer
  inside Night Vision.
- Preserve Night Vision's assessment-oriented product model and cautious
  verdict language.
- Store enough Hipcheck evidence for users to understand why a candidate
  version received its Night Vision verdict.
- Build new analysis capabilities as Hipcheck plugins when existing Hipcheck
  plugins do not cover Night Vision's upgrade assessment needs.
- Keep the MVP integration simple enough to run inside the existing Night
  Vision backend architecture.

## Non-Goals

- Replacing Night Vision's vulnerability, KEV, package-source, or upgrade
  candidate logic with Hipcheck.
- Making Hipcheck responsible for Night Vision's user accounts, saved
  assessments, API, UI, or domain model.
- Treating Hipcheck's final recommendation as the whole Night Vision verdict.
- Running Hipcheck on every dependency in every submitted package source during
  the MVP.
- Designing a production-scale distributed analysis service before the MVP
  proves the workflow.

## Decision Boundary

Night Vision should use Hipcheck as an analysis engine, not expose it as the
product interface. Users should see a Night Vision upgrade assessment with
supply chain findings inside it, not a raw Hipcheck report unless they
explicitly inspect evidence.

Night Vision owns the upgrade assessment workflow: verdicts, persistence, API,
UI, Hipcheck invocation, normalized assessment input types, failure mapping, and
the policy-to-verdict mapping. Those pieces define product behavior and should
remain in the Night Vision repository.

Hipcheck owns the reusable analysis platform: plugin execution, policy
evaluation, cache behavior, target handling, general-purpose data plugins, and
machine-readable report fields. Night Vision should depend on that behavior
through a clear `hc` invocation boundary.

While the MVP product shape is still changing, Night Vision-specific Hipcheck
plugins may live in the Night Vision repository. They should follow Hipcheck
conventions and can move later if they become generally useful.

## MVP Integration Shape

For the MVP, Night Vision should integrate with Hipcheck by invoking `hc check`
from the backend and reading JSON output. This keeps the integration
process-local and avoids introducing a new deployed service before the workflow
is proven.

Night Vision should run Hipcheck for the candidate version being considered.
For higher-confidence comparisons, it may also run Hipcheck for the currently
reachable vulnerable version and compare normalized findings across the two
results.

This RFD does not require Night Vision to run Hipcheck across every transitive
dependency for the MVP. Hipcheck's SBOM support currently analyzes the root
package described by an SBOM, not each dependency in the SBOM. Night Vision can
still resolve and reason about dependency deltas itself, then use Hipcheck for
focused analysis of the package versions that matter to the upgrade decision.

Night Vision should ship one predetermined Hipcheck policy file as part of its
backend configuration. That policy should select the Hipcheck plugins that
matter for upgrade assessment and group their results into Night Vision finding
categories.

Hipcheck's score and `PASS` or `INVESTIGATE` recommendation should be treated
as one input to the Night Vision verdict. Night Vision should preserve the RFD
0001 verdicts and normalize Hipcheck output into findings used by the verdict
engine.

Night Vision should retain the raw Hipcheck JSON output for auditability,
debugging, and future reprocessing. Moving Hipcheck work behind a queue or
worker can happen later if request latency requires it; it is not an MVP
prerequisite.

## Hipcheck Version and Reporting Strategy

Night Vision pins Hipcheck `main` commit
[`06a3db9394742a58a7fb3412b677db25feb6f678`](https://github.com/mitre/hipcheck/commit/06a3db9394742a58a7fb3412b677db25feb6f678)
for the MVP integration. The pinned source reports version `3.15.0` and is
built with its locked dependency set. The artifact layout and policy are
defined in [`backend/hipcheck/README.md`](../../backend/hipcheck/README.md).
Night Vision must not rely on a floating local Hipcheck installation or a
floating plugin manifest.

Because Hipcheck and Night Vision are both MITRE-maintained projects, the teams
have room to add upstream Hipcheck report fields when Night Vision needs better
machine-readable output. Night Vision should not treat the current Hipcheck
JSON report as a fixed external contract if it lacks the fields needed for
upgrade assessment.

Changes to Hipcheck reporting should be made in the Hipcheck repository,
reviewed there, and consumed by Night Vision from the selected Hipcheck
revision.
The Night Vision repository should contain only the integration boundary,
fixtures, normalized types, and tests needed to consume the agreed report shape.

## Implementation Guidance

Night Vision should define the smallest Hipcheck JSON consumer contract it needs
to normalize supply chain findings. The backend should tolerate missing
optional fields by recording missing evidence, but should fail the Hipcheck
portion of the assessment when required identity fields, result state, or
parseable structured output are missing.

When Night Vision needs a supply chain signal that Hipcheck does not already
provide, the Night Vision team should implement it as a Hipcheck plugin instead
of building an equivalent analyzer directly into Night Vision.

The detailed backend consumer contract, failure mapping, security boundary,
deployment artifact list, and plugin plan are defined in
[Hipcheck Integration](../backend/hipcheck-integration.md).

## Consequences

This shift reduces duplicate engineering work and aligns Night Vision with a
tool already built around supply chain analysis. It also changes the main
technical risk: instead of building analyzers from scratch, Night Vision must
make Hipcheck invocation reliable, parseable, and explainable inside an upgrade
assessment product.

The team should expect some Hipcheck work to be necessary because existing
plugins will not cover every package-version comparison Night Vision wants.
