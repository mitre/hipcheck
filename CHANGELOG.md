# Hipcheck Changelog

All notable changes to this project will be documented in this file. This
project adheres to [Semantic Versioning].

## [3.3.1] - 2024-06-21

This patch release includes two general categories of fixes:

- Getting `Containerfile` builds on new releases working.
- Getting `cargo-dist` generation of binaries on new releases working.

### Changed

* Fix broken Docker Hub action by [@alilleybrinker](https://github.com/alilleybrinker)
* Update Containerfile to be accepted on Docker Hub push by [@alilleybrinker](https://github.com/alilleybrinker)
* Upgrade `cargo-dist` to 0.16.0 by [@alilleybrinker](https://github.com/alilleybrinker) in [#137](https://github.com/mitre/hipcheck/pull/137)

### Fixed

* Fix broken Containerfile syntax by [@alilleybrinker](https://github.com/alilleybrinker)
* Get Containerfile working by [@alilleybrinker](https://github.com/alilleybrinker)
* Reduce prebuild targets to ones that work by [@alilleybrinker](https://github.com/alilleybrinker)

__Full Changelog__: <https://github.com/mitre/hipcheck/compare/hipcheck-v3.3.0...hipcheck-v3.3.1>

[3.3.1]: https://github.com/mitre/hipcheck/compare/hipcheck-v3.3.0..hipcheck-v3.3.1

## [3.3.0] - 2024-06-20

Hipcheck version 3.3.0 is mostly focused on refactors and internal
improvements, including a substantial refactor of the `hc` Command Line
Interface to be easier to use and easier for us to enhance in the future.
We've also continued to mature our tooling and processes around Hipcheck,
which should hopefully make future advancement easier.

## RFDs

* Add "Hipcheck's Values" RFD by [@alilleybrinker](https://github.com/alilleybrinker) in [#70](https://github.com/mitre/hipcheck/pull/70)
* Added RFD #3, "Plugin Architecture Vision" by [@alilleybrinker](https://github.com/alilleybrinker) in [#71](https://github.com/mitre/hipcheck/pull/71)

### `hc`

* Change `ureq` Agent to use native system certs by [@mchernicoff](https://github.com/mchernicoff) in [#85](https://github.com/mitre/hipcheck/pull/85)
* Remove OpenSSL as a Hipcheck dependency by [@mchernicoff](https://github.com/mchernicoff) in [#80](https://github.com/mitre/hipcheck/pull/80)
* Added new types to form the basis of scoring refactor by [@j-lanson](https://github.com/j-lanson) in [#127](https://github.com/mitre/hipcheck/pull/127)
* Change hc CLI to use derive instead of builder (WIP) by [@mchernicoff](https://github.com/mchernicoff)
* Small fixes; still panics with no argument by [@mchernicoff](https://github.com/mchernicoff)
* Refactor CLI by [@alilleybrinker](https://github.com/alilleybrinker) in [#93](https://github.com/mitre/hipcheck/pull/93)
* Print help on empty args to `check` and `schema` by [@j-lanson](https://github.com/j-lanson) in [#107](https://github.com/mitre/hipcheck/pull/107)
* Move error/context to appropriate sub-modules by [@j-lanson](https://github.com/j-lanson) in [#115](https://github.com/mitre/hipcheck/pull/115)
* Move `metric` and `session` out of `analysis` by [@mchernicoff](https://github.com/mchernicoff) in [#116](https://github.com/mitre/hipcheck/pull/116)
* Move `source` out of `data` by [@mchernicoff](https://github.com/mchernicoff) in [#117](https://github.com/mitre/hipcheck/pull/117)
* Creates general `http` module for making requests by [@mchernicoff](https://github.com/mchernicoff) in [#118](https://github.com/mitre/hipcheck/pull/118)
* Refactor `hc check` CLI by [@j-lanson](https://github.com/j-lanson)
* Initial work on performance by [@vcfxb](https://github.com/vcfxb) in [#131](https://github.com/mitre/hipcheck/pull/131)
* Improve performance of grapheme frequency calculation by [@vcfxb](https://github.com/vcfxb) in [#133](https://github.com/mitre/hipcheck/pull/133)
* Adds hc ready command by [@mchernicoff](https://github.com/mchernicoff) in [#81](https://github.com/mitre/hipcheck/pull/81)
* Restore `libc` version to 0.2.153 to match latest version on crates.io by [@mchernicoff](https://github.com/mchernicoff)
* Restore `libc` version to 0.2.153 to match latest version on crates.io by [@mchernicoff](https://github.com/mchernicoff)
* Make top-level commands for `hc` `Option`s to allow for no command by [@mchernicoff](https://github.com/mchernicoff)
* Removes unnecessary `use` in `cli.rs` by [@mchernicoff](https://github.com/mchernicoff)
* Remove unnecessary `Default` implementation for `hc help` by [@mchernicoff](https://github.com/mchernicoff)
* Adds test for CLI commands by [@mchernicoff](https://github.com/mchernicoff)
* Disable built-in `help` command for all `hc` commands by [@mchernicoff](https://github.com/mchernicoff)
* Rename help flag internally to pass tests by [@mchernicoff](https://github.com/mchernicoff)
* Fix mishandling of `HC_CONFIG` with new CLI by [@j-lanson](https://github.com/j-lanson) in [#114](https://github.com/mitre/hipcheck/pull/114)

### Continuous Integration Workflows

* Filter GitHub workflow to not run tests if changes to a push or pull-request are outside of code folders by [@mchernicoff](https://github.com/mchernicoff) in [#68](https://github.com/mitre/hipcheck/pull/68)
* Add "Dependency Tree" task to CI by [@alilleybrinker](https://github.com/alilleybrinker) in [#79](https://github.com/mitre/hipcheck/pull/79)
* Publish tagged HC releases to Dockerhub by [@j-lanson](https://github.com/j-lanson) in [#113](https://github.com/mitre/hipcheck/pull/113)
* Add ability to manually exec push-to-dockerhub action by [@j-lanson](https://github.com/j-lanson) in [#119](https://github.com/mitre/hipcheck/pull/119)

### `xtask`

`xtask` is our internal development tooling.

* Add license and description `xtask/src/task/rfd.rs` by [@mchernicoff](https://github.com/mchernicoff) in [#90](https://github.com/mitre/hipcheck/pull/90)
* Add `xtask` changelog sanity check for `git-cliff` by [@j-lanson](https://github.com/j-lanson) in [#92](https://github.com/mitre/hipcheck/pull/92)
* Change `xtask validate` to `xtask check` when `xtask ci` is called by [@mchernicoff](https://github.com/mchernicoff) in [#89](https://github.com/mitre/hipcheck/pull/89)

### Other Project Tooling

* `cargo release` updates Hipcheck version in README by [@mchernicoff](https://github.com/mchernicoff) in [#111](https://github.com/mitre/hipcheck/pull/111)
* Make `cargo-dist` releases include `config/` and `scripts/` by [@alilleybrinker](https://github.com/alilleybrinker) in [#135](https://github.com/mitre/hipcheck/pull/135)
* Removes missing `/libs` folder from Container file by [@mchernicoff](https://github.com/mchernicoff) in [#72](https://github.com/mitre/hipcheck/pull/72)

### Dependency Version Bumps

* Bump anyhow from 1.0.83 to 1.0.86 by [@dependabot[bot]](https://github.com/dependabot) in [#76](https://github.com/mitre/hipcheck/pull/76)
* Bump clap from 4.5.6 to 4.5.7 by [@dependabot[bot]](https://github.com/dependabot)
* Bump clap from 4.5.4 to 4.5.6 by [@dependabot[bot]](https://github.com/dependabot) in [#122](https://github.com/mitre/hipcheck/pull/122)
* Bump libc from 0.2.154 to 0.2.155 by [@dependabot[bot]](https://github.com/dependabot) in [#74](https://github.com/mitre/hipcheck/pull/74)
* Bump proc-macro2 from 1.0.84 to 1.0.85 by [@dependabot[bot]](https://github.com/dependabot) in [#109](https://github.com/mitre/hipcheck/pull/109)
* Bump regex from 1.10.4 to 1.10.5 by [@dependabot[bot]](https://github.com/dependabot) in [#121](https://github.com/mitre/hipcheck/pull/121)
* Bump schemars from 0.8.19 to 0.8.20 by [@dependabot[bot]](https://github.com/dependabot) in [#78](https://github.com/mitre/hipcheck/pull/78)
* Bump schemars from 0.8.20 to 0.8.21 by [@dependabot[bot]](https://github.com/dependabot) in [#83](https://github.com/mitre/hipcheck/pull/83)
* Bump serde from 1.0.201 to 1.0.202 by [@dependabot[bot]](https://github.com/dependabot) in [#75](https://github.com/mitre/hipcheck/pull/75)
* Bump serde from 1.0.202 to 1.0.203 by [@dependabot[bot]](https://github.com/dependabot) in [#82](https://github.com/mitre/hipcheck/pull/82)
* Bump toml from 0.8.12 to 0.8.13 by [@dependabot[bot]](https://github.com/dependabot) in [#77](https://github.com/mitre/hipcheck/pull/77)
* Bump toml from 0.8.13 to 0.8.14 by [@dependabot[bot]](https://github.com/dependabot) in [#123](https://github.com/mitre/hipcheck/pull/123)
* Bump url from 2.5.0 to 2.5.1 by [@dependabot[bot]](https://github.com/dependabot)

### New Contributors

* [@vcfxb](https://github.com/vcfxb) made their first contribution in [#133](https://github.com/mitre/hipcheck/pull/133)

__Full Changelog__: <https://github.com/mitre/hipcheck/compare/hipcheck-v3.2.1...hipcheck-v3.3.0>

[3.3.0]: https://github.com/mitre/hipcheck/compare/hipcheck-v3.2.1..hipcheck-v3.3.0

## [3.2.1] - 2024-05-10

Nothing really new in Hipcheck itself. Publishing this version mostly to work
out issues with the machinery for publishing new releases and distributing
prebuilt binaries.

### Added

* add `xtask changelog` command by [@alilleybrinker](https://github.com/alilleybrinker) in [#63](https://github.com/mitre/hipcheck/pull/63)

### Changed

* Improved `Cargo.toml` metadata, removed unused deps by [@alilleybrinker](https://github.com/alilleybrinker) in [#61](https://github.com/mitre/hipcheck/pull/61)
* Improved `xtask` experience, removed old commands by [@alilleybrinker](https://github.com/alilleybrinker) in [#62](https://github.com/mitre/hipcheck/pull/62)

### Fixed

* Removed `publish = false` on Hipcheck by [@alilleybrinker](https://github.com/alilleybrinker)
* Add missing crate description for Hipcheck by [@alilleybrinker](https://github.com/alilleybrinker)
* Corrected bad metadata in Hipcheck crate by [@alilleybrinker](https://github.com/alilleybrinker)
* Fix broken `cargo-dist` build by [@alilleybrinker](https://github.com/alilleybrinker) in [#60](https://github.com/mitre/hipcheck/pull/60)

__Full Changelog__: <https://github.com/mitre/hipcheck/compare/hipcheck-v3.2.0...hipcheck-v3.3.0>

[3.2.1]: https://github.com/mitre/hipcheck/compare/hipcheck-v3.2.0..hipcheck-v3.2.1

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
* Bump `ureq` from 2.9.6 to 2.9.7 by [@dependabot[bot]](https://github.com/dependabot) in [#35](https://github.com/mitre/hipcheck/pull/35)
* Bump `schemars` from 0.8.16 to 0.8.17 by [@dependabot[bot]](https://github.com/dependabot) in [#34](https://github.com/mitre/hipcheck/pull/34)
* Add Conventional Commit check to CI by [@alilleybrinker](https://github.com/alilleybrinker) in [#36](https://github.com/mitre/hipcheck/pull/36)
* Added basic CI testing by [@alilleybrinker](https://github.com/alilleybrinker)
* Move common-use crates into `hc_common` by [@mchernicoff](https://github.com/mchernicoff) in [#37](https://github.com/mitre/hipcheck/pull/37)
* Move data type and retrieval crates into `hc_data` by [@mchernicoff](https://github.com/mchernicoff) in [#39](https://github.com/mitre/hipcheck/pull/39)
* Merges support crates for `hc_data` into `hc_data` by [@mchernicoff](https://github.com/mchernicoff) in [#40](https://github.com/mitre/hipcheck/pull/40)
* Merge crates into `hc_metric` by [@mchernicoff](https://github.com/mchernicoff) in [#43](https://github.com/mitre/hipcheck/pull/43)
* Move `hc_pm` into `hc_session` by [@mchernicoff](https://github.com/mchernicoff) in [#44](https://github.com/mitre/hipcheck/pull/44)
* Creates a single analysis crate that handles most of the Hipcheck analysis pipeline by [@mchernicoff](https://github.com/mchernicoff) in [#45](https://github.com/mitre/hipcheck/pull/45)
* Bump `libc` from 0.2.153 to 0.2.154 by [@dependabot[bot]](https://github.com/dependabot) in [#46](https://github.com/mitre/hipcheck/pull/46)
* Complete unifying Hipcheck in single crate by [@alilleybrinker](https://github.com/alilleybrinker) in [#47](https://github.com/mitre/hipcheck/pull/47)
* Removed dead code by [@alilleybrinker](https://github.com/alilleybrinker) in [#50](https://github.com/mitre/hipcheck/pull/50)
* Added "Release Engineering" RFD by [@alilleybrinker](https://github.com/alilleybrinker) in [#48](https://github.com/mitre/hipcheck/pull/48)
* Add `cargo-dist` for prebuilt binaries by [@alilleybrinker](https://github.com/alilleybrinker) in [#41](https://github.com/mitre/hipcheck/pull/41)
* Removed `pathbuf` module in favor of crate by [@alilleybrinker](https://github.com/alilleybrinker)
* Organize helper modules under `util/` by [@alilleybrinker](https://github.com/alilleybrinker)
* Added basic community docs by [@alilleybrinker](https://github.com/alilleybrinker) in [#54](https://github.com/mitre/hipcheck/pull/54)
* Adds security policy by [@mchernicoff](https://github.com/mchernicoff) in [#59](https://github.com/mitre/hipcheck/pull/59)

### Fixed

* Resolve Cargo warnings by [@alilleybrinker](https://github.com/alilleybrinker)
* Move dependabot config back to `.github` folder by [@alilleybrinker](https://github.com/alilleybrinker) in [#38](https://github.com/mitre/hipcheck/pull/38)
* Remove `atty` dep for GHSA-g98v-hv3f-hcfr by [@alilleybrinker](https://github.com/alilleybrinker) in [#42](https://github.com/mitre/hipcheck/pull/42)
* Add missing license notices by [@alilleybrinker](https://github.com/alilleybrinker) in [#52](https://github.com/mitre/hipcheck/pull/52)
* Fix double-version command in `xtask` by [@alilleybrinker](https://github.com/alilleybrinker) in [#51](https://github.com/mitre/hipcheck/pull/51)
* Get `cargo xtask doc --open` working again by [@alilleybrinker](https://github.com/alilleybrinker) in [#53](https://github.com/mitre/hipcheck/pull/53)

### New Contributors

* [@mchernicoff](https://github.com/mchernicoff) made their first contribution in [#59](https://github.com/mitre/hipcheck/pull/59)
* [@dependabot[bot]](https://github.com/dependabot) made their first contribution in [#46](https://github.com/mitre/hipcheck/pull/46)

[3.2.0]: https://github.com/mitre/hipcheck/compare/4372390..HEAD
[Semantic Versioning]: https://semver.org/spec/v2.0.0.html
