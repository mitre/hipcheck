
# CLI

Hipcheck uses a command line interface based on nested subcommands to implement its various features. This page describes Hipcheck's CLI and the function of each subcommand.

## Using Hipcheck's CLI

The command to run Hipcheck from the command line is `hc`. All subcommands are run with the syntax `hc [FLAGS] [OPTIONS] <SUBTASK>`. Entering `hc` without any subcommands will run `hc help`.

## Available first level subcommands

* help
* check
* schema

### help

`hc [FLAGS] [OPTIONS] help [<SUBTASK>]`

Displays the help text.

Running the help subcommand without another subcommand displays the main help text for Hipcheck, while `hc help check` and `hc help schema` display the help text for the other subcommands.

### check

`hc [FLAGS] [OPTIONS] check <SUBTASK>`

Analyzes a repository or single pull request with a set of analysis tools. Hipcheck then provides a detailed overall risk assessment. This is Hipcheck's main function.

A github token must be set in the HC_GITHUB_TOKEN system environment variable for `hc check` to run. Hipcheck only needs permissions for accessing public repository data, so those are the only permissions to assign to your generated token.

There are currently two working secondary subcommands under `check`. Running `hc check` without a subtask runs `hc help check`. The available subcommands are:

* npm
    * `hc [FLAGS] [OPTIONS] check npm <PACKAGE>` obtains the Git repo for an npm package by uri or <package name>[@<optional version>], analyzes the Git repo for risks, and outputs an overall risk assessment for that npm package Git repo.
* maven
    * `hc [FLAGS] [OPTIONS] check maven <PACKAGE>` obtains the Git repo for a maven package by uri, analyzes the Git repo for risks, and outputs an overall risk assessment for that maven package Git repo.
* pypi
    * `hc [FLAGS] [OPTIONS] check pypi <PACKAGE>` obtains the Git repo for a pypi package by uri or <package name>[@<optional version>], analyzes the Git repo for risks, and outputs an overall risk assessment for that pypi package Git repo.
* repo
    * `hc [FLAGS] [OPTIONS] check repo <SOURCE>` analyzes an entire repository for risks and outputs an overall risk assessment for that repository.
* request
    * `hc [FLAGS] [OPTIONS] check request <PR/MR URI>` analyzes a single pull or merge request for risks and outputs an overall risk assessment for that pull/merge request.
* spdx
    * `hc [FLAGS] [OPTIONS] check spdx <FILEPATH>` analyzes packages described in an SPDX 2.2 tag-value or JSON document.

### schema

`hc [FLAGS] [OPTIONS] schema <SUBTASK>`

Prints a JSON formatted schema corresponding to analyzing a specified subtarget. The provided schema matches the reports that Hipcheck generates when using `hc check`.

There are currently two working secondary subcommands under `schema`. This subcommand specifies the subtarget whose schema this command prints. Running `hc schema` without a subtask runs `hc help schema`. The available subcommands are:

* repo
    * `hc [FLAGS] [OPTIONS] schema repo` prints the schema for the report generated when Hipcheck analyzes an entire repository.
* request
    * `hc  [FLAGS] [OPTIONS] schema request` prints the schema for the report generated when Hipcheck analyzes a single pull or merge request.

## Flags

All flags may be entered before or after a subcommand.

* -V, --version
    * Prints the current version information.
    * This will override any provided subcommand.
* --print-config
    * Prints the current Hipcheck config directory.
    * This will override any provided subcommand.
* --print-data
    * Prints the current Hipcheck data directory.
    * This will override any provided subcommand.
* --print-home
    * Prints the current Hipcheck home directory.
    * This will override any provided subcommand.

## Options

All options may be entered before or after a subcommand.

### Configuration Options

* -c, --config `<FILE>`
    * Specifies the path to the configuration file.
    * **Hipcheck will not run `hc check` if it cannot find the configuration file.**
    * The config file is called Hipcheck.toml.
    * On a default Hipcheck installation, this file should be in `hipcheck/config/`.
    * If no filepath is specified, Hipcheck defaults to looking in the current active directory.

* -d, --data `<FOLDER>`
    * Specifies the path to the folder containing essential Hipcheck data files.
    * **Certain Hipcheck analyses will generate an error if they cannot find necessary files in this folder.**
    * The custom Hipcheck `module-deps.js` file needs to be in this folder.
    * A default Hipcheck installation currently does not create this folder and the files in it.
    * If no filepath is specified, Hipcheck defaults to looking in the default platform data directory.

* -H, --home `<FOLDER>`
    * Specifies the path to the hipcheck home/root where repos are cached.
    * If no filepath is specified, Hipcheck defaults to looking in the HC_HOME system environment variable first and then the system cache directory second.

### Output Options
* -j, --json
    * Displays the output of running `hc check` in JSON format.
* -k, --color `[<COLOR>]`
    * Sets the output coloring.
    * Color options are `auto`, `never`, and `always`.
    * The default output coloring is `auto`.
* -q, --quiet
    * Silences progress reporting when running `hc check`.
