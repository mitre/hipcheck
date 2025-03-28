---
title: Report Caching
weight: 12
slug: 0012
extra:
  rfd: 12
  primary_author: Julian Lanson
  primary_author_link: https://github.com/j-lanson
  status: Proposed
---

# Report Caching

In anticipation of users running Hipcheck as a GitHub action against the
dependencies of their own project, we propose to add to Hipcheck the ability to
cache reports. Given the same policy file, Hipcheck binary, target repository
and ref, Hipcheck is nearly deterministic across runs (a few plugins in our
suite need updating to restrict their analyses not to surpass the date/time of
the target ref of the target repo). Therefore, we can help drive down execution
time of multi-target analyses in resource-constrained environments such as
Continuous Integration (CI) runners by allowing Hipcheck to cache the report of
previous runs and skip redundant execution.

## Report Caching Keys

We have identified the following as the keys that must be matched to have a
report "cache hit":
- **The policy file path and hash.** Not only does the policy file itself need
	to be consistent (indicated by the hash), but it must be in the same
	location relative to the Hipcheck binary. This is because the policy file
	may contain relative paths to config files for various plugins, so moving
	the policy file can have indirect effects on the configuration of plugins,
	and by extension the score a target receives.
- **The Hipcheck binary hash.** Naturally, an `hc` binary that hashes to a
	different value can produce a different report. We choose to use the binary
	hash instead of the version string for uniquely identifying the Hipcheck
	instance to help developers who would otherwise have to manually invalidate
	the cache between each run (since during development the code has changed but
	the version has not).
- **The repository and analyzed commit.** The repository is an obvious key,
	which would likely be represented by the repo's path in the Hipcheck target
	cache. We must specifically key on the git commit or tag that was analyzed
	rather than the ref, because refspecs such as `HEAD` can implicitly refer to
	different commits as the remote repository is updated.

One limitation present in this keying scheme is that changes to plugin-specific
config files that are referenced by the policy file ought also to invalidate a
cached repo. For example, a policy file using the `mitre/binary` plugin might
configure that plugin to use `Binary.kdl`. If the user changes the content of
`Binary.kdl`, the analysis configuration is technically different, but the
policy file path and hash have not changed. We have addressed overcoming this
limitation in a subsequent [RFD][rfd-13].

## Inserting Report Caching into the Analysis Workflow

Checking the report cache for an existing report should occur just after single
target resolution, where we now have the concrete commit/tag information for a
`LocalGitRepo` instance. Inserting a report in the cache should occur upon a
successful execution of the `hipcheck/src/main.rs::run()` function in
`cmd_check()`. We anticipate that this guidance will be impacted by the changes
proposed by RFD11 to support multi-target seeds. It is uncertain whether the
RFD11 changes or the changes proposed in this RFD will be implemented first, but
in the end the report cache should be checked before we execute the analysis of
a single target repository and after we receive a successful report for that analysis,
regardless of how the structure of Hipcheck core changes.

We will mention this below as well, but given the anticipated use of
parallelization for multi-target seeds, we will need synchronization primitives
to protect the report cache from race conditions.

## Report Cache Design

The report cache should follow the precedent of existing target and plugin
caches, and thus be stored in a `report/` subdirectory of the directory
identified by the `HC_CACHE` environment variable.

These existing Hipcheck caches have simple keying systems with an implicit
hierarchical structure. For example, a plugin's version is more specific than
it's name, which is in turn more specific than its publisher. This has allowed
for a simple implementation using a directory hierarchy as keys, and the "value"
is a file in the lowest subdirectory. The proposed report cache keying strategy
has more elements and does not have an implicit hierarchy; if we were to try
using directories as keys, would the policy file hash be parent to the Hipcheck
binary hash or vice versa? Where would the repository fit in? These are
important questions because if we adopt a directory-based system, if we wanted
to invalidate the cache based on a particular sub-key, we would have to search
the entire `report/` directory recursively for paths containing that sub-key.

Therefore, we propose using a simple database file, such as a SQLite database,
to store a table containing all the key elements plus the filename of the
report. We would likely store all the reports directly in `report/` as
compressed files, plus the one database file. We also propose a simple unique ID
column for the table so that entries can be referenced without specifying every
key element. We discuss the generation strategy and format of the unique ID
below.

### Report Cache Entry Unique ID

Since the multi-key that is used to cache elements is burdensome to specify when
manipulating entries, we propose to add a unique ID scheme to simplify referring
to reports.

To generate this hash, we will do the following (all hash operations performed
using SHA256):

1. Generate the combined hash of the policy file and plugins, as described in
   [RFD13][rfd-13].
2. Generate the Hipcheck binary hash.
3. Hash the concatenation of the repository name and specific commit ID being
   analyzed.
4. Concatenate the hashes of 1-3 in a consistent order and hash the result,
   which will be the unique ID.

As a hash shortcode, we can allow `<REPO_NAME>-<SHORT_HASH>`, where
`<SHORT_HASH>` is the shortest hash prefix necessary to distinguish the report
from other reports on the same repository.

### Report Cache Synchronization

