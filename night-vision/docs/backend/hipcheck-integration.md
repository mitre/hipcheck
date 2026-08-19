# Hipcheck Integration

Night Vision consumes Hipcheck output as supply chain evidence for upgrade
assessments. This document defines the backend-facing Hipcheck JSON contract
and tracks expected Night Vision-driven Hipcheck plugin work for the MVP.

## JSON Contract

Night Vision defines the consumer-side contract, normalized types, fixtures,
and compatibility tests. Hipcheck provides the upstream report fields that
satisfy this contract. The contract should be implemented through intentional
Hipcheck report fields, not by scraping display-oriented text.

### Top-Level Fields

For the MVP, each Hipcheck JSON report should expose these top-level fields:

| Field | Type | Expected contents |
| --- | --- | --- |
| `schema_version` | String | Version of the Hipcheck JSON report schema, such as `1`. |
| `hipcheck.version` | String | Hipcheck build version, such as `4.0.0-pre`. |
| `hipcheck.commit` | String | Git commit used to build the `hc` binary. |
| `target.kind` | String | Hipcheck target kind, such as `npm`, `repository`, or `spdx`. |
| `target.purl` | String or null | Package URL (PURL) for package targets, such as `pkg:npm/%40scope/name@1.2.7`. For NPM package targets, this field is required and must include the package version. |
| `target.source_repository_url` | String | Resolved source repository URL. Hipcheck requires resolving a source repository URL before it can run, so this field must always be present. |
| `policy.id` | String | Stable policy identifier used by Night Vision, such as `night-vision-upgrade-assessment`. |
| `policy.version` | String or null | Policy version or policy file content hash. |
| `policy.source` | String | Policy file path, URI, or bundled policy name. |
| `policy.recommendation` | String | Hipcheck's overall recommendation, such as `PASS` or `INVESTIGATE`. |
| `checks` | Array of objects | Per-plugin check results. |

### Check Fields

Each object in `checks` should expose the following fields. A check is
identified by its plugin identity and query:

| Field | Type | Expected contents |
| --- | --- | --- |
| `plugin.name` | String | Hipcheck plugin name. |
| `plugin.publisher` | String | Plugin publisher, such as `mitre`. |
| `plugin.version` | String | Plugin version string. |
| `plugin.query` | String | Query endpoint or query name used for the check. |
| `policy.expression` | String | Policy expression evaluated against the plugin output. |
| `state` | String | One of `passed`, `failed`, `skipped`, `unsupported`, or `errored`. |
| `effect` | String | Night Vision recommendation effect: `blocking`, `review`, `context`, or `missing-check`. |
| `severity` | String or null | Optional severity such as `low`, `medium`, `high`, or `critical`. |
| `summary` | String | Short machine-readable result summary suitable for normalization. |
| `value` | Object, array, string, number, boolean, or null | Structured measured value returned by the plugin. |
| `concerns` | Array of objects | Concerns reported by the Hipcheck plugin during execution. |
| `started_at` | String or null | RFC 3339 timestamp for when the check started. |
| `ended_at` | String or null | RFC 3339 timestamp for when the check ended. |
| `error` | Object or null | Error details for `skipped`, `unsupported`, or `errored` checks. |

### Concern Fields

Each object in `concerns` should expose:

| Field | Type | Expected contents |
| --- | --- | --- |
| `kind` | String | Concern kind reported by the plugin, such as `warning`, `missing-data`, `unsupported-target`, or `policy-context`. |
| `message` | String | Short concern message safe to store and show to maintainers. |
| `details` | Object or null | Structured concern details supplied by the plugin. |

### Error Fields

Each `error` object should expose:

| Field | Type | Expected contents |
| --- | --- | --- |
| `kind` | String | Error class, such as `target-resolution`, `network`, `timeout`, `plugin`, `policy`, or `parse`. |
| `message` | String | Short diagnostic safe to store and show to maintainers. |
| `retryable` | Boolean | Whether rerunning the same check may succeed without code or policy changes. |

### Missing Fields

Night Vision should tolerate missing optional fields by recording a
missing-evidence finding rather than failing the whole assessment when the rest
of the report is usable.

Night Vision should fail the Hipcheck portion of the assessment when required
identity fields, result state, or parseable structured output are missing.

## Integration Requirements

The Night Vision backend should add a clear boundary around Hipcheck execution.
That boundary should handle:

- building command arguments without shell interpolation;
- choosing policy, cache, config, and exec paths;
- enforcing timeouts and output-size limits;
- collecting stdout, stderr, exit status, and structured JSON;
- mapping Hipcheck failures into Night Vision assessment errors;
- preserving enough raw data for audit and debugging;
- normalizing Hipcheck findings into Night Vision assessment records.

### Failure Mapping

The boundary should map common Hipcheck failure modes into predictable Night
Vision behavior:

