---
title: "mitre/affiliation"
extra:
  nav_title: "<code>mitre/affiliation</code>"
---

# `mitre/affiliation`

Identifies project contributors affiliated with an organization of concern.

## Configuration

| Parameter         | Type      | Explanation   |
|:------------------|:----------|:--------------|
| `orgs-file-path`  | `String`  | Path to an "orgs file" specifying how to match affiliation. |
| `count-threshold` | `Integer` | The permitted number of concerning contributors.            |

## Default Policy Expression

```
(lte $ {config.count_threshold})
```

## Default Query: `mitre/affiliation`

Returns the number of commits flagged for having concerning contributors.

## Explanation

Affiliation analysis tries to identify when commit authors or committers
may be affiliated or unaffiliated with some list of organizations.
This determination is based on the email address associated with authors or
committers on each Git commit, compared against a configured list of web hosts
associated with organizations of concern.

The construction of the list is based on an "orgs file," whose path is provided
in the configuration of this form of analysis. This orgs file defines two
things: 1) a list of organizations, including web hosts associated with them,
and the name of the country to which they primarily belong, and 2) a "strategy"
for how the list of to-be-flagged hosts should be constructed.

The strategy defines the list of organizations to be included in the list of
those considered when checking affiliation, and whether the analysis should
flag commits from those _affiliated_ with the list of organizations, or
_independent_ from the list of organizations (for completeness, it also
permits _all_ or _none_, which would flag all commits, or none of them).

If the `strategy` key is used in the configuration, then all organizations
listed in the "orgs file" are implicitly included in the list of organizations
to consider.

If the `strategy_spec` table is used, then `strategy_spec.mode` and
`strategy_spec.list` keys must be defined. The `strategy_spec.mode` key accepts
the same set of values (`affiliated`, `independent`, `all`, or `none`) as the
`strategy` key, while `list` accepts an array of strings in one of two forms:
`"country:<country_name>"` or `"org:<org_name>"`. The first form will include
in the list of organizations all those organizations which are associated with
the named country, while the second form will include in the list a single
organization with the given name.

To illustrate this, imagine the following strategy specification:

```toml
[strategy_spec]
mode = "affiliated"
kind = ["country:United States", "org:MITRE"]
```

This strategy spec would flag any commits those authors or committers can be
identified as being affiliated with any American company listed in the file or
with MITRE specifically.

## Limitations

* __The orgs file is limited__: The current construction requires the manual
  definition, in the "orgs file," of companies, their associated web hosts, and
  their primary affiliated country. This manual work is laborious, possibly
  error-prone, requires updating over time, and is less complete than accessing
  more authoritative sources of corporate information.
* __Limits in git's identity system__: Git's identity system is, by default,
  quite weak. Commit author or committer data may be freely spoofed or filled
  with junk information which make identifying the true author or committer of
  a commit impossible. Git _does_ support commit signing, to at least confirm
  that a commit has been authored by the person who owns the relevant signing
  key. This signing can then be checked against known signatures, including
  sources like Keybase which provide easy distribution and checking of known
  signatures against known identities. However, commit signing and checking of
  signatures incorporates the complexity and limitations of cryptographic
  signatures as a technical mechanism for trust, including questions of how to
  handle failures to sign, changes in keys or loss of trust in previously
  trusted keys, and so on. This is an important issue, and incorporation of
  commit signing information in Hipcheck is intended for the future, but
  currently Hipcheck does not use commit signatures in any way. When
  integrated into Hipcheck, the question of how to handle the very common
  case of signatures _not_ being used would arise as well. How to resolve
  this here is an open question.
* __Questions about the best default configuration__: Additionally, there is a
  question in the default configuration of this analysis regarding whether to
  flag commits affiliated with organizations of concern, or commits
  unaffiliated with any known organization. This question relies on assumptions
  of the behavior of malicious actors in this context, and whether malicious
  contributions would be made in commits authored or committed by those using
  their corporate emails.