With the implementation of RFD11, it will be more likely for multiple `hc check`
analysis instances to be adding, deleting, and/or reading cache entries
simultaneously. Due to the transaction synchronization logic it offers, using a
SQLite (or comparable) database to manage the cache would help to reduce (but
not entirely eliminate) the possibility of race conditions. The remaining
potential race condition that we have identified is that between when an entry
is created for a report and when the associated compressed file appears in the
`report/` directory. As an alternative to this split-storage system, the entire
compressed report could be stored as a blob in the database file. [This
page](sqlite-blobs) shows whether storing blobs inside the database is
effective, as a product of configured page size and blob size. We should
investigate what size we expect a compressed report JSON file to take up.

## Report Cache CLI

As with the target and plugin caches, we propose offering users a subcommand for
interfacing with the report cache, namely `hc cache report`. Users
are most likely to want to clear the cache, set a different caching eviction
policy, or invalidate all entries that match a certain subkey (e.g., invalidate
all entries for the following policy file). We propose the following subcommand
semantics:

```
// Invalidate all (request user confirmation)
hc cache report delete --all

// Shortcut for `invalidate --all`
hc cache report clear

// Invalidate all (don't request user confirmation)
hc cache report delete --all -y

// Invalidate all entries keyed by this hc binary.
// If no <HC_PATH> supplied, defaults to the current
// `hc` binary.
hc cache report delete --hc [<HC_PATH>]

// Invalidate all entries keyed by this policy file
hc cache report delete --policy <POLICY_PATH>

// Invalidate all entries keyed by this target, use
// target resolution system to resolve to a repo/ref
hc cache report delete --target <TARGET> [--ref <REFSPEC>]

// Invalidate the entry with this unique cache ID
hc cache report delete <UNIQUE_ID>
```

For the `delete` subcommand, we propose that `--hc`, `--policy`, and `--target`
flags can all be composed to target particular entry subgroups with increasing
specificity.

### Reviewing Reports Marked For Investigation

We anticipate a CI workflow where users run Hipcheck against their project
dependencies and mark the job as failing if any dependencies are marked for
investigation. We expect the user will then do their own investigation of the
marked dependency, after which they may either decide eliminate or substitute
that dependency in their project or determine that the dependency is not a risk.
In this latter case the user will wish to allow the CI to continue without
failing for this dependency.

Thus, we propose a `hc cache report reviewed` command that takes the policy file
and target information as `hc check` does, but updates the `report/` database to
indicate that the failed report has been reviewed and should no longer cause an
alert. This would constitute an additional boolean column in the SQLite database
file for each entry. The benefit of tying the "reviewed" status to a particular
cache entry is that if any of the key elements change (different `hc` version,
policy file, or repo commit), the "reviewed" status for the old key no longer
applies, and users will be asked to re-review.

The proposed CLI for this command is as follows:

```
// Mark the cache entry with policy <POLICY_PATH>, target <TARGET> as reviewed.
// Optionally specify <HC_PATH>, otherwise defaults to the current `hc` binary.
hc cache report reviewed --policy <POLICY_PATH> --target <TARGET> [--ref
<REFSPEC>] [--hc <HC_PATH>]

// Mark the report with cache row unique ID <UNIQUE_ID> as reviewed
hc cache report reviewed <UNIQUE_ID>
```

Pseudo-code for how the "reviewed" status influences the control flow is as
follows:

```
if report for target exists in cache:
  if reviewed:
    no alert
	return
  else:
    score report
	if failed investigate policy
	  mark needs investigation
else:
  generate report
  cache report
  score report
  if failed investigate policy
    mark needs investigation
```

### Report Cache Functionality in CI

One rather obvious deficiency in the design we have proposed so far is how users
running Hipcheck as a CI action will be able to mark dependencies as reviewed to
enable the CI to move forward, given they don't have easy access to a terminal
for running CLI commands. We describe a solution here, and take this opportunity
to sketch out some broader aspects of Hipcheck in CI.

We propose Hipcheck to act as a GitHub CI action that fails if any provided
targets need investigation. This means that, provided subsequent workflow steps
aren't explicitly configured to run in spite of a previous failed step, the
entire job will also fail. The failure message will report to users what
dependencies/targets need investigation, and each dependency's unique ID. As a
side note: if users are not interested in this more strict workflow, they can
mark the check action as `continue-on-error` so that an "investigate"
determination on any number of dependencies does not cause the whole job to
fail.

Once the user decides the dependency is acceptable, they will update a file they
keep in their repository called `reviewed.txt`, which will be a newline-
separated list of unique IDs or report shortcodes. This list will be read in by
Hipcheck at runtime during an `hc check` and will be referenced when reports are
emitted to determine whether to return a failure status code when Hipcheck
encounters a report in need of investigation.

This addresses how to let Hipcheck pass CI, but not necessarily how to actually
cache report generation in a GitLab runner. For this, we propose to use the
`actions/cache` GitHub action to cache the `HC_CACHE` directory between runs.
Keys in the cache crated by this action are deleted after 7 days without use; so as a workaround to
allow people who don't run Hipcheck consistently in that timeframe, we could
offer a hack GitHub action that simply loads the key. Users can set up this hack
action as a cron job to ensure it gets set consistently. This strategy has the
limitation that when two Hipcheck actions run simultaenously, the second
instance to complete will trample the writes to the report cache directory and
SQL file that was made by the first.

[sqlite-blobs]: https://www.sqlite.org/intern-v-extern-blob.html
[workspace-docs]: https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/store-information-in-variables#default-environment-variables
[cache-action]: https://github.com/marketplace/actions/cache
[rfd-13]: @docs/rfds/0013-plugin-config-hash.md
