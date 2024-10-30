---
title: Submit Chunking
weight: 10
slug: 0010
extra:
  rfd: 10
---

# Submit Chunking

We propose to update Hipcheck's gRPC protocol
to support `Submit` (outbound) query chunking. Currently, there is a query state
`ReplyInProgress` to indicate when a query response is one chunk in a series of
fragmented response messages, and that they should be combined on the receiving
side into a single message. The `ReplyComplete` state indicates that the current
reply message will not be followed by any additional chunks.

When initially designing the protocol we did not expect that there would be a
need for an analogous system on the outbound side, but after having run Hipcheck
against the Linux kernel we encountered such a need. This RFD proposes to rename
the existing `Submit` query state to `SubmitComplete` and to add another variant
to the `QueryState` enum called `SubmitInProgress`. According to the gRPC
documentation, renaming a field does not break backwards compatibility.

`Submit` query chunking will use the exact same chunking algorithm as `Reply`,
but will look for different `QueryState` variants.
