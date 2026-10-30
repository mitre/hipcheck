# Architecture

This page describes Night Vision's current MVP system boundaries and the flow
that produces an upgrade assessment. The frontend, `nv-server`, and PostgreSQL
are the deployed application components. The server also runs background tasks
and invokes Hipcheck as a subprocess; those are not separately deployed services.

## Table of Contents

[[_TOC_]]

## MVP Purpose and Boundaries

Night Vision helps a user decide whether a newer NPM package version is a
reasonable lower-risk upgrade when a reachable version is associated with a
CISA Known Exploited Vulnerability (KEV). It does not prove that an upgrade is
safe or calculate an agency's BOD 26-04 deadline. The [MVP roadmap] describes
the end-to-end workstreams, and [RFD 0001] defines the assessment decision.

The current application accepts NPM `package.json` sources. SvelteKit renders
the frontend and calls the `nv-server` REST API through server-side route loads
and actions. `nv-server` owns source resolution, vulnerability correlation,
candidate assessment, and report persistence. PostgreSQL stores submitted
sources and their resolved dependency graphs, CVE and KEV data and sync state,
and assessment results and evidence.

![Night Vision MVP architecture showing the browser and SvelteKit frontend,
the REST API and in-process tasks in nv-server, PostgreSQL, the CVE List and
KEV sources, the NPM registry, and the Hipcheck subprocess.](./architecture.png)

The editable diagram is [architecture.dot](./architecture.dot). From the
repository root in the Flox environment, regenerate the image with
`dot -Tpng -Gdpi=160 docs/system/architecture.dot -o docs/system/architecture.png`.

Arrows show where data moves. The frontend does not fetch vulnerability feeds,
NPM metadata, or Hipcheck results directly; `nv-server` initiates the external
data requests shown above.

Hipcheck is an analysis engine called by Night Vision, not a public API or the
owner of the final verdict; [RFD 0002] explains that boundary.

## Assessment Data Flow

1. A user submits a `package.json` through the frontend. The REST API validates
   and stores the source as `pending` before responding. An in-process worker
   claims pending work, fetches NPM registry metadata, resolves direct and
   transitive package versions, and persists the reachable graph and source
   lifecycle state. The source can be processing or failed; neither state
   implies that it has zero exposures.
2. Independently, in-process sync tasks fetch CVE List records from a Git
   repository and CISA's KEV catalog over HTTP. They persist the data and sync
   outcomes in PostgreSQL. The API exposes dataset availability and freshness
   through `/data-status`.
3. For a completed source, the API correlates its stored reachable versions
   with locally stored CVE and KEV records when exposure results are requested.
   A KEV-linked exposure identifies the affected package version, CVE, and
   reachability path. Exposure results depend on both datasets being available;
   the MVP does not use a separate re-analysis queue to create them.
4. A user starts an upgrade assessment from a selected exposure. The API
   persists a pending assessment before starting an in-process task. Discovery
   fetches newer NPM versions and evaluates their locally known KEV status.
   When a specific candidate is requested, the task invokes `hc check` for
   that candidate, normalizes the available Hipcheck findings, and combines
   them with vulnerability evidence and caveats. Hipcheck does not run for
   every discovered version. The shipped policy has limited signal coverage,
   as described in [RFD 0002].
5. The task stores the assessment outcome and evidence in PostgreSQL. The
   frontend reads status and completed results through the REST API, then
   renders the affected version, candidates, verdict, supporting findings, and
   limits. A failed or unfinished assessment remains distinguishable from a
   completed recommendation.

Source resolution has persisted pending work and lease recovery inside
`nv-server`. Assessment execution uses a spawned task after its pending row is
stored; startup reconciliation marks interrupted assessments failed. These
mechanisms provide observable lifecycle state without a separate message
broker or worker deployment.

## Frontend Information Architecture

The frontend boundary is SvelteKit's server-side API client: route loads read
API data, and form actions submit sources or assessments. The browser receives
rendered view data rather than directly accessing PostgreSQL or external
feeds. [RFD 0006] defines the intended MVP navigation around
Assessments, Package sources, and Data status. The implemented routes and
remaining RFD direction are distinct:

| Area | Current frontend | RFD 0006 direction |
| --- | --- | --- |
| Entry point | `/` is a placeholder; the sidebar links to Package Sources and Assessments. | Make Assessments the default landing area and include Data status in navigation. |
| Package sources | `/packagesources`, `/packagesources/new`, and `/packagesources/[sourceId]` submit sources, show lifecycle state, and show KEV-linked exposures. | `/sources`, `/sources/new`, and `/sources/:sourceId` organize the same source journey. |
| Assessments | `/assessments` lists assessments remembered by the current browser; `/assessments/[assessmentId]` reads status and completed results from the API. | Make Assessments the default exposure work queue and connect source, exposure, candidate, evidence, and verdict in the detail flow. |
| Data status | The REST API provides `/data-status`; there is no frontend data-status route yet. | `/data-status` explains CVE and KEV freshness, failures, and assessment impact. |

This page identifies routes and data ownership only. [RFD 0006] contains the
page-level interaction and presentation decisions; the [REST API documentation]
contains endpoint contracts.

## Post-MVP Scaling

The current background work runs within `nv-server`, with concurrency and
durable state managed there. This keeps the MVP deployment small, but server
capacity bounds the work it can perform. If load or reliability requirements
justify it later, dedicated queues and separately deployed workers could
distribute source resolution, synchronization, or assessment execution. That
infrastructure is not part of the current architecture, and no queue product
or worker topology is selected here.

## Design References

- [MVP roadmap] describes the product flow and workstreams.
- [RFD 0001] defines the upgrade-safety assessment goal and verdicts.
- [RFD 0002] defines Night Vision's use of Hipcheck and its limited MVP policy.
- [RFD 0006] defines the intended frontend information architecture.

[MVP roadmap]: ../project/mvp-roadmap.md
[RFD 0001]: ../rfds/0001-mvp-upgrade-safety-assessments.md
[RFD 0002]: ../rfds/0002-use-hipcheck-for-supply-chain-analysis.md
[RFD 0006]: ../rfds/0006-mvp-frontend-assessment-flow.md
[REST API documentation]: ../backend/rest-api-usage.md
