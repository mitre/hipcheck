---
title: Targets
weight: 1
---

# Targets

__Targets__ are Hipcheck's term for "things that Hipcheck analyzes," and
they are what you specify with the positional argument in the `hc check`
command. Generally, targets are intended to specify _something that leads
to a source repository_, which can seem like a vague concept.

More concretely, targets can be:

- A Git source repository URL or local path
- A package name and optional version (perhaps requiring you to specify the
  package host)
- An SPDX software bill of materials (SBOM) file with a source repository
  reference for the main package in it

Let's break each of those down in turn.

## Git Source Repository URL or Local Path

Hipcheck's central focus for analysis is a project's _source repository_,
because Hipcheck cares about analyzing the metadata associated with a project's
development (see [Why Hipcheck?](@/docs/getting-started/why.md) for more
information). Giving Hipcheck the source repository URL or local path is the
most _direct_ way of telling Hipcheck what you want it to analyze. When you
specify other types of targets, Hipcheck will see if it can find a reference to
a source repository from those targets, and will produce an error if it can't.

Specifying a Git source repository looks like:

```sh
$ # For a remote Git repository.
$ hc check https://github.com/mitre/hipcheck
$ # For an existing local Git repository.
$ hc check ~/Projects/hipcheck
```

When Hipcheck is given a remote URL for a Git repository, it will clone
that repository into a local directory, as analyses on local data are _much_
faster than trying to gather data across the network.

{% info(title="Hipcheck's Storage Paths") %}
Hipcheck uses three directories to store important materials it needs to
run. Each can be specified by a command line flag, by an environment
variable, or inferred from the user's current platform (in decreasing
priority). Each directory serves a specific purpose:

- The Config directory: stores Hipcheck's configuration files.
- The Data directory: stores Hipcheck's helper scripts needed for running
  additional external tools Hipcheck relies on.
- The Cache directory: stores local clones of repositories Hipcheck is
  analyzing.

Of these, the Cache directory is one that has a tendency to grow as your
use of Hipcheck continues. Some Git repositories, especially those for
long-running and very active projects, can be quite large. In the future,
we [plan to augment Hipcheck with tooling for better managing this
cache directory](https://github.com/mitre/hipcheck/issues/182).
{% end %}

In general, Hipcheck tries to ensure it ends up with both a _local path_
for a repository (either because the user specified a local repository
in the CLI, or by cloning a remote repository to a local cache) and
a _remote URL_. If the user provided a remote URL directly, that's not
a problem. If the user provided a local path, then Hipcheck tries to
infer the upstream repository by seeing if the default branch has an
upstream remote branch configured. If it does, then Hipcheck records
that as the remote branch for the local repository.

Hipcheck does this because some analyses rely on APIs provided by
specific source repository hosts. Today, only GitHub is supported,
but we'd like to add support for more source repository APIs in
the future. If the user provides a GitHub source repository URL,
or a local repository path from which a remote GitHub URL can be
inferred, then the GitHub-specific analyses will be able to run.

## Package Name and Optional Version

Users can also specify targets as a package name and version from some
popular open source package repositories. Today Hipcheck supports packages
on NPM (JavaScript), PyPI (Python), and Maven Central (Java). We'd like to
expand that support to more platforms in the future.

Packages from these hosts may be specified as package names, with optional
versions. When specifying one of these targets, it is not sufficient to
specify just the package name, you'll need to use the `-t`/`--type` flag to
specify the package host. For example:

```sh
$ hc check --type npm chalk@5.3.0
$ hc check --type pypi numpy@2.0.0
$ hc check --type maven commons-csv@1.11.0
```

Without specifying the platform, Hipcheck will be unable to determine
what package is being specified, and will produce an error.

For each of these types of targets, Hipcheck will then try to identify a
source repository associated with the package in the package's metadata.
The specific method of doing this differs depending on the platform.
Some provide a standard mechanism for specifying the source repository,
and some don't. For those that don't though, there are generally common
norms for how that information is provided, so Hipcheck can often still
identify the source repository in one of the common locations.

When the source repository is discovered, it is handled in the same way
as if it had been provided as the target directly by the user. See
this page's [Git Source Repository URL or Local Path](#git-source-repository-url-or-local-path)
section for more information.

## SPDX Software Bill of Materials

Finally, Hipcheck can accept SPDX version 2 Software Bills of Material (SBOM)
files, in the JSON or key-value text formats. SPDX is a popular format for
specifying Software Bills of Materials, meaning it contains information about
a package and the package's dependencies.

Running Hipcheck on an SPDX SBOM looks like:

```sh
$ hc check my-package.spdx.json
```

Today, Hipcheck only supports the SPDX 2.3 SBOM format, though we'd like to
add support for more formats, and for SPDX 3.0, in the future.

When provided with an SBOM, Hipcheck parses the file to identify the "root"
package being specified, and tries to infer any source repository information
for that package. If it is unable to identify a source repository for the
package being described, it produces an error. If it _can_ identify a source
repository, that repository is processed as if the user specified it directly
on the command line (see this page's [Git Source Repository URL or Local Path](#git-source-repository-url-or-local-path)
section for more information).

When provided with an SBOM, Hipcheck today _does not_ separately analyze
each of the dependencies specified in the SBOM. Rather, it _only_ analyzes
the root package. If you'd like to analyze each of the dependencies in the
SBOM, you'll need to call Hipcheck separately for each of them.
