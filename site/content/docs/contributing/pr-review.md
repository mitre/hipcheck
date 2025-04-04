---
title: PR Submission and Review Checklist
weight: 4
---

# PR Submission and Review Checklists

These checklists should be followed by PR submitters and reviewers to ensure 
that changes to Hipcheck's usage are properly versioned, do not result in
stale documentation, and keep the working tree clean. 

## PR Review

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

## PR Submission

1. Ensure all tests pass

2. (If the PR has previously been reviewed) Read comments on merge request, make changes accordingly, and resolve conversations on github. 

3. Ensure documentation comments have ///, the rest have //. There should be a space between the comments and slashes
   
   3a. Comments should be descriptive but not redundant 

   3b. Comments should have proper spelling

4. Run cargo xtask ci in the terminal and fix any errors

5. Make needed commits using git add and git commit

6. Squash all commits using rebase https://www.git-tower.com/learn/git/faq/git-squash

7. Make sure commit message follows conventional commit standards https://www.conventionalcommits.org/en/v1.0.0/

8. Sign off on the squashed commit https://docs.pi-hole.net/guides/github/how-to-signoff/

