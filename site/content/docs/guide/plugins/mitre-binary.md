---
title: "mitre/binary"
extra:
  nav_title: "<code>mitre/binary</code>"
---

# `mitre/binary`

Identifies likely binary files in a source repository.

## Configuration

| Parameter      | Type      | Explanation   |
|:---------------|:----------|:--------------|
| `binary-file`  | `String`  | Path to a file specifying how to match binaries. |
| `opt-thresold` | `Integer` | The number of detect binaries to permit. |

## Default Policy Expression

```
(lte (count $) {config.opt-threshold or 0})
```

## Default Query: `mitre/binary`

Returns the number of detected binaries found.

## Explanation

Binary analysis searches through all of the files in the repository for binary
files (i.e. files not in readable text) that may contain code. There is a high
liklihood that these are deliberately malicious insertions. The precense of such
files could indicate the precense of malicious code in the repository and is a
cause for suspicion.

The analysis works by searching through the entire repository filetree. It
identifies all binary files and filters out files that are obviously not code
(e.g. images or audio files). If, after filtering, more binary files remain than
the configured thershold amount, the repository fails this analysis.

The analysis displays the internal filetree location of each suspicious binary file.
The user can then examine each file to determine if it is malicious or not.

## Limitations

* __Not all binary files may be malicious__: The repo may use certain binary
  files (beyond image and audio files) for legitimate purposes. This
  analysis does not investigate what the files do, only that they exist.

* __No additional information on binary files__: Hipcheck does not currently
  return any additional information about the suspcious files, only their
  locations in the repo filetree. The user must search for them manually if
  they wish to learn more about them.
