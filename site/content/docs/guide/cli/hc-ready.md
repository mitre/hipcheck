---
title: hc ready
extra:
  nav_title: "<code>hc ready</code>"
---

# `hc ready`

`hc ready` is a command for checking that Hipcheck is ready to run analyses.
This is intended to help the user debug issues with a Hipcheck installation,
including problems like missing configuration files, inaccessible config,
data, or cache paths, missing authentication tokens, and more.

`hc ready` starts all the plugins specified in the
[policy file](@/docs/guide/config/policy-file.md) and confirms that initial
configuration is successful. If `hc ready` doesn't report any problems with
plugins, that means they are installed, runnable by Hipcheck, and are able to
load their configuration without issues.

Since `hc ready` does not involve the analysis subsystem like
[`hc check`](@/docs/guide/cli/hc-check.md) does, it will not expose plugin
issues that would only occur while analyzing a target.

`hc ready` has no special flags currently, and only accepts the
[General Flags](@/docs/guide/cli/general-flags.md) that _all_ Hipcheck
commands accept.

The output of `hc ready` is a report containing key information about
Hipcheck, the third-party tools it relies on as external data sources,
the paths it's currently using for configuration files, data files,
and local repository clones, along with any API tokens it will use
for external API access.

If all required information is found and passes requirements for Hipcheck
to run, it will report that Hipcheck is ready to run.

We recommend running `hc ready` before running Hipcheck the first time,
and as a good first debugging step if Hipcheck begins reporting issues.
