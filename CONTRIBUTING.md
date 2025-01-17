# Contributing

The Hipcheck project is happy to accept contributions! We want Hipcheck to be
the best possible toolkit for assessing risks in open source software.

## Before Contributing

We recommend you do a few things before contributing to the project:

- __Read the Hipcheck Values__: The Hipcheck project has a list of values for
  the product and the project, enumerated in [RFD #2][values].
- __Read the Code of Conduct__: We require all participants in the project
  and in project-controlled spaces like our Discussions forum to follow the
  [Code of Conduct][coc]. This outlines expectations, procedures for reporting
  violations, and means of enforcement.
- __Read the Contributing Guide__: We maintain a
  [guide to contributing][contributing] to Hipcheck on our project website,
  which covers the mechanics of coordinating changes, testing changes,
  licensing, and more.

## When Contributing

The expectations for a contribution vary depending on how substantial it is.

- __Small Contributions__, including bug fixes, minor documentation changes
  like edits for grammar, spelling, or clarity, or code quality improvements
  which don't impact functionality, should be submitted by:
  1. Optionally, [opening an issue][issue] in the issue tracker explaining the
     problem and proposing the change.
  2. [Submitting a PR][pr] with the change, requesting review from the [Hipcheck
     Maintainers][maintainers] team.
- __Larger Contributions__, including changes to functionality of Hipcheck or
  proposals for changes to the plugin protocol, should:
  1. Start with an issue in the issue tracker.
  2. If large enough, may require an RFD. You can learn more about the RFD
     process from ["RFD #0: The RFD Process."][rfd_0]
  3. Submit a PR with the change, requesting review from the Hipcheck
     Maintainers team.

We also recommend checking out our [public roadmap][roadmap] to understand
our current priorities.

When making contributions, we do have some expectations:

- __Signoff Commits__: We enforce a [Developer Certificate of Origin][dco]
  and require [signoff on all commits][signoff]. When you open a PR, there's a
  GitHub Action that checks for this.
- __Builds on Supported Platforms__: We require builds to pass on all platforms
  for which we provide prebuilt binaries. This is checked in CI.
- __Passes All Tests__: We require passing tests on all supported platforms
  as well. This is checked in CI.
- __Formatting, Linting__: We run all changes against `cargo fmt` and
  `cargo clippy` to ensure they're meeting our style guidelines.
- __Document Your Changes__: If you're making a change which impacts an API,
  we expect that you will update documentation to reflect your changes.
- __Conventional Commits__: We enforce [Conventional Commits][cc] in CI. Commit
  messages must indicate what kind of change is being made.

Many of the above items can be checked locally with the command
`cargo xtask ci`. Even more, such as cross-platform passing tests, is checked
in CI on a PR.

[values]: https://hipcheck.mitre.org/docs/rfds/0002/
[coc]: https://github.com/mitre/hipcheck/blob/main/CODE_OF_CONDUCT.md
[contributing]: https://hipcheck.mitre.org/docs/contributing/
[dco]: https://developercertificate.org/
[signoff]: https://git-scm.com/docs/git-commit#Documentation/git-commit.txt--s
[roadmap]: https://github.com/orgs/mitre/projects/29
[rfd_0]: https://hipcheck.mitre.org/docs/rfds/0000/
[issue]: https://github.com/mitre/hipcheck/issues/new/choose
[pr]: https://github.com/mitre/hipcheck/compare
[maintainers]: https://github.com/orgs/mitre/teams/hipcheck-maintainers
[cc]: https://www.conventionalcommits.org/en/v1.0.0/
