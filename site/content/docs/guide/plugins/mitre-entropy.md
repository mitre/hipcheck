---
title: "mitre/entropy"
extra:
  nav_title: "<code>mitre/entropy</code>"
---

# `mitre/entropy`

Identify the presence of textually unusual commits in a source repository's
history.

## Configuration

| Parameter           | Type     | Explanation   |
|:--------------------|:---------|:--------------|
| `langs-file`        | `String` | Path to a file specifying how to infer languages. |
| `entropy-threshold` | `Float`  | Threshold for a Z-score, above which a commit is considered "high entropy" |
| `commit-percentage` | `Float`  | Threshold for a percentage of "high entropy" commits permitted. |


## Default Policy Expression

```
(lte
  (divz
    (count (filter (gt {config.entropy-threshold or 10.0}) $))
    (count $))
  {config.commit-percentage or 0.0})
```

## Default Query: `mitre/entropy`

Returns an array of commit entropies for commits identified as impacting
likely source files.

## Explanation

Entropy analysis attempts to identify commits which contain a high degree of
textual randomness, in the believe that high textual randomness may indicate
the presence of packed malware or obfuscated code which ought to be assessed
for possible malicious content.

Entropy analysis works by determining the total number of occurrences for all
unicode graphemes which appear in a repository's Git diffs for commits which
include code. In then converts these occurence counts into frequencies based on
the total number of each individual grapheme divided by the total number of
all graphemes in the combined set of Git diffs. It also determines grapheme
frequencies for each commit individually. These individual and total grapheme
frequencies are then combined into a score as an individual frequency times
the log base 2 of the individual frequency divided by the total frequency.
These individual grapheme scores are then summed to produce a per-commit score,
which is normalized into a Z-score same as the churn metric. These entropy
values therefore represent how much the grapheme frequency map of a given
commit differs from the average set of grapheme frequencies across all commits.

Entropy cannot run if a repository contains only one commit (or only one commit
that affects a source file). Entropy analysis will always give an error when run
against a repo with a single commit.

## Limitations

* __Whether entropy surfaces malicious contributions is an open question__:
  We have ongoing work to confirm that entropy does help identify the presence
  of malicious contributions, and therefore is a useful metric for assessing
  supply chain risk against malicious contribution attacks, but at the
  moment this is an assumption made by Hipcheck.
* __Entropy's statistical calculations may be insufficient__: There is ongoing
  work to assess the statistical qualities of the entropy metric and determine
  whether it needs to be changed.
