---
title: "mitre/fuzz"
extra:
  nav_title: "<code>mitre/fuzz</code>"
---

# `mitre/fuzz`

Checks if a project participates in OSS Fuzz.

## Configuration

None

## Default Policy Expression

```
(eq $ #t)
```

## Default Query: `mitre/fuzz`

Returns `true` if the project _does_ participate in OSS Fuzz, `false` otherwise.

## Explanation

Repos being checked by Hipcheck may receive regular fuzz testing. This analysis
checks if the repo is participating in the OSS Fuzz program. If it is fuzzed,
this is considered a signal of a repository being lower risk.

## Limitations

* __Not all languagues supported__: Robust fuzzing tools do not exist for every
  language. It is possible fuzz testing was not done because no good option for it
  existed at the time. Lack of fuzzing in those cases would still indicate a higher
  risk, but it would not necessarily indicate bad software development practices.
* __Only OSS Fuzz checked__: At this time, Hipcheck only checks if the repo
  participates in Google's OSS Fuzz. Other fuzz testing programs exist, but a repo
  will not pass this analysis if it uses one of those instead.
