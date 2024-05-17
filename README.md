# Hipcheck

__Hipcheck__ scores risks for software projects; yours and your dependencies.
It analyzes repositories to assess risks, review development practices,
and identify possible supply chain attacks, making it possible to assess
and manage open source software supply chain security at scale.

---


* [Capabilities](#capabilities)
* [Goals](#goals)
* [Analyses](#analyses)
* [Usage](#usage)
  * [Installation](#installation)
  * [Configuration](#configuration)
* [Examples](#examples)
* [Learn More](#learn-more)

## Capabilities

Hipcheck can analyze repositories and pull requests. For repositories,
it answers questions like:

* Does this project practice code review?
* When was this project last updated?
* Are there concerning contributors to this project?
* Are there potential malicious contributions to review?
* Are there potential typosquatting attacks present?
* Where are the highest risk parts of the codebase?

For pull requests, it answers questions like:

* What parts of the code are in the greatest need of review?
* Is this pull request especially concerning?
* Is this contributor new to this part of the code?

With analyses like these (and more), Hipcheck provides automation-assisted
risk management for software projects.

## Goals

Hipcheck's core goals are to be:

* __Effective__: A risk tool is only helpful if it identifies risks. Hipcheck's
  analyses look at project practices, potential supply chain attacks, who is
  contributing, and how projects change over time to produce high quality,
  actionable conclusions and to guide manual review.
* __Fast__: Software development moves quickly, and Hipcheck runs quickly too.
  Whether it's running in CI looking for high-risk PRs, reporting on
  high risk parts of a codebase, or running against your dependencies,
  you won't wait long for a risk report.
* __Configurable__: Different projects have different threat models and risk
  tolerances, and Hipcheck handles them gracefully. Analyses, weights, and
  risk thresholds are all configurable.

## Installation

### As a Container

You can build Hipcheck locally with `docker`, using the
Hipcheck `Containerfile`.

```sh
$ # Run the following from the root of the Hipcheck repository.
$ docker build -t hipcheck:3.1.0 -f ./Containerfile
```

### Local Install

First, install the Rust compiler. We recommend following the official
[installation instructions][install_rust]. Make sure to add
`${CARGO_HOME}/bin` to your `PATH`.

You will also need Node installed. We recommend following the official
[installation_instructions][install_node].

You may either install using an [automated script](#script-based-installation)
or [manually](#build-from-source).

#### Script-based Installation

You can install the latest release of Hipcheck by downloading and running the
`install.sh` script from the repository root.

```sh
$ curl https://raw.githubusercontent.com/mitre/hipcheck/main/install.sh | bash
```

This will ask you to export pre-defined values for the `HC_CONFIG` and `HC_DATA`
environment variables on which Hipcheck relies.

#### Build from Source

Get the Hipcheck repository. Then navigate into the root directory of
the repository and run `cargo install --path hipcheck`.

```sh
$ git clone https://github.com/mitre/hipcheck
$ cd hipcheck
$ cargo install --path hipcheck
```

## Usage

### Container Image

You can run Hipcheck in a container like so:

```sh
$ docker run --env "HC_GITHUB_TOKEN=<GITHUB_TOKEN>" hipheck:3.1.0 [<HIPCHECK_ARGS>]...
```

### Direct Usage

To run Hipcheck, make sure you export `HC_GITHUB_TOKEN` with a valid token for
connecting to the GitHub API.

If you installed from `install.sh` and set the appropriate environment
variables as directed, you can run Hipcheck with the `hc` binary without any
further configuration.

```sh
$ hc check repo https://github.com/expressjs/express
```

If you installed from source, you will need to configure values for `--config`,
`--data` and `--home`. From the CLI section of the Hipcheck book:

* -c, --config `<FILE>`
    * Specifies the path to the configuration file.
    * This value can instead be set persistently with the `HC_CONFIG` environment variable.
    * **Hipcheck will not run `hc check` if it cannot find the configuration file.**
    * The config file is called Hipcheck.toml.
    * On a default Hipcheck installation, this file should be in `hipcheck/config/`.
    * If no filepath is specified, Hipcheck defaults to looking in the current active directory.

* -d, --data `<FOLDER>`
    * Specifies the path to the folder containing essential Hipcheck data files.
    * This value can instead be set persistently with the `HC_DATA` environment variable.
    * **Certain Hipcheck analyses will generate an error if they cannot find necessary files in this folder.**
    * The custom Hipcheck `module-deps.js` file needs to be in this folder.
    * A default Hipcheck installation currently does not create this folder and the files in it.
    * If no filepath is specified, Hipcheck defaults to looking in the default platform data directory.

* -H, --home `<FOLDER>`
    * Specifies the path to the hipcheck home/root where repos are cached.
    * If no filepath is specified, Hipcheck will look in the `HC_HOME` system environment variable first and then the system cache directory second.
    * `hc --print-home` shows the directory Hipcheck is currently using as its home.
    * If a Git repo cloning is interrupted or Hipcheck is using too much disk space, clear appropriate subdirectories in the home directory.

### Run Configuring

Hipcheck requires a set of configuration files, which you can find default
versions of in this repository, under the `config/` directory. The path to
this configuration file must be specified if it is not in the current
active directory.

## Learn More

Hipcheck is documented in the Hipcheck book, found under the `/docs/book` directory
in this repository. Follow the instructions in the README there to build and
view the contents of the book.

## License

Hipcheck's software is licensed under the Apache 2.0 license (SPDX license
identifier `Apache-2.0`), the full text of which may be found in the `LICENSE.md`
file included with this repository.

## Public Release

Approved for Public Release; Distribution Unlimited. Public Release Case Number 22-2145.

Portions of this software were produced for the U. S. Government under Contract No.
FA8702-19-C-0001 and W56KGU-18-D-0004, and is subject to the Rights in Noncommercial
Computer Software and Noncommercial Computer Software Documentation Clause DFARS
252.227-7014 (FEB 2014).

[react]: https://github.com/facebook/react
[install_rust]: https://www.rust-lang.org/tools/install
[install_node]: https://nodejs.org/en/learn/getting-started/how-to-install-nodejs

