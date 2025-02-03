---
title: The Query Protocol
weight: 3
---

# The Query Protocol

Hipcheck core communicates with plugins over a [gRPC][grpc] channel using
[protobuf][protobuf]-encoded messages. After a sequence of initialization
messages, Hipcheck and the plugin establish a bi-directional query protocol on
top of the gRPC channel.  Plugin authors using a [Hipcheck SDK][rust-sdk] need not
understand the query protocol with any further level of detail; third-party
plugin authors who are eschewing the use of an SDK do need to understand the
query protocol, and it is toward these parties that this documentation is
directed.

This is the `Query` message object and associated `QueryState` enum that we will
explain in more detail below. For more information on the `proto3`
specification, see [here][proto3].

```protobuf
message Query {
    // The ID of the request, used to associate requests and replies.
    // Odd numbers = initiated by `hc`.
    // Even numbers = initiated by a plugin.
	int32 id = 1;

    // The state of the query, indicating if this is a request or a reply,
    // and whether it is complete or part of a series of chunks.
    QueryState state = 2;

    // Publisher name and plugin name. When a plugin receives a request-type
    // Query from Hipcheck, it can validate that the message was meant for
    // them with these fields.
    //
    // When a plugin makes a request to another plugin through Hipcheck, these
    // fields are used to indicate the target plugin.
    string publisher_name = 3;
    string plugin_name = 4;

    // The name of the query being made, so the responding plugin knows what
    // to do with the provided data.
    string query_name = 5;

    // The key for the query, as a JSON object. This is the data that Hipcheck's
    // incremental computation system will use to cache the response. A message
	// may contain zero or more of these fields.
    repeated string key = 6;

    // The response for the query, as a JSON object. This will be cached by
    // Hipcheck for future queries matching the publisher name, plugin name,
    // query name, and key. A message may contain zero or more of these fields.
    repeated string output = 7;

    // An unstructured concern raised during the query that will be raised
    // in the final Hipcheck report. A message may contain zero or more of
	// these fields.
    repeated string concern = 8;

    // Used to indicate whether or not a string field present in a `repeated string` field
    // was split between two messages
    bool split = 9;
}

enum QueryState {
    // Something has gone wrong.
    QUERY_STATE_UNSPECIFIED = 0;

    // We are completed submitting a new query.
    QUERY_STATE_SUBMIT_COMPLETE = 1;

    // We are sending a chunk of a response followed by more chunks.
    QUERY_STATE_REPLY_IN_PROGRESS = 2;

    // We are closing a reply to a query. If a query response is in one chunk,
    // just send this. If a query is in more than one chunk, send this with
    // the last message in the reply. This tells the receiver that all chunks
    // have been received.
    QUERY_STATE_REPLY_COMPLETE = 3;

    // We are sending a chunk of a request followed by more chunks.
    QUERY_STATE_SUBMIT_IN_PROGRESS = 4;
}
```

The `Query` struct is used for both requests and reponses in the Hipcheck plugin
system. In a request-type message, the `publisher_name`, `plugin_name` and
`query_name` identify the target endpoint to be queried with one or more `key`
fields. Each `key` field must be submitted to the endpoint separately. In a
response-type message, there must be an `output` field for each `key` field
received in the request, and in the corresponding order. As an additional
component in response messages, plugins may report zero or more `concern`
fields, which are unstructured strings that will be included in the Hipcheck
analysis report. `key`, `output`, and `concern` fields are all tagged as
`repeated` in the above protocol definition, meaning that a valid `Query`
message may contain zero, one, or multiple copies of each of these field. There
are additional contextual requirements on these fields specific to Hipcheck that
are described in this document.

Hipcheck expects that plugins may need to query other plugin endpoints in order
to complete requests made to them. Thus, it is valid for a plugin endpoint to
respond to a request-type message from Hipcheck with a request-type message of
their own. In allowing this, Hipcheck and the plugins must be able to maintain
the state of the request while the plugin waits for the response to its request.
Thus, each `Query` has an `id` field, which identifies the stateful "session" of
the message. When a plugin receives a request-type message with a new `id` from
Hipcheck core, all request- and response-type messages the plugin sends to
Hipcheck in service of answering this original request must use its same `id`
value.

## Chunking