| Failure mode | Night Vision behavior | Verdict impact |
| --- | --- | --- |
| `hc` binary is missing, cannot start, or exits before producing output. | Record a Hipcheck execution error with stdout, stderr, exit status, and configured binary path when safe to show. | Use `unknown` unless non-Hipcheck evidence is enough to make a stricter verdict. |
| Hipcheck times out. | Stop the process, mark the Hipcheck run as retryable, and preserve partial diagnostics without trusting incomplete JSON. | Use `unknown` for the supply chain portion; the overall verdict may be `unknown` or stricter based on vulnerability evidence. |
| Hipcheck cannot resolve the package target or source repository. | Record a target-resolution error tied to the package name and version. | Use `unknown` unless Night Vision has another blocking finding. |
| Hipcheck reports that a target or check is unsupported. | Normalize the result as a missing-check finding instead of treating the whole assessment as failed. | Reduce evidence quality; use `caution` or `unknown` when the unsupported check is important to the policy. |
| A plugin errors while other checks complete. | Preserve successful check results and normalize the failed plugin result with its error kind and retryability. | Use the completed findings; add `caution` or `unknown` when the failed plugin covers an important risk class. |
| Hipcheck returns malformed JSON or omits required identity, state, or value fields. | Treat the Hipcheck report as an integration error and do not derive findings from display text. | Use `unknown` for the Hipcheck portion unless other Night Vision evidence is stricter. |
| Hipcheck returns valid JSON with missing optional fields. | Normalize the usable checks and add missing-evidence findings for omitted optional evidence. | Reduce evidence quality without failing the whole assessment. |
| Hipcheck reports `INVESTIGATE` with no blocking Night Vision finding. | Preserve the Hipcheck recommendation and map the relevant failed checks into review findings. | Normally use `caution`, unless other evidence justifies `avoid` or `unknown`. |
| Hipcheck reports `PASS` but Night Vision has a blocking vulnerability or upgrade finding. | Keep the Hipcheck result as supporting supply chain evidence, but do not let it override Night Vision's domain findings. | Use `avoid` or the stricter Night Vision verdict. |

### Security Boundary

Hipcheck execution should be treated as untrusted external analysis work even
when the `hc` binary and plugins are bundled with Night Vision. The backend
should assume that package metadata, package archives, source repositories,
Hipcheck plugin output, stdout, stderr, and error messages may contain hostile
or malformed content.

The MVP integration should enforce these boundaries:

- Build `hc` command arguments without shell interpolation.
- Run `hc` with a constrained working directory controlled by Night Vision.
- Use Night Vision-owned cache, config, data, and exec paths rather than user
  home directories or ambient developer configuration.
- Pass an explicit, minimal environment to `hc` and plugin processes.
- Do not pass secrets, service tokens, database URLs, or user credentials into
  Hipcheck plugin environments by default.
- Allow plugin-specific credentials only through explicit Night Vision
  configuration, with clear ownership of which plugin receives which secret.
- Enforce wall-clock timeouts and bounded stdout, stderr, JSON, cache, and
  artifact sizes.
- Store raw Hipcheck output as evidence only after applying the same storage,
  retention, and display-safety rules used for other untrusted evidence.
- Treat plugin paths and manifests as deployment artifacts, not as values
  supplied by end users.

Night Vision should not rely on Hipcheck to protect the rest of the backend
from resource exhaustion, secret exposure, or unsafe display content. Hipcheck
may provide useful isolation and plugin-process boundaries, but Night Vision
owns the product-level security boundary around invoking it and storing its
results.

### Deployment

Night Vision deployments use the immutable artifact layout documented in
[`backend/hipcheck/README.md`](../../backend/hipcheck/README.md). It names the
pinned Hipcheck commit, exact runtime paths, and the command shape used by the
backend boundary. For local development, build the backend image and invoke its
bundled `hc`; do not install or discover a system `hc`. For containerized
deployment, the backend image includes:

- the `hc` binary;
- Night Vision's predetermined Hipcheck policy file;
- Hipcheck exec configuration;
- required Hipcheck plugin manifests or local paths for Night Vision-owned MVP
  plugins;
- writable cache and data directories with bounded retention.

Compose mounts only `/var/cache/night-vision/hipcheck` and
`/var/lib/night-vision/hipcheck` as writable storage. The policy, exec
configuration, manifests, and executable artifacts remain in the read-only
image. The MVP must prefer reproducible artifacts over floating plugin resolution.
Night Vision-owned plugins should be built and packaged from the same Night
Vision change set as the policy and backend integration. Later deployments can
add a stronger plugin update process, artifact mirroring, and cache management
once the integration is stable.

## Plugin Plan

When Night Vision needs a supply chain signal that Hipcheck does not already
provide, the Night Vision team should implement that signal as a Hipcheck
plugin instead of building an equivalent analyzer directly into Night Vision.

### Initial Plugin Areas

Likely Night Vision-driven Hipcheck plugin work includes:

- NPM release delta analysis, such as package size changes, new install
  scripts, new binaries, and new native build requirements.
- Package publication continuity analysis, such as discontiguous versions,
  unusual publication timing, and maintainer changes.
- Provenance and source consistency analysis, such as broken provenance
  attestations or source/tag mismatches.
