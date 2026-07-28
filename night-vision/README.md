
# Night Vision: 🔭 CTI for OSS Packages

__Night Vision__ is a cyber threat intelligence (CTI) system for open source
software (OSS) packages. Its Minimum Viable Product (MVP) helps users assess
whether newer patch versions are reasonable moves from known-insecure package
versions, with explicit motivation to support Federal Civilian Executive
Branch (FCEB) compliance with CISA's BOD 26-04.

> [!important]
> This project is currently in __early development__ and is not yet suitable
> for production deployment.

## What is Night Vision?

With Night Vision, users will assess packages they depend on, starting with
newer patch versions that may help them move away from known-insecure package
versions. Over time, Night Vision will also support package-source analysis,
ongoing monitoring, and alerts for new cyber threats affecting dependencies.

Currently, we are building the Minimum Viable Product, with the intent to
support upgrade safety assessments for packages hosted on [NPM]. The MVP is
motivated by CISA's BOD 26-04, and focuses on noticing when reachable package
versions enter the Known Exploited Vulnerabilities (KEV) catalog and
identifying safer versions to upgrade to. See
[RFD 0001](docs/rfds/0001-mvp-upgrade-safety-assessments.md) for the MVP
discussion. More package hosts, package files, source repository hosts, and
beyond will be supported in future versions of Night Vision.

## Repository Contents

```text
.
├── .env.local.example        Example environment file for local Docker Compose.
├── .env.production.example   Example environment file for production Docker Compose.
├── .flox/                    Flox development environment configuration.
├── backend/                  The backend REST API and database handling.
├── docs/                     Internal project documentation.
├── frontend/                 The frontend application in TypeScript w/ SvelteKit.
├── scripts/                  Helper scripts for local Docker Compose and testing.
├── CONTRIBUTING.md           Contribution workflow and project conventions.
├── docker-compose.local.yml  Local Docker Compose overrides.
└── docker-compose.yml        Base Docker Compose configuration.
```

## Getting Started

Before you start contributing to the project, it's a good idea to read the
following docs:

- [Dev Practices](docs/project/dev-practices.md): expectations and tips for
  being a successful Night Vision developer.
- [AI Policy](docs/project/ai-policy.md): our rules and recommendations for
  using AI on the project.

Additionally, if you're working on the backend (the REST API or PostgreSQL
database) check out [`docs/backend/`](docs/backend/). If you're working on the
SvelteKit frontend, start with the
[frontend developer guide](docs/frontend/developer-guide.md).

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
  1.12.0 or later.
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

### Limitations on Windows

Unfortunately, [Nix] (which Flox uses under the hood) does not support Windows,
and so neither does Flox. To use Flox on Windows, we recommend using WSL
(the Windows Subsystem for Linux), which Flox supports.

Night Vision is not deployed on Windows. Contributions from developers using
Windows are welcome, but Windows support is best-effort and not guaranteed.

## Running with Docker Compose

### Local Development

Local Compose uses `docker-compose.yml` plus `docker-compose.local.yml`. The
local override builds `nv-app:local` and `nv-server:local`, binds the frontend
to `127.0.0.1:3000`, the backend to `127.0.0.1:8080`, and PostgreSQL to
`127.0.0.1:5432`. It uses a clearly named local Postgres volume and CVE List
cache volume. The setup script writes
local secret files; the local Compose wrapper stages those files into a
Docker-approved host mount directory before passing them to containers as
Compose secrets. The default
staging directory is `/Users/Shared/Docker/night-vision` on macOS and
`C:\Users\Public\Docker\night-vision` on Windows, or `.secrets/docker` on
native Linux. Set `DOCKER_SECRET_MOUNT_DIR` to override it.

```sh
cp .env.local.example .env
scripts/setup-compose-secrets.sh -x
scripts/docker-compose-local.sh up --build
```

To build the backend image from a network that requires custom certificate
authorities, set `CA_FILE_SECRET_FILE` to the certificate bundle before running
the local Compose wrapper. The wrapper stages the file with the other local
secrets and passes it to the backend Dockerfile as the `ca_file` build secret.

```sh
CA_FILE_SECRET_FILE="$HOME/.config/certs/system_certs.pem" scripts/docker-compose-local.sh up --build
```

To choose a different local password, pass it only to the setup command:

```sh
POSTGRES_PASSWORD='replace-me' scripts/setup-compose-secrets.sh -x
```

If you change `POSTGRES_DB`, `POSTGRES_USER`, or the Postgres password after the
database volume has already been initialized, recreate the volume before
starting Compose again:

```sh
scripts/docker-compose-local.sh down -v
scripts/docker-compose-local.sh up --build
```

To validate the Docker shell scripts and local Compose configuration, run:

```sh
scripts/test.sh
```

### Production Deployment

Production Compose uses `docker-compose.yml` only. Copy
`.env.production.example` to `.env` and provide explicit values for
`NV_SERVER_IMAGE`, `NV_SERVER_DATABASE_URL_SECRET_FILE`, and
`POSTGRES_PASSWORD_SECRET_FILE`.

```sh
docker compose up -d
```

Production Compose secrets are mounted from host files. They keep secret values
out of container environment inspection and generated Compose config, but they
are not an encrypted secret store. Secret files must contain exactly one line
and must not be readable by group or world.

## License




[Flox]: https://flox.dev/
[Jujutsu]: https://github.com/jj-vcs/jj
[Git install guide]: https://git-scm.com/install/
[Flox guide]: ./docs/project/flox.md
[Nix]: https://nix.dev/
[NPM]: https://www.npmjs.com/
