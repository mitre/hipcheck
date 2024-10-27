---
title: "mitre/activity"
extra:
  nav_title: "<code>mitre/activity</code>"
---

# `mitre/activity`

Determines if a project is actively maintained.

## Configuration

| Parameter | Type      | Explanation   |
|:----------|:----------|:--------------|
| `weeks`   | `Integer` | The permitted number of weeks before a project is considered inactive. |

## Default Policy Expression

```
(lte $ P{config.weeks or 71}w)
```

## Default Query: `mitre/activity`

Returns a `Span` representing the time from the most recent commit to now.

## Limitations

* __Cases where lack of updates is warranted__: Sometimes work on a piece of
  software stops because it is complete, and there is no longer a need to
  update it. In this case, a repository being flagged as failing this analysis
  may not be truly risky for lack of activity. However, _most of the time_
  we expect that lack of updates ought to be concern, and so considering this
  metric when analyzing software supply chain risk is reasonable. If you
  are in a context where lack of updates is desirable or not concerning, you
  may consider changing the configuration to a different duration, or disabling
  the analysis entirely.
