---
title: Contribute
---

# Contribute to Hipcheck

The Hipcheck project is happy to accept contributions!

## Coordinating Changes

For small changes, including improvements to documentation, correction of
bugs, fixing typos, and general code quality improvements, submitting
without coordinating with the Hipcheck team is generally fine and
appreciated!

For larger changes, including the addition of new data sources, new analyses,
refactoring modules, changing the CLI or configuration, or similar, we
highly suggest discussing your proposed changes before submission. Often
this will begin with opening up a GitHub Issue or a Discussion, and for
larger changes may also involve writing a Request for Discussion (RFD)
document.

RFD's are how the Hipcheck project manages large scale changes to the tool,
and are documented more on the RFD's page.

The Hipcheck product roadmap is public, and we always recommend checking
there to see how your proposed changes may fit into the currently-planned
work.

## Commit Messages

All commits to Hipcheck are required to follow the Conventional Commits
specfication. We use this requirement to help us auto-generate material
for our `CHANGELOG.md` and GitHub Release notes with each new version,
though we do still double-check and write them by hand.

We also generally try to make sure commits serve a reasonably clear
purpose, and include comments whenever appropriate to explain the
reasoning behind what is being changed, or at least link to a GitHub
Issue or Discussion for further explanation.

## Testing

All changes to Hipcheck must pass continuous integration (CI) tests prior
to being merged. You can simulate this test suite, at least on your own
operating system and architecture, using the following command:

```sh
$ cargo xtask ci
```

Passing this command is not a _guarantee_ of passing the official CI suite
on GitHub, but is a good way to approximate things locally.

If you want faster tests locally, we also recommend installing `cargo-nextest`.
The `cargo xtask ci` command will use it instead of `cargo test` if it's
installed.

## Intellectual Property

When you make contributions to Hipcheck, they're done under the terms
of the Apache 2.0 license.
