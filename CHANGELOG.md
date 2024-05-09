# Hipcheck Changelog

All notable changes to this project will be documented in this file. This
project adheres to [Semantic Versioning].

## [3.2.0] - 2024-05-09

This is the first new version of Hipcheck since our initial open source
release, and it represents a lot of housekeeping to get the project up
and running! That includes:

- Getting Hipcheck compiling cleanly on the latest stable version of Rust.
- Getting all of Hipcheck's dependencies up to date.
- Shrinking Hipcheck's crate structure down to just a single binary crate.

In addition, we worked on a lot of best-practice related items, including:

- Defining RFD's (Requests for Discussion) as our means of managing the
  evolution of Hipcheck over time.
- Setting up a DevContainer configuration, for folks who'd like to contribute
  to Hipcheck without needing to set up their local environment by hand.
- Establishing Continuous Integration testing, to increase confidence in the
  correctness of future changes we may merge.
- Defining a security policy, a code of conduct, and a guide for potential
  contributors, so people know how to interact with the project.
- Defining our "Release Engineering" practices, which will help smooth out
  the flow of future releases of Hipcheck.

Up next we're planning to work on more serious redesigning of Hipcheck's
architecture to support third-party plugins for data and analysis. If that's
something that appeals to you, please let us know in the Discussions page!

Here's to the first of many more releases!

### Changed

* Run rustfmt to fix formatting by [@alilleybrinker](https://github.com/alilleybrinker) in [#20](https://github.com/mitre/hipcheck/pull/20)
* Introduce RFD process by [@alilleybrinker](https://github.com/alilleybrinker) in [#25](https://github.com/mitre/hipcheck/pull/25)
* Define devcontainer config by [@alilleybrinker](https://github.com/alilleybrinker) in [#26](https://github.com/mitre/hipcheck/pull/26)
* Enable dependabot version bumps by [@alilleybrinker](https://github.com/alilleybrinker) in [#32](https://github.com/mitre/hipcheck/pull/32)
* Bump ureq from 2.9.6 to 2.9.7 by [@dependabot[bot]](https://github.com/dependabot) in [#35](https://github.com/mitre/hipcheck/pull/35)
* Bump schemars from 0.8.16 to 0.8.17 by [@dependabot[bot]](https://github.com/dependabot) in [#34](https://github.com/mitre/hipcheck/pull/34)
* Add Conventional Commit check to CI by [@alilleybrinker](https://github.com/alilleybrinker) in [#36](https://github.com/mitre/hipcheck/pull/36)
* Added basic CI testing by [@alilleybrinker](https://github.com/alilleybrinker)
* Move common-use crates into hc_common by [@mchernicoff](https://github.com/mchernicoff) in [#37](https://github.com/mitre/hipcheck/pull/37)
* Move data type and retrieval crates into hc_data by [@mchernicoff](https://github.com/mchernicoff) in [#39](https://github.com/mitre/hipcheck/pull/39)
* Merges support crates for hc_data into hc_data by [@mchernicoff](https://github.com/mchernicoff) in [#40](https://github.com/mitre/hipcheck/pull/40)
* Merge crates into hc_metric by [@mchernicoff](https://github.com/mchernicoff) in [#43](https://github.com/mitre/hipcheck/pull/43)
* Move hc_pm into hc_session by [@mchernicoff](https://github.com/mchernicoff) in [#44](https://github.com/mitre/hipcheck/pull/44)
* Creates a single analysis crate that handles most of the Hipcheck analysis pipeline by [@mchernicoff](https://github.com/mchernicoff) in [#45](https://github.com/mitre/hipcheck/pull/45)
* Bump libc from 0.2.153 to 0.2.154 by [@dependabot[bot]](https://github.com/dependabot) in [#46](https://github.com/mitre/hipcheck/pull/46)
* Complete unifying Hipcheck in single crate by [@alilleybrinker](https://github.com/alilleybrinker) in [#47](https://github.com/mitre/hipcheck/pull/47)
* Removed dead code by [@alilleybrinker](https://github.com/alilleybrinker) in [#50](https://github.com/mitre/hipcheck/pull/50)
* Added "Release Engineering" RFD by [@alilleybrinker](https://github.com/alilleybrinker) in [#48](https://github.com/mitre/hipcheck/pull/48)
* Add 'cargo-dist' for prebuilt binaries by [@alilleybrinker](https://github.com/alilleybrinker) in [#41](https://github.com/mitre/hipcheck/pull/41)
* Removed pathbuf module in favor of crate by [@alilleybrinker](https://github.com/alilleybrinker)
* Organize helper modules under `util/` by [@alilleybrinker](https://github.com/alilleybrinker)
* Added basic community docs by [@alilleybrinker](https://github.com/alilleybrinker) in [#54](https://github.com/mitre/hipcheck/pull/54)
* Adds security policy by [@mchernicoff](https://github.com/mchernicoff) in [#59](https://github.com/mitre/hipcheck/pull/59)

### Fixed

* Resolve Cargo warnings by [@alilleybrinker](https://github.com/alilleybrinker)
* Move dependabot config back to .github folder by [@alilleybrinker](https://github.com/alilleybrinker) in [#38](https://github.com/mitre/hipcheck/pull/38)
* Remove 'atty' dep for GHSA-g98v-hv3f-hcfr by [@alilleybrinker](https://github.com/alilleybrinker) in [#42](https://github.com/mitre/hipcheck/pull/42)
* Add missing license notices by [@alilleybrinker](https://github.com/alilleybrinker) in [#52](https://github.com/mitre/hipcheck/pull/52)
* Fix double-version command in `xtask` by [@alilleybrinker](https://github.com/alilleybrinker) in [#51](https://github.com/mitre/hipcheck/pull/51)
* Get `cargo xtask doc --open` working again by [@alilleybrinker](https://github.com/alilleybrinker) in [#53](https://github.com/mitre/hipcheck/pull/53)

### New Contributors

* [@mchernicoff](https://github.com/mchernicoff) made their first contribution in [#59](https://github.com/mitre/hipcheck/pull/59)
* [@dependabot[bot]](https://github.com/dependabot) made their first contribution in [#46](https://github.com/mitre/hipcheck/pull/46)

[3.2.0]: https://github.com/mitre/hipcheck/compare/4372390..HEAD
[Semantic Versioning]: https://semver.org/spec/v2.0.0.html
