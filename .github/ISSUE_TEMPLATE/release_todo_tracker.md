---
name: Release Todo Tracker
about: Track component version bumps leading up to next release
title: "\U0001F6E4️ Tracking: Hipcheck v<VERSION> Release Tracker"
labels: 'product: project, tracking-issue, type: chore'
assignees: ''

---

If a bump specifies a dependent component, bump that component's version, and so on.

Crates (bump `Cargo.toml`)
- `hipcheck-common` (bumps `hipcheck`, `hipcheck-sdk` crates)
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `hipcheck-sdk-macros` (bumps `hipcheck-sdk`)
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `hipcheck-sdk` (bumps all Rust plugins)
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `hipcheck-macros` (bumps `hipcheck`)
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `hipcheck` (bump `site/{config.toml, static/dl/install.*}`, `dist/Containerfile`) 
  - [ ] Major
  - [ ] Minor
  - [ ] Patch

Rust Plugins (bump `Cargo.toml`, `plugin.kdl`, `local-plugin.kdl`, Policy files, Download manifests)
- All plugins
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `git` (bumps `activity`, `affiliation`, `churn`, `entropy`, `identity`)
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `github` (bumps `fuzz`, `identity`)
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `npm` (bumps `typo`)
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `activity`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `affiliation`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `binary`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `churn`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `entropy`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `fuzz`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `identity`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `linguist`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `review`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch
- `typo`
  - [ ] Major
  - [ ] Minor
  - [ ] Patch

Misc:
- [ ] Policy files. Update plugin versions in `config/{Hipcheck.kdl, local.Hipcheck.kdl}` and `config_to_policy.rs`
- [ ] Download manifests. After plugin release, run `cargo xtask manifest` to auto-update files in `site/static/dl`
