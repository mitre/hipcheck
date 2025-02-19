---
title: Starting Debugging
weight: 1
---

# Starting Debugging

## Using `hc ready`

The `hc ready` command prints a variety of information about how Hipcheck is
currently configured, including Hipcheck's own version, the versions of tools
Hipcheck may need to run its analyses, the configuration of paths Hipcheck will
use during execution, and the presence of API tokens Hipcheck may need.

It also starts the plugins specified in the
[policy file](@/docs/guide/config/policy-file.md) and confirms that all of them
started and received configuration successfully. This is likely to catch
configuration errors, though it will not expose issues that would only arise
during analysis with [`hc check`](@/docs/guide/cli/hc-check.md).

This is a very useful starting point when debugging Hipcheck. While Hipcheck
can only automatically check basic information like whether configured paths
are present and accessible, you should also review whether the paths `hc ready`
reports are the ones you intend for Hipcheck to use.

See the [`hc ready`](@/docs/guide/cli/hc-ready.md) documentation for more
information on its specific CLI.

## Checking API Tokens

Similarly, for any API tokens, it's good to make sure those tokens are valid
to use, and have the appropriate permissions required to access the
repositories or packages you are trying to analyze.
