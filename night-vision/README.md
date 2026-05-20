
# Night Vision: 🔭 CTI for OSS Packages

__Night Vision__ is a cyber threat intelligence (CTI) system for open source
software (OSS) packages. It allows users to subscribe for alerts to new cyber
threat indicators on the open source software they use.

> [!important]
> This project is currently in __early development__ and is not yet suitable
> for production deployment.

## What Night Vision Does

With Night Vision, users will provide their package files (like `package.json`
for most JavaScript packages) to subscribe to alerts for new cyber threats for
all the dependencies in those package files. Night Vision will thereafter
monitor those packages for new threats, and alert users when any are discovered
so they can take action.

Currently, we are building the Minimum Viable Product, with the intent to
support analyzing packages hosted on [NPM] found in users' `package.json`
files. More package hosts, package files, source repository hosts, and beyond
will be supported in future versions of Night Vision.

## Repository Contents

```text
.
├── backend/    The backend REST API (in Rust) and PostgreSQL database.
├── docs/       Internal project documentation.
└── frontend/   The frontend application (in TypeScript w/ SvelteKit).
```

## Getting Started

### Onboarding

Before you start contributing to the project, it's a good idea to read the
following docs:

- [Dev Practices](docs/project/dev-practices.md): expectations and tips for
  being a successful Night Vision developer.
- [AI Policy](docs/project/ai-policy.md): our rules and recommendations for
  using AI on the project.

Additionally, if you're working on the backend (the REST API or PostgreSQL
database) check out [`docs/backend/`](docs/backend/). We don't yet have
documentation for the frontend, though it will be added soon as we get frontend
development up and running.

You can see the full set of documentation in
[`docs/introduction.md`](docs/introduction.md).

### Installing Tools

Night Vision uses [Flox] to ensure a consistent development environment. The
manifest describing what software is packaged is found in
`.flox/env/manifest.toml`.

To get started with developing on Night Vision:

- __Install Git__: See the [Git install guide] for more. We recommend the
  latest version of Git, though any relatively recent version is acceptable.
  After activating Flox, you'll have access to the latest version of Git, as
  provided by Flox. This initial install is only necessary to clone the Night
  Vision repository.
- __Install Flox__: See our [Flox guide] for more. Install Flox version
  1.11.4 or later.
- __Clone the Night Vision repository__: Run
  `git clone git@github.com:mitre/hipcheck.git`
- __Activate Flox__: Inside the new `night-vision/` folder, run `flox activate`.
- (Optional) __Activate Jujutsu__: Some team members use [Jujutsu] locally. If
  you want to use `jj`, run `jj git init --colocate`. Jujutsu is installed in
  the Flox environment, so you don't need to install it manually.

You should now have all the tools you need to contribute to the Night Vision
project! Versions for all tools are set and maintained as part of the Flox
environment, so you don't need to worry about installing the correct versions
yourself.

Whenever you want to stop using the Flox environment, run `exit` or quit
your terminal.

### Developing on Windows

Unfortunately, [Nix] (which Flox uses under the hood) does not support Windows,
and so neither does Flox. To use Flox on Windows, we recommend using WSL
(the Windows Subsystem for Linux), which Flox supports.

## License




[Flox]: https://flox.dev/
[Jujutsu]: https://github.com/jj-vcs/jj
[Git install guide]: https://git-scm.com/install/
[Flox guide]: ./docs/project/flox.md
[Nix]: https://nix.dev/
[NPM]: https://www.npmjs.com/
