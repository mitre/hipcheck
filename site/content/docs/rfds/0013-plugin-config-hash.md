---
title: Plugin Configuration Hashing
weight: 13
slug: 0013
extra:
  rfd: 13
  primary_author: Julian Lanson
  primary_author_link: https://github.com/j-lanson
  status: Accepted
  pr: 1032
---

# Plugin Configuration Hashing

RFD12 proposed a combination of the policy file path and hash as a means to
uniquely identify the policy used in an analysis.  A substantial limitation of
this strategy is that hashing the policy file does not capture changes to any
plugin-specific config files that it references.

To address this, we propose to enable plugins to take part in determining if an
analyis' configuration is the same as previous ones by reporting their own
configuration hash.

Currently, the response object for the `set_configuration()` gRPC call that
configures a plugin is defined as follows:

```
message SetConfigurationResponse {
    // The status of the configuration call.
    ConfigurationStatus status = 1;
    // An optional error message, if there was an error.
    string message = 2;
}
```

The `message` field is currently only used for communicating an error. We
propose to update the semantics of this structure such that the `message` field
is expected also for a sucessful configuration attempt. That field's value
should be a lowercase hex string representing the SHA-256 hash of the
configuration. For plugins that take paths to custom config files as
configuration, the hash calculation should include the content of these files
such that changes to them result in a different hash.

The `hc` core should be updated to expect and save/return the configuration
hash of each plugin.

## Using Plugin Configuration Hashes in Report Caching

Now, the policy file hash and the plugin configuration hashes must be composed
to produce a single overarching "policy hash" for the analysis. To do so we will
order the policy file and configuration hashes lexicographically, concatenate
them, and hash the result with our chosen hash algorithm. This solution was
proposed in this [StackOverflow post][so-post]. In order for this scheme to be
consistent, the case of the hash digest alphabet characters must be also, hence
why we have stipulated above that the hex digests should contain lowercase
characters.

[so-post]: https://crypto.stackexchange.com/questions/54544/how-to-to-calculate-the-hash-of-an-unordered-set
