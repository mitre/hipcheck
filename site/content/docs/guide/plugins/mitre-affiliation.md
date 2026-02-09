---
title: "mitre/affiliation"
extra:
  nav_title: "<code>mitre/affiliation</code>"
---

# `mitre/affiliation`

__Identifies project contributors affiliated with an organization of concern.__

What is an "organization of concern"? That depends on how you configure this
analysis! This analysis is considered using an "orgs file," a [KDL file][kdl]
that specifies two things:

1. What kinds of affiliation to consider concerning.
2. What organizations to match against when assessing affiliation.

You can view the [full documentation for the orgs file](#orgs-file) below.

## Usage

### Import

```kdl,hl_lines=3
plugins {
  // Add this to your `plugins` block, replacing `{version}` with the latest version. 
  plugin "mitre/affiliation" version="^{version}" manifest="https://hipcheck.mitre.org/dl/plugin/mitre/affiliation.kdl"
}
```

### Configuration

| Parameter         | Type      | Explanation   |
|:------------------|:----------|:--------------|
| `orgs-file-path`  | `String`  | Path to an "orgs file" specifying how to match affiliation. |
| `count-threshold` | `Integer` | The permitted number of concerning contributors.            |

For example:

```kdl,hl_lines=3 4,name=Hipcheck.kdl
analysis "mitre/affiliation" {
    // `#rel` indicates a file in the same directory as the policy file.
    orgs-file #rel("Orgs.kdl")
    count-threshold 0
}
```

### Default Orgs File

The default orgs file defines a list of well-known corporate entities such as
Amazon, Google, Huawei, and Red Hat) and flags any repo contributors that do
not have an email associated with one of these companies' domains. The ethos
is to try and capture unknown third-party contributors.

You can [view the default orgs file][default_orgs_file] within the Hipcheck
source repository.

### Default Policy Expression

```kdl
analysis "mitre/affiliation" policy="(lte $ {config.count_threshold})"
//                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

By default, this analysis will flag a target as having concerning contributors
if the number of flagged contributors exceeds the configured `count-threshold`.

## Explanation

### Purpose

The goal of this analysis is to enable Hipcheck users to identify
"concerning contributors" by organization.

Affiliation analysis tries to identify when commit authors or committers
may be affiliated or unaffiliated with some list of organizations.
This determination is based on the email address associated with authors or
committers on each Git commit, compared against a configured list of web hosts
associated with organizations.

The construction of the list is based on an "orgs file," whose path is provided
in the configuration of this form of analysis. This orgs file defines two
things: 1) a list of organizations, including web hosts associated with them,
and the name of the country to which they primarily belong, and 2) a "strategy"
for how the list of to-be-flagged hosts should be constructed and matched
against.

### Orgs File

The orgs file is split into two sections: the matching strategy and the list of
known organizations.

#### Matching Strategy

The strategy specifies how to determine whether a contributor matches one or
more known organizations.

A strategy includes:

- __Matching Mode__: a positional argument that can be...
  - `"affiliated"`: Flag contributors who are affiliated with the specified
    organizations.
  - `"independent"`: Flag contributors who are not affiliated with the
    specified organizations.
  - `"all"`: Flag all contributors.
  - `"none"`: Flag no contributors.
- __Org Matchers__: Child nodes that specify _what organizations to match
  against_ in the organizations list. If no org matchers are specified, the
  __matching mode__ will be applied to all organizations in the orgs file. Note
  that org matchers are additive: a contributor will be flagged if they match
  _any org matcher_. The possible org matchers are:
  - `country "<COUNTRY_NAME>"`: Match against all organizations from the
    identified country.
  - `org "<ORG_NAME>"`: Match against the specified organization.

For example, look at the following strategy:

```kdl
strategy "affiliated" {
    country "United States"
    org "MITRE"
}
```

This would flag contributors affiliated with MITRE or any organization from the
United States.

Alternatively, look at the following strategy:

```kdl
strategy "independent"
```

This would flag contributors who are _not_ affiliated with any of the
organizations in the orgs file. It matches against all organizations in the
orgs file because no org matchers are specified as child nodes of `strategy`.

#### Orgs List

This is a list of known organizations, including web hosts associated with
them and the name of the country to which they primarily belong.

For example, here is an excerpt of the default orgs file that ships with
a fresh installation of Hipcheck:

```kdl
orgs {
    org "AT&T" country="United States" {
        host "att.com"
    }
    org "Alibaba" country="China" {
        host "alibaba.com"
    }
    org "Amazon" country="United States" {
        host "amazon.com"
    }
}
```

As you can see, the orgs list is contained inside the `orgs` node, and each
`org` contains:

- __Name__: a positional string with a human-readable name for
  the organization.
- __Country__: a key-value argument indicating the country to which the
  organization primarily belongs.
- __Hosts__: a series of child nodes, each with a string containing a host name
  associated with the organization. These are what are used for matching
  against contributors based on Git commit identity data.

This orgs list is the basis for matching behavior specified in the `strategy`
node. If the `strategy` does not specify org matchers such as
`country "<COUNTRY_NAME>"` or `org "<ORG_NAME>"`, then the `strategy` matches
against the full list of organizations, according to the strategy mode.

### Limitations

#### The Orgs File Is Limited

The current construction requires the manual definition, in the "orgs file," of
companies, their associated web hosts, and their primary affiliated country.
This manual work is laborious, possibly error-prone, requires updating over
time, and is less complete than accessing more authoritative sources of
corporate information.

#### Git's Identity System Is Limited

Git's identity system is, by default, quite weak. Commit author or committer
data may be freely spoofed or filled with junk information which make
identifying the true author or committer of a commit impossible. Git _does_
support commit signing, to at least confirm that a commit has been authored by
the person who owns the relevant signing key. This signing can then be checked
against known signatures, including sources like Keybase which provide easy
distribution and checking of known signatures against known identities.
However, commit signing and checking of signatures incorporates the complexity
and limitations of cryptographic signatures as a technical mechanism for trust,
including questions of how to handle failures to sign, changes in keys or loss
of trust in previously trusted keys, and so on. This is an important issue, and
incorporation of commit signing information in Hipcheck is intended for the
future, but currently Hipcheck does not use commit signatures in any way. When
integrated into Hipcheck, the question of how to handle the very common case of
signatures _not_ being used would arise as well. How to resolve this here is an
open question.

#### The Default Configuration Might Not Be Right For You

Additionally, there is a question in the default configuration of this analysis
regarding whether to flag commits affiliated with organizations of concern, or
commits unaffiliated with any known organization. This question relies on
assumptions of the behavior of malicious actors in this context, and whether
malicious contributions would be made in commits authored or committed by those
using their corporate emails.

[kdl]: https://kdl.dev/
[default_orgs_file]: https://github.com/mitre/hipcheck/blob/main/config/Orgs.kdl
