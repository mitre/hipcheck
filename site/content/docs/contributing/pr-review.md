---
title: PR Review Checklist
weight: 4
---

# PR Review Checklist

This checklist should be followed both by PR submitters and reviewers to ensure
that changes to Hipcheck's usage are properly versioned and do not result in
stale documentation.

1. If this PR has introduced organizational changes to the repository's
   directory structure (such as a refactor or adding a new crate), ensure that
   information in
   `site/content/docs/contributing/developer-docs/repo-structure.md` is
   up-to-date.

2. If this PR contains the first changes to a crate since its last release,
   ensure that the version of that crate has been [appropriately bumped][semver]
   in the current release Tracker issue, as well as all of its dependencies.

3. For any changes to the following (non-exhaustive): Hipcheck core CLI, a
   plugin's configuration fields, the plugin gRPC interface, Hipcheck
   configuration file formats (e.g. policy files, `Exec.kdl`), the policy
   expression language syntax or semantics; ensure that appropriate sections of
   the documentation have been updated accordingly.

[semver]: https://semver.org/
