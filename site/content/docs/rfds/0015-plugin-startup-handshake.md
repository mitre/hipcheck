---
title: Plugin Startup Handshake
weight: 15
slug: 0015
extra:
  rfd: 15
  status: Proposed
---

# Plugin Startup Handshake

This RFD replaces the port-assignment portion of [RFD #4][rfd_4]. It proposes
a cross-platform startup handshake in which a plugin selects its own loopback
port and reports readiness to Hipcheck over standard output. It preserves the
existing gRPC query protocol.

## Summary

Today, Hipcheck selects a TCP port, passes it as `--port <PORT>`, and then
tries to connect. The core cannot reserve that port while the child is starting:
another process can claim it after Hipcheck checks it and before the plugin
binds it. In addition, the configured port range is not a meaningful contract
when the OS selects an ephemeral port.

Under the proposed protocol, Hipcheck starts a compatible plugin with
`--hipcheck-control=stdio` and a per-launch nonce. The plugin binds
`127.0.0.1:0`, starts serving gRPC, and writes one machine-readable readiness
record to stdout. Hipcheck validates the record, connects to the reported port,
and proves that it reached the launched plugin by completing a nonce-bound
startup RPC. Only then is the plugin considered started.

SDK users will not need to implement the handshake themselves. A non-SDK
plugin needs to add one small CLI behavior and one RPC implementation.

## Goals

- Remove Hipcheck's check-then-release port assignment race.
- Work on Windows, macOS, and Linux without platform-specific plugin code.
- Keep third-party plugins language-neutral and straightforward to implement.
- Give Hipcheck a deterministic readiness result, child-exit diagnostic, and
  bounded startup timeout.
- Prevent accidental connection to a stale or unrelated local plugin process.
- Preserve compatibility with installed plugins during a staged migration.

## Non-goals

- Sandboxing a malicious plugin. A plugin already executes with the invoking
  user's authority; the launch nonce is a correctness check, not an isolation
  boundary.
- Changing the existing query or configuration semantics.
- Requiring a particular runtime, SDK, shell, or IPC implementation.
- Supporting remote plugin processes in this protocol version.

## Design

### Launch contract

For handshake-capable plugins, Hipcheck starts the manifest entrypoint with:

```text
--hipcheck-control=stdio
--hipcheck-launch-nonce=<base64url-encoded-32-random-bytes>
```

Hipcheck pipes stdout, pipes stderr, and does not pass `--port`. It continues
to forward stderr as plugin logs. Stdout is reserved exclusively for control
records for the life of this mode; plugins must not write diagnostics, logs, or
application output there. Hipcheck should treat any non-control stdout before
the ready record as a protocol error, include a bounded escaped excerpt in its
diagnostic, then terminate the child.

The nonce is generated afresh for every spawn attempt, is not logged, and is
provided only in the child command line. Command-line visibility means it is
not a secret from same-user processes; it binds Hipcheck's readiness check to
the child it intended to launch and detects stale or unrelated listeners.

### Readiness record

After binding and starting its gRPC server, a plugin writes exactly one UTF-8
JSON object followed by `\n` to stdout, flushes it, and keeps the stream open:

```json
{"hipcheck_control":1,"event":"ready","port":51342}
```

The record is limited to 4 KiB. Version 1 requires these fields:

| Field | Type | Requirement |
| --- | --- | --- |
| `hipcheck_control` | integer | Must equal `1`. |
| `event` | string | Must equal `"ready"`. |
| `port` | integer | Must be in `1..=65535`. |

Unknown fields are ignored to permit additive changes. A plugin must bind the
reported port to IPv4 loopback (`127.0.0.1`), not `0.0.0.0` or a public
interface. The SDKs must check both bind success and the actual local address
before writing readiness. Hipcheck connects only to `127.0.0.1:<port>`.

A future control-protocol version may add records after readiness. Version 1
has no such records; Hipcheck closes its stdout read handle after accepting the
ready record. This prevents unrelated stdout data from being interpreted as
control traffic.

### Authenticated readiness RPC

Transport connection alone is not proof that the launched plugin owns the
reported port. Before accepting readiness, Hipcheck invokes this new plugin
service method:

```proto
message StartupHandshakeRequest {
  // Exact value passed in --hipcheck-launch-nonce.
  string launch_nonce = 1;
}

message StartupHandshakeResponse {
  // Protocol revision understood by this plugin; initially 1.
  uint32 control_protocol_version = 1;
  // Canonical plugin identifier from the plugin manifest, e.g. "mitre/git".
  string plugin_id = 2;
}

service PluginService {
  rpc StartupHandshake(StartupHandshakeRequest)
      returns (StartupHandshakeResponse);
}
```

The plugin stores the nonce received at launch. `StartupHandshake` succeeds
only when the request nonce matches it, and returns its compiled or manifest
plugin identity. Hipcheck requires version `1` and compares `plugin_id` with
the resolved manifest identity. A mismatch is a startup failure. This is also
the first real gRPC request, avoiding a false-ready result from a lazy client
connection.

The server may reject repeated handshakes after the first successful one. The
nonce must never be exposed by other RPCs or included in errors or logs.

### State machine and failure handling

```text
spawn child -> wait for one ready record -> StartupHandshake RPC -> ready
                    |                         |
                    +-- timeout/exit/error ----+--> kill, reap, retry
```

Hipcheck uses one configurable startup deadline per spawn attempt. It races
three events: child exit, receipt of a complete stdout line, and deadline
expiry. It rejects malformed JSON, oversized lines, missing or invalid required
fields, invalid ports, early EOF, child exit, and failed/mismatched handshakes.
For every rejected attempt Hipcheck kills and reaps the child before retrying;
it must not leave a failed process or stderr-forwarding task behind.

The current `max-spawn-attempts` remains the number of complete attempts. The
current connection backoff settings are retired for handshake-capable plugins:
the readiness record is emitted only after the server is listening. A separate
`startup-timeout` configuration value replaces their timing role. The initial
default should be conservative and platform-independent (for example, 30
seconds); exact default and configuration syntax are implementation details to
settle with the implementation PR.

### Why stdout control records

Inherited standard streams are available to normal child processes on all
supported host platforms and are supported by essentially every language.
They require no path naming, handle-passing API, permissions model, or
platform-specific dependency from plugin authors. Reserving stdout and using
stderr for logs creates an unambiguous, testable boundary.

The alternative of passing a pre-bound TCP listener is stronger only if the
listener can be transferred without closing it. That requires different handle
inheritance rules and APIs on Unix and Windows and is awkward for plugins
written without an SDK. The proposed design removes port selection races while
keeping the only required plugin operations—bind, write one JSON line, and
serve one RPC—portable.

## Required changes

### Hipcheck core

- Add control-protocol capability metadata to `plugin.kdl`; see
  [Compatibility and manifest changes](#compatibility-and-manifest-changes).
- Add generated types and client support for `StartupHandshake` to the common
  protobuf crate and all current protocol consumers.
- Replace `PluginExecutor::get_available_port()` and `--port` injection for
  handshake-capable plugins with stdout control parsing and nonce generation.
- Pipe stdout instead of inheriting it, preserve stderr log forwarding, and
  concurrently monitor stdout, child exit, and deadline.
- Perform `StartupHandshake` before constructing `PluginContext`; record the
  reported port only as diagnostic/runtime metadata.
- Remove the port range from executor configuration once legacy support ends.
- Add integration tests for success, malformed and non-control stdout, bind
  failure, no readiness, early exit, deadline, wrong port, wrong nonce, wrong
  identity, retry cleanup, and concurrent startup of many plugins.

### Rust SDK

- Add a `StartupControl` parsed from the two new CLI arguments.
- Keep the existing public `PluginServer::listen_local` behavior for legacy
  mode. In control mode, expose a new SDK-owned `run`/`listen_for_hipcheck`
  path that binds loopback port `0`, starts serving, emits and flushes the
  readiness record, and implements `StartupHandshake`.
- Update the generated service implementation to retain the nonce privately
  and return the plugin's canonical identity and control protocol version.
- Update the SDK's standard plugin argument helper and all in-tree Rust
  plugins so typical plugin authors do not write protocol code.
- Provide tests that launch a plugin through captured stdio on every supported
  target in CI where available.

### Python SDK

- Add the same control arguments to `get_parser_for` and preserve `--port` for
  legacy mode.
- Update `PluginServer.listen` (or introduce a clearly named wrapper) to bind
  `127.0.0.1:0`, obtain the assigned port, start gRPC, write and flush the
  readiness record, and retain the nonce.
- Generate/implement `StartupHandshake` with the same nonce, identity, and
  version checks as the Rust SDK.
- Keep stdout control-only and route SDK/plugin logging to stderr.
- Add subprocess integration tests for successful and failed handshakes.

### Plugins without an SDK

A third-party plugin can support the new protocol in any language by doing the
following:

1. Parse `--hipcheck-control=stdio` and `--hipcheck-launch-nonce=...`.
2. Bind the gRPC server to `127.0.0.1:0`; retrieve the assigned port.
3. Start accepting requests before emitting readiness.
4. Write the exact version-1 JSON readiness record and newline to stdout, then
   flush stdout. Send logs only to stderr.
5. Implement `StartupHandshake` to compare the supplied nonce exactly, return
   control protocol version `1`, and return the plugin's canonical ID.
6. Continue serving until Hipcheck terminates the process or the plugin's
   existing shutdown behavior applies.

The published plugin-author guide will include language-agnostic pseudocode,
wire-format examples, and a small conformance-test executable. The test will
launch the plugin, validate the stdout record, call the RPC, and exercise common
failure diagnostics without requiring an SDK.

## Compatibility and manifest changes

The manifest must declare startup protocol support, rather than relying on
failed invocation to discover it. This RFD proposes an additive field:

```kdl
startup protocol="stdio-v1"
```

The absence of `startup` means legacy `port-v0`. Hipcheck selects `stdio-v1`
when declared and otherwise uses the current `--port` behavior. A future
manifest revision may express a supported-protocol list if negotiation becomes
necessary; one explicit value is sufficient for this migration.

Hipcheck releases the feature in these phases:

1. **Foundation.** Land protobuf support, core implementation behind the
   manifest field, Rust and Python SDK support, conformance tests, and updated
   documentation. Existing plugins continue unchanged.
2. **First-party migration.** Update every first-party plugin manifest to
   `startup protocol="stdio-v1"` and release compatible artifacts. Exercise
   both Windows and Unix integration coverage before changing defaults.
3. **Third-party adoption.** Publish an implementation guide and conformance
   test. Plugin maintainers release `stdio-v1` artifacts and update manifests.
   Hipcheck continues supporting `port-v0` for at least two Hipcheck minor
   releases after the first stable SDK releases.
4. **Deprecation.** Warn once per legacy plugin launch with the manifest
   identity and documentation link. Do not warn for a plugin that cannot be
   upgraded because its manifest remains pinned, unless users opt in to a
   stricter mode.
5. **Removal.** In the next documented breaking major release, remove
   `port-v0`, port-range configuration, and `--port` injection. Hipcheck gives
   a clear error identifying legacy artifacts and the last compatible core
   release. The project should announce the target major release no later than
   the deprecation phase.

This field changes the plugin launch interface but not policy-file syntax or
plugin discovery. Plugin authors must publish a new artifact and manifest
version because an older artifact cannot obey the new launch contract.

## Alternatives considered

### Keep core-selected ports with retries

Retries make collisions less likely but cannot eliminate check-then-bind races.
They also obscure readiness failures and do not make a selected port an owned
resource. Rejected.

### Plugin reports a port without a handshake RPC

This fixes port assignment but Hipcheck could connect to an unrelated process
if the plugin exits after emitting its port and another listener reuses it.
The explicit nonce-bound RPC is small and closes this correctness gap.

### A Unix socket or named pipe for control

These mechanisms are excellent platform-native IPC, but paths, permissions,
and APIs differ materially across Windows and Unix. Requiring them would make
non-SDK plugins harder to author. They remain possible future transports, but
are not the portable baseline.

### Hipcheck passes an already-bound listener to the plugin

This can eliminate the remaining plugin-side bind race, but requires reliable
descriptor/handle inheritance across Unix and Windows, plus language-specific
adoption. It is unsuitable as the universal plugin contract and would not
improve the normal user experience enough to justify that burden.

### Use stdout for logs and a separate control channel

This reverses the conventional stdout-data/stderr-diagnostics split and still
requires a cross-platform control transport. Reserving stdout is simpler and
matches the current expectation that plugin logs use stderr. Rejected.

## Security and operational considerations

The protocol does not make untrusted plugin execution safe. The manifest
identity comparison and nonce prevent accidental wiring to the wrong local
service, while loopback binding prevents exposing an unauthenticated plugin
API to the network. Hipcheck must cap control-record size, redact nonce values
from diagnostics, and always reap failed children. The control parser must not
block stderr draining, since a verbose plugin can otherwise deadlock on a full
stderr pipe before it becomes ready.

## Open questions

- Should `StartupHandshake` become a general `GetPluginInfo` method used after
  startup, or remain narrowly scoped to launch verification?
- What exact `startup-timeout` default best accommodates container-backed
  plugins without making ordinary failures slow?
- Should a strict mode reject all legacy plugins before the breaking release?
- Should the conformance tester be bundled with `hc`, published as a separate
  binary, or both?

[rfd_4]: ./0004-plugin-api.md