- Dependency range and dependency delta analysis for upgrade candidates.
- Obfuscation or suspicious artifact detection for newly introduced packaged
  files.

### RFD 0001 Detection Coverage

The RFD 0001 supply chain detections should map to Hipcheck coverage as
follows:

| RFD 0001 detection | Additional Hipcheck plugin needed? |
| --- | --- |
| Discontiguous version | Yes. This requires package registry release-history comparison. |
| Package size increase | Yes. This requires comparing published package artifacts across versions. |
| New package maintainer | Yes. This requires registry maintainer or publisher history. |
| Broken provenance attestation | Yes. This requires package provenance validation and historical comparison. |
| New install script | Yes. This requires package manifest comparison across versions. |
| Source/tag mismatch | Yes. Existing repository analysis may help, but Night Vision needs package-to-source consistency checks. |
| Overly broad dependency range | Yes. This requires dependency range comparison and reachability context. |
| Malicious `bin` entries | Yes. This requires package manifest and executable-name analysis. |
| Introduction of `node-gyp` dependency | Yes. This requires dependency delta analysis for native build paths. |
| Malicious publication behavior | Partially. General Hipcheck project-practice signals may contribute, but package-release-specific signals need new plugin work. |
| Introduction of obfuscated code | Yes. This requires artifact inspection and comparison against prior package contents. |

### Data Plugin Needs

Night Vision-owned analysis plugins will also need richer Hipcheck data plugin
support. Existing Hipcheck data plugins provide useful starting points:
`mitre/npm` can read NPM dependencies from a local package tree, `mitre/git`
can expose commit, diff, and contributor history, and `mitre/github` can expose
GitHub review, contributor, collaborator, organization, and fuzzing data.

Night Vision's package-version comparisons require additional data access. That
general-purpose data access should be added upstream in Hipcheck:

| Needed data | Data plugin work |
| --- | --- |
| NPM version history, publication times, dist-tags, deprecation state, and publication continuity. | Improve `mitre/npm` with registry metadata queries, or add a focused NPM registry data plugin if that would keep the existing plugin's local-package role cleaner. |
| NPM package manifest data for arbitrary versions, including scripts, `bin` entries, dependency ranges, repository metadata, and native-build indicators. | Improve `mitre/npm`; the existing dependency query returns resolved dependency names, but Night Vision needs versioned manifest fields and ranges. |
| Published package artifact metadata, including tarball URL, integrity, unpacked size, file list, file modes, and selected file contents. | Improve `mitre/npm` with artifact queries, or add a dedicated NPM artifact data plugin if downloading and unpacking tarballs should be isolated from registry metadata access. |
| NPM maintainer, publisher, provenance, and publish-authentication data. | Improve `mitre/npm` for registry publisher metadata; add new provenance or Sigstore-oriented data access if NPM provenance evidence is not available through the registry data alone. |
| Source repository refs, tags, release commits, and package-to-source comparison inputs. | Improve `mitre/git` where local clone and ref inspection is enough; pair it with improved `mitre/npm` artifact metadata for source/tag mismatch checks. |
| GitHub release-process data, such as release tags, workflow files, workflow runs, branch protection, repository rules, and security settings. | Improve `mitre/github`; its existing collaborator, contributor, review, organization, and fuzzing queries do not cover release-process controls. |

### MVP Trust Boundary

For the MVP, Night Vision should run Hipcheck only with MITRE-produced plugins.
This keeps Hipcheck plugin supply chain risk outside the expected threat model
for the initial integration.

Night Vision should still treat plugin execution as external analysis work for
process, resource, and error isolation, but it does not need to solve
third-party Hipcheck plugin trust for the MVP.

### Night Vision-Owned Plugins

Night Vision-owned plugins should expose focused JSON outputs and, where
possible, default policy expressions. That allows the same plugin to be useful
to Hipcheck users outside Night Vision while still supporting Night Vision's
assessment workflow.

For the MVP, Night Vision-developed Hipcheck plugins should live in the Night
Vision repository. Keeping the plugins with Night Vision makes the assessment
policy, backend integration code, plugin tests, and plugin release artifacts
reviewable together while the product shape is still changing.

Night Vision may propose moving generally useful plugins upstream once their
APIs and policy behavior stabilize.

### Development Conventions

Night Vision-specific plugins should follow normal Hipcheck plugin conventions:

- provide a CLI that accepts `--port <PORT>`;
- optionally accept Hipcheck's `--log-level <LEVEL>`;
- keep running until Hipcheck shuts down the plugin process;
- expose typed query endpoints using the Hipcheck Rust SDK;
- define input and output schemas that are stable enough for policy files and
  Night Vision normalization;
- include unit tests for query logic and integration tests that run through
  `hc check` with a local policy file.

All new Night Vision-developed Hipcheck plugins should be implemented in Rust.
This keeps plugin code close to Hipcheck's native ecosystem, gives the team
strong static typing around package metadata and JSON schemas, and avoids
splitting long-term maintenance across multiple plugin languages.
