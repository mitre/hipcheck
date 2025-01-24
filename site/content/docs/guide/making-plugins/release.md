---
title: Packaging and Releasing a Plugin
weight: 3
---

# Packaging and Releasing a Plugin

When your plugin implementation is complete, whatever the language (Rust,
Python, etc.) or platform (executable, Container image, etc.) you have chosen,
you will have a collection of one or more "artifacts". These are the files
necessary for your plugin to run. The next step is to package the plugin so that
it can be used in a Hipcheck analysis.

To start the packaging process, create a directory that contains your plugin
artifacts. Inside that directory you must write a [plugin manifest file](#the-plugin-manifest-file)
named `plugin.kdl` that tells Hipcheck how to run it. After you have written
this file, if you are testing the plugin, only using the plugin locally, or
copying it to other machines offline, see [Local Deployment](#local-deployment).
Otherwise if you plan to host the plugin on a webserver so that it can be
downloaded by other users, see [Remote Deployment](#remote-deployment).

## The Plugin Manifest File

Each Hipcheck plugin must have an associated plugin manifest file named
`plugin.kdl`.  It provides information about your plugin, how to run it, and any
runtime dependencies on other plugins it may have. Here is an example plugin
manifest file:

```
publisher "mitre"
name "activity"
version "0.2.0"
license "Apache-2.0"

entrypoint {
  on arch="aarch64-apple-darwin" "activity"
  on arch="x86_64-apple-darwin" "activity"
  on arch="x86_64-unknown-linux-gnu" "activity"
  on arch="x86_64-pc-windows-msvc" "activity.exe"
}

dependencies {
  plugin "mitre/git" version="0.3.0" manifest="https://hipcheck.mitre.org/dl/plugin/mitre/git.kdl"
}
```

The fields are as follows:

- `publisher`: String (required) - The name of your organization.
- `name`: String (required) - The name of the plugin.
- `version`: String (required) - A [SemVer][semver]-compatible version identifier.
- `license`: String (required) - The license under which your plugin may be
	used.
- `entrypoint`: Object (required) - A list containing the commandline invocation
	of your plugin on each supported architecture. See [below](#entrypoint).
- `dependencies`: Object (optional) - A list identifying the versioned Hipcheck
    plugins that your plugin may query during its own execution. See
	[below](#dependencies).

### `entrypoint`

Each element in this list describes how to run your plugin on a particular
architecture.

The `arch` string is a [target triple][target-triple] describing
a platform on which software runs. For example `x86_64-pc-windows-msvc`
describes a Windows OS using Microsoft Visual C++ and running on an x86_64 chip.
In the majority of cases, if you are running on a standard CPU running a
standard operating system (MacOS, Windows, Ubuntu, Debian, etc.), your
architecture will be one of the options shown in the above example `plugin.kdl`
file. If not, you can write your own arch target triple. When Hipcheck runs and
parses a `plugin.kdl` it will try to match the architecture triple that it
detected at compile time against the `arch` field of each entrypoint to select
the right one. If Hipcheck is struggling to do so, you can pass `--arch
<TARGET_TRIPLE>` on the commandline to force it to use a target triple string
of your choosing when selecting the proper `entrypoint`.

After the `arch` string comes the string defining how Hipcheck will invoke the
plugin on the commandline. When Hipcheck runs the plugin, it will append the
directory containing the `plugin.kdl` and the plugin artifacts to the system
path so your software may find the artifacts it needs. Hipcheck expects that
the entrypoint string will start with the name of a binary on the operating
system path, followed by some number of arguments. For example, if your plugin
is an executable file, the entrypoint string may be as simple as
`"<PLUGIN_FILE_NAME> [ARGS}"`, as above.  If your plugin were a Python script,
the entrypoint string may be `"python3 <PLUGIN_PY_FILE>" [ARGS]`. If your plugin
code is represented by a container image, you may use `"podman <IMAGE_FILE>
[ARGS]"` or `"docker <IMAGE_FILE> [ARGS]"`. Whatever it is, at runtime Hipcheck
will append ` --port <PORT>` to this string to tell the plugin which port to
listen on, so you will need to ensure that the behavior of your entrypoint
string can handle this addition.

You may have as many or as few entries in the `entrypoint` section of the plugin
manifest. If you are doing a [local deployment](#local-deployment), you may
include only the single architecture on which you plan to run the plugin. If you
are releasing the plugin for many people to use, it is recommended but not
required to support at least the four Hipcheck-supported architectures:

- `aarch64-apple-darwin` - MacOS running on aarch64
- `x86_64-apple-darwin` - MacOS running on x86_64
- `x86_64-unknown-linux-gnu` - Linux running on x86_64
- `x86_64-pc-windows-msvc` - Windows with Microsoft Visual C++ running on x86_64

Note that if your plugin is an executable, you will need to cross-compile the
binary for each architecture you plan to support.

### `dependencies`

The dependencies section is an optional list of other Hipcheck plugins upon
which your plugin relies to function. This list should include any plugin
queried by any of your plugin's endpoints. Referring to the above example
`plugin.kdl`, we see that each entry starts with `plugin`, and contains the
publisher/name pair of the plugin, its version specifier, and a `manifest`
field.

In an abstract way, the `manifest` field "points" to the dependent plugin. If
you are deploying your plugin locally and rely on another local plugin, the
`manifest` should be a path to the `plugin.kdl` of the dependent plugin. Note
that if you use a relative path, it will be interpreted as relative to the
directory in which Hipcheck runs, not the directory in which the current
`plugin.kdl` file is stored. Thus, absolute paths should be preferred where
possible. If your plugin has a dependency on a remotely-hosted plugin, the
`manifest` field should contain a URL to that plugin's [download manifest](#the-download-manifest).

## Local Deployment

In a local deployment scenario, you run Hipcheck using a copy of your plugin
that is already on your local machine. Even if you plan on doing a public
release, this deployment type is highly encouraged for testing beforehand.

By now you should have a directory somewhere on your machine containing your
plugin manifest file and all of the artifacts for your plugin. Assuming your
plugin manifest has an entry in `entrypoint` that matches your current
architecture, you can run the plugin in a Hipcheck analysis. If built your
plugin to be a top-level analysis, add your plugin to the `plugins` section of
the [policy file](@/docs/guide/config/policy-file.md#the-plugin-section) you use
in your Hipcheck analysis. Ensure that the `manifest` field points to your local
`plugin.kdl` file. The path you provide will need to be relative to the
directory from which Hipcheck runs, or you can use the `#rel()` [macro][rel-macro]
to specify a path relative to the policy file itself. Then, add your plugin to a
relevant `category` in the `analysis` section, set up any necessary
configuration, and you are ready to use your plugin!

If your plugin was instead designed to act as a dependency of an existing local
plugin, simply update the `dependencies` section of that plugin's `plugin.kdl`
with an entry for your plugin, following the same guidance for the `manifest`
field as above.

## Remote Deployment

Publishing a plugin involves creating archived packages for each architecture
you plan to support and storing these packages along with a [download manifest](#the-download-manifest) in
a network-accessible location. For publicly released plugins, this would be an
Internet-facing webserver. With each new release of your plugin, you will need
to update the download manifest and upload a new compressed package for each
architecture, so this process will benefit heavily from automation if you expect
to release new versions of your plugin over time.

As mentioned above, you will need an archived package for each architecture
you support. If all of your plugin artifacts are platform-independent, such as
text files and container images, you can use a single package directory for all
architectures. On the other hand, if any artifacts are platform-dependent, such
as an executable file, you will need to create platform-specific versions of
each of those artifacts and create a separate package directory for each plugin.
For executable files, this involves cross-compilation of your source code.

Once you have one or more package directories that cover each architecture, you
can move on to archiving the package(s). Hipcheck supports the following archive
formats: `.zip`, `.tar`, `.tar.gz`, `.tar.xz`, and `.tar.zst`. When archiving
your plugin artifacts into one of these formats, be careful to note that
**Hipcheck expects your plugin artifacts to be in the root of the archive, not
included as a sub-directory.** Also, do not forget a properly-versioned
`plugin.kdl` file in each archive.

Now you should have one or more archive files that contain plugin artifacts.
The penultimate step is to write the download manifest file.

### The Download Manifest

The download manifest file is an index of the different `(version,
architecture)` pairs available for your plugin, and information about where to
retrieve the archive file for that pair. For example:

```
plugin version="0.1.0" arch="aarch64-apple-darwin" {
    url "https://github.com/mitre/hipcheck/releases/download/git-v0.1.0/git-aarch64-apple-darwin.tar.xz"
    hash alg="SHA256" digest="e419092d9caef566ef1903c417f66ecb155917cc50e278e8b2c5de127baf51c7"
    compress format="tar.xz"
    size bytes=1081808
}

plugin version="0.1.0" arch="x86_64-pc-windows-msvc" {
    url "https://github.com/mitre/hipcheck/releases/download/git-v0.1.0/git-x86_64-pc-windows-msvc.zip"
    hash alg="SHA256" digest="b9ab124af4b22df0b68e57f1e036ac5127d391e1745527bfec86c79f2a9e49b3"
    compress format="zip"
    size bytes=4085279
}

plugin version="0.2.0" arch="aarch64-apple-darwin" {
    url "https://github.com/mitre/hipcheck/releases/download/git-v0.2.0/git-aarch64-apple-darwin.tar.xz"
    hash alg="SHA256" digest="9a57a247461ffdfb41426339ff0037cf6f4435d4f16df9f2ef73255d6bc86d9a"
    compress format="tar.xz"
    size bytes=1926440
}

plugin version="0.2.0" arch="x86_64-pc-windows-msvc" {
    url "https://github.com/mitre/hipcheck/releases/download/git-v0.2.0/git-x86_64-pc-windows-msvc.zip"
    hash alg="SHA256" digest="fc292ddfb259b1e679d893f32b2f6bffe4b6e8b5a49fd3e1596c824d91f56a2e"
    compress format="zip"
    size bytes=6939167
}
```

This file contains four entries, spanning two versions and two architectures.
Each entry starts with the word `plugin` followed by the `version=` and `arch=`
fields. Within each entry are the following fields, all of which are required:

- `url`: String - A URL pointing to the plugin package archive.
- `hash` - Information about the hash of the archive file for download verification
	- `alg`: String - The hash algorithm used. Currently only `SHA256` and `BLAKE3` are supported.
	- `digest`: String - The hash digest of the archive file as a hexadecimal string.
- `compress`:
	- `format`: String - The compression algorithm used. Valid values include `zip`, `tar`, `tar.gz`, `tar.xz`, `tar.zst`.
- `size`:
	- `bytes`: Integer - The size of the archive file in bytes.

Besides the `url` field which allows Hipcheck to find the archive file, the
above fields all help Hipcheck validate that the archive file download was
complete and correct.

Now you must ensure that the plugin archives are accessible at the URL that you
specified in your download manifest, and finally you must upload the download
manifest file itself to a network-accessible location. To finally use your
plugin, include it either as a dependency in another plugin's `plugin.kdl`, or
specify it as an analysis plugin in the `plugins` field of a policy file. In
either case, set the `manifest=` field of that entry to be the URL of the
download manifest file.

[semver]: https://semver.org/
[target-triple]: https://wiki.osdev.org/Target_Triplet
[rel-macro]: @/docs/guide/config/policy-file.md#macros
