
# Night Vision: 🔭 CTI for OSS Packages

__Night Vision__ is a cyber threat intelligence (CTI) system for open source
software (OSS) packages. It allows users to subscribe for alerts to new cyber
threat indicators on the open source software they use.

## What Night Vision Does

With Night Vision, users will provide their package files (like `package.json`
for most JavaScript packages) to subscribe to alerts for new cyber threats for
all the dependencies in those package files. Night Vision will thereafter
monitor those packages for new threats, and alert users when any are discovered
so they can take action.

## Repository Contents

This repository contains both the backend and frontend for Night Vision. The
backend is a REST API written in [Rust] and backed by [PostgreSQL]. The frontend
is a single page application (SPA) written in [TypeScript] with [Svelte] and
using [pnpm] as its package manager and build system.

## Getting Started

Night Vision uses [Flox] to ensure a consistent development environment. The
manifest describing what software is packaged is found in
`.flox/env/manifest.toml`.

To get started with developing on Night Vision:

- __Install Git__: See the [Git install guide] for more.
- __Install Flox__: See the [Flox install guide] for more.
- __Checkout Night Vision__: `git clone git@github.com:mitre/hipcheck.git`
- __Activate Flox__: Inside the new `night-vision/` folder, run `flox activate`.
- (Optional) __Activate Jujutsu__: Some team members use [Jujutsu] locally. If
  you want to use `jj`, run `jj git init --colocate`.

You should now have all the tools you need to contribute to the Night Vision
project!

Whenever you want to stop using the Flox environment, run `exit` or quit
your terminal.

### Developing on Windows

Unfortunately, [Nix] (which Flox uses under the hood) does not support Windows,
and so neither does Flox. To use Flox on Windows, we recommend using WSL
(the Windows Subsystem for Linux), which Flox supports.

## License




[Rust]: https://rust-lang.org/
[PostgreSQL]: https://www.postgresql.org/
[TypeScript]: https://www.typescriptlang.org/
[Svelte]: https://svelte.dev/
[pnpm]: https://pnpm.io/
[Flox]: https://flox.dev/
[Jujutsu]: https://github.com/jj-vcs/jj
[Git install guide]: https://git-scm.com/install/
[Flox install guide]: https://flox.dev/docs/install-flox/install/
[Nix]: https://nix.dev/
