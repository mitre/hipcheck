---
title: hc cache
extra:
  nav_title: "<code>hc cache</code>"
---

# `hc cache`

`hc cache` is a command for users to manage Hipcheck's data cache.

When Hipcheck runs with `hc check`, one of its first operations is to resolve
the target of analysis from the target specifier provided by the user. A
resolved target must include a Git repository, as that's the basis for most
kinds of analysis we want to run with Hipcheck, analyzing the behaviors
associated with the development of the software in question.

After that Git repository is identified, it's cloned into Hipcheck's local
repository cache, so that any operations which need the Git metadata can run
on a local copy of that data instead of operating over the network in the case
of a remote repo. Note that Hipcheck creates a copy in the local repository
cache even if the target of analysis is a local repo. This is to ensure that
any analysis operations which may change the state of the repo, but example
by checking out a different commit, branch, or tag, don't modify the existing
repository on disk.

Over time, this local cache of repositories can grow large, as Hipcheck does
not do any automation cleanup of prior repositories stored there. This is
intended to make it easier to re-analyze existing repositories, as Hipcheck
will merely pull the latest changes from a repository which has been
analyzed before and remains in the repository cache.

The following is the CLI help text for `hc cache`:

```
Manage Hipcheck cache

Usage: hc cache [OPTIONS] <COMMAND>

Commands:
  list    List existing caches
  delete  Delete existing caches
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help (see more with '--help')

Output Flags:
  -v, --verbosity <VERBOSITY>  How verbose to be [possible values: quiet, normal]
  -k, --color <COLOR>          When to use color [possible values: always, never, auto]
  -f, --format <FORMAT>        What format to use [possible values: json, human]

Path Flags:
  -C, --cache <CACHE>    Path to the cache folder
  -p, --policy <POLICY>  Path to the policy file
```

As shown, this allows the user to list the items currently found in the cache,
and to delete specific items.

## `hc cache list`

The following is the help text for `hc cache list`:

```
List existing caches

Usage: hc cache list [OPTIONS]

Options:
  -s, --strategy <STRATEGY>  Sorting strategy for the list, default is 'alpha' [default: alpha] [possible values: oldest, newest, largest, smallest, alpha, ralpha]
  -m, --max <MAX>            Max number of entries to display
  -P, --pattern <FILTER>     Consider only entries matching this pattern
  -h, --help                 Print help (see more with '--help')

Output Flags:
  -v, --verbosity <VERBOSITY>  How verbose to be [possible values: quiet, normal]
  -k, --color <COLOR>          When to use color [possible values: always, never, auto]
  -f, --format <FORMAT>        What format to use [possible values: json, human]

Path Flags:
  -C, --cache <CACHE>    Path to the cache folder
  -p, --policy <POLICY>  Path to the policy file
```

This by default lists all entries found in the repository cache. Those entries
can be filtered, sorted, and a maximum number to show can be set. The pattern
defines a prefix pattern to search for when filtering repositories. The
strategy defines how sorting should be done, and supports the following
options:

| Strategy   | What It Does                   |
|:-----------|:-------------------------------|
| `oldest`   | Sort from oldest to newest.    |
| `newest`   | Sort from newest to oldest.    |
| `largest`  | Sort from largest to smallest. |
| `smallest` | Sort from smallest to largest. |
| `alpha`    | Sort alphabetically.           |
| `ralpha`   | Sort reverse-alphabetically.   |

## `hc cache delete`

`hc cache delete` is for deleting entries from the repository cache. The
help text for it is:

```
Delete existing caches

Usage: hc cache delete [OPTIONS]

Options:
  -s, --strategy <STRATEGY>...  Sorting strategy for deletion. Args of the form 'all|{<STRAT> [N]}'. Where <STRAT> is the same set of strategies for `hc cache list`. If [N], the max number of entries to delete is omitted, it will default to 1
  -P, --pattern <FILTER>        Consider only entries matching this pattern
      --force                   Do not prompt user to confirm the entries to delete
  -h, --help                    Print help (see more with '--help')

Output Flags:
  -v, --verbosity <VERBOSITY>  How verbose to be [possible values: quiet, normal]
  -k, --color <COLOR>          When to use color [possible values: always, never, auto]
  -f, --format <FORMAT>        What format to use [possible values: json, human]

Path Flags:
  -C, --cache <CACHE>    Path to the cache folder
  -p, --policy <POLICY>  Path to the policy file
```

The same `pattern` and `strategy` flags apply to this command. By default it
will prompt the user to confirm before deleting; this can be overriden with the
`--force` flag.
