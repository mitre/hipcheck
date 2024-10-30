---
title: Batched Plugin Queries
weight: 9
slug: 0009
extra:
  rfd: 9
---

# Batched Plugin Queries

Now that we've made substantial progress in converting legacy Hipcheck analysis
implementations to plugins, we've identified the need for a way to "batch"
queries for performance reasons. For example, the entropy and churn analyses
rely on a "linguist" functionality that evaluates whether a given file in a repo
contains source code. The plugin design philosophy would dictate that we
separate the `linguist` functionality into its own plugin, with an endpoint
`is_likely_source_file(&Path) -> bool`, and have churn and entropy each query
that. But making a `gRPC` request from the `entropy` plugin for each file in a
potentially large repository is likely to incur runtime costs.

Plugins can expose query endpoints that take a `Vec<_>` of objects and return a
`Vec<_>` of reponses, but because `salsa` memo-ization operates on the entire
query object, it would cache the entire `Vec<_>` as a key, and therefore any
future `Vec`-based queries would not benefit from the memo-ization unless they
were the exact same `Vec` in terms of size and order of elements.

## Proposed Protocol Change

The current definition of a `Query` gRPC message in the Hipcheck v1 protocol
includes the following fields:

```protobuf
message Query {
	...

    // incremental computation system will use to cache the response.
    string key = 6;

    // The response for the query, as a JSON object. This will be cached by
    // Hipcheck for future queries matching the publisher name, plugin name,
    // query name, and key.
    string output = 7;

    // Concern chunking is the same as other fields.
    repeated string concern = 8;
}
```

We propose augmenting the `key` and `output` fields into `repeated` fields.
From a gRPC perspective, this means that multiple `key` and `output` fields can
appear in each message. Compiling this `protobuf` definition into a Rust
language `struct` will have the effect of replacing `key: String` with `key:
Vec<String>` and the same for `output`.

According to the `proto3` [language guide][proto3]:

> "For string, bytes, and message fields, singular is compatible with repeated.
> Given serialized data of a repeated field as input, clients that expect this
> field to be singular will take the last input value if it’s a primitive type
> field or merge all input elements if it’s a message type field. Note that this
> is not generally safe for numeric types, including bools and enums. Repeated
> fields of numeric types are serialized in the packed format by default, which
> will not be parsed correctly when a singular field is expected."

Therefore making this change should not break compatibility with any plugins
compiled using the existing protocol definition.

## Protocol Changes to Support Chunking Algorithm

Since gRPC has a maximum message size, in Hipcheck core and SDK we use a
chunking algorithm to break down messages estimated to be too large into a
vector of acceptable "chunks" sent in serial.

Without the above proposed changes, `key` and `output` are `String` fields. It
is easy to know how to compose the `key` field of two chunked message, you just
append the content of the second message's `key` field to that of the first
message. But when `key` becomes a `Vec`, combining two `key` fields becomes
ambiguous. To maximize space usage, we may "split" up an element of the `Vec`
and send half in the first message, half in the second. If we naively `extend`
the `Vec`s, we'll fail to re-assemble the split element. To notify when the
first element of `key` in a subsequent message should be combined with the last
element in the `key` aggregator, we need to add a field to the `Query` struct
definition in protobuf:

```
	bool split = 9;
```

Whenever we split a vec element, we will set this field to `true` in the same
message. The recepient will note the "latest" `Vec` field to have content
(in descending order: `key`, `output`, `concern`), and note that the next
message chunk should have a first element in that field appended to the last of
the current message. For example:

```
{
	key: vec!["abc", "def"]
	output: vec!["hi"]
	concern: vec![]
	split: true
}
```

Upon receiving the above message, the recipient will expect to append the first
element of the next message's `output` vector to `"hi"`.

## Associated Change to Querying Plugins from Rust SDK

With the above protocol changes, under the hood the `key` field of a query
object will be a `Vec<String>` instead of a singular `String`.

We propose to leave the current `PluginEngine::query(key: Value)` function API
intact. Under the hood we will simply wrap `key` into a single-element vector.
We will add a second function `PluginEngine::batch_query(keys: Vec<Value>)`
which will not do any additional wrapping of the `keys` field and insert it
as-is into the protobuf `Query` struct once each key has been JSON-serialized.

We propose also to create a `PluginEngine::batch(target: String)` function,
which returns an instance struct that exposes a `query(key: Value)` function.
This struct instance will aggregate `key` values to use as separate queries to a
given `target` query endpoint, and send them all as a single batched query when
the instance goes out of scope.

## Associated Change to Hipcheck Core

Since we want to make full use of `salsa` memo-ization, when the Hipcheck core
query engine receives a request from a plugin to another plugin that contains a
`key: Vec<String>` with length greater than one, it will split out each element
and make a separate `salsa` request for it.

As a result, **no plugin should expect to receive a query where the `key` field
has more than one element**, and doing so should be considered an error.

Once the result of querying a plugin on each element of `key` is finished,
Hipcheck core will package the results into an array of equal length where the
result at each index corresponds with the key value each each index of `key`,
and return the generated `QueryResponse` object to the requesting plugin.

[proto3]: https://protobuf.dev/programming-guides/proto3/