The gRPC protocol has a default maximum message size of 4MB, which is sometimes
not sufficient to contain an entire request or response. Thus, the `Query`
struct supports chunking, or splitting a large request or response into multiple
consecutive messages under the 4MB limit that can be re-assembled by the
receiving party. This adds a layer of complexity to the message format. When
sending a chunked request-type message, all but the last message in the series
should have a `state` of `QUERY_STATE_SUBMIT_IN_PROGRESS`, whereas the last
message should have `QUERY_STATE_SUBMIT_COMPLETE`. The same is true for
response-type messages and `QUERY_STATE_REPLY_IN_PROGRESS` and
`QUERY_STATE_REPLY_COMPLETE`. Naturally, a single request or response message
that does not require chunking should use the appropriate `COMPLETE` state
variant.

A chunked `Query` message may have zero or more `key`, `output`, and `concern`
fields; in this way, these fields can be thought of arrays of zero or more
elements.  Chunking involves sending messages which contain ordered,
non-overlapping subarrays of the `key`, `output`, and `concern` fields in the
unchunked message. Once one element has been included in a chunked message, it
must not be included again.  Furthermore, we can consider the `key`, `output`,
and `concern` arrays as one larger ordering in that no element of `output` may be
sent before all elements of `key` are sent, and no element of `concern` may be
sent before all elements of `output` are sent. For example, from an original
`Query` that contains 4 keys, 4 output, and 8 concerns, we may send the
following chunked messages in order:
- One containing the all `key` elements and the first two `output` elements.
- One containing the remaining two `output` elements and six `concern` elements.
- One containing the remaining `concern` elements.

In some cases, a single element of one of these fields may exceed the 4MB limit.
To handle this case and to generally enable more efficient chunking, the `Query`
struct has a `split` field. When this field is set to `true`, it indicates that
the last element of `key`, `output` or `concern` is incomplete, and that the
contents of the first element of the next message should instead be appended to
that element. For example, if a chunked message contains `key: ['abcd', 'ef'],
split: true`, and the next message contains `key: ['gh', 'ijkl'], split: false`,
then the resulting `key` array should be `['abcd', 'efgh', 'ijkl']`. Because of
the aforementioned field ordering requirements, it will never be ambiguous
whether `split` refers to `key`, `output`, or `concern`. Note that when
splitting a string, the split **must** occur on a UTF-8 character boundary.

## Field Reference

### `id`

**In requests**: If part of an on-going session, must match the session's `id`.
Currently, plugins may not initiate requests that are not part of an on-going
session.

**In responses**: Must match the `id` value of the associated request.

### `state`

**In requests**: Must be `QUERY_STATE_SUBMIT_IN_PROGRESS` if another chunked
request message should be expected to follow, otherwise
`QUERY_STATE_SUBMIT_COMPLETE`.

**In responses**: If an unrecoverable error occurred during plugin execution,
must be `QUERY_STATE_UNSPECIFIED`; using this state lifts any requirements
stipulated in the fields below. Else, must be `QUERY_STATE_REPLY_IN_PROGRESS` if
another chunked request message should be expected to follow, otherwise
`QUERY_STATE_REPLY_COMPLETE`.

### `publisher_name`, `plugin_name`

**In requests**: Must have a valid value. Receiving plugins should expect that
these fields match their information.
**In responses**: May be left blank, ignored by Hipcheck.

### `query_name`

**In requests**: If field is an empty string, refers to plugin's default query
endpoint. If no query endpoint was specified in a `GetQuerySchemasResponse`
during the initialization step, this is an error.
**In responses**: May be omitted, ignored by Hipcheck.

### `key`

**In requests**: Must be at least one `key` field (pre-chunking).
**In responses**: Optional, but if included should match the order and length of
`output` fields (pre-chunking).

### `output`

**In requests**: Should not be included, ignored by Hipcheck.
**In responses**: Must be at least one `output` field, order and length
(pre-chunking) must match `key` fields of request-type message to which this
message is a response.

### `concern`

**In requests**: Should not be included, ignored by Hipcheck.
**In responses**: May include zero or more `concern` fields.

### `split`

In both request and response messages, may only be set to true if `state` is the
respsective `IN_PROGRESS` variant and at least one of `key`, `output`, and
`concern` have at least one element.

[grpc]: https://grpc.io/
[protobuf]: https://protobuf.dev/
[proto3]: https://protobuf.dev/programming-guides/proto3/
[rust-sdk]: @/docs/guide/making-plugins/rust-sdk.md
