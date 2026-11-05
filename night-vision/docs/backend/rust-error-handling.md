
# Rust Error Handling

This guide describes how backend Rust code should define, wrap, report, and
log errors in Night Vision. It is practical project guidance for contributors
adding server code, backend tools, database code, or background workers.

It adapts useful prior art from Oxide's
[Defining Error Types and Logging Errors][oxide-error-types] guide, but this
document is the Night Vision policy. Where this guide and existing Night Vision
docs disagree, prefer the more specific Night Vision document for the area
being changed.

[[_TOC_]]

## Core Rules

- Add context at the layer that knows what operation failed.
- Preserve the source chain with `std::error::Error::source()` when wrapping a
  lower-level error.
- Do not make an outer error's `Display` repeat its source error text.
- Print or log full error chains at diagnostic boundaries.
- Keep user-facing messages specific enough to act on, but safe to show.
- Do not expose secrets, credentials, stack traces, or sensitive resource
  existence details in API responses, logs, CLI output, or documentation
  examples.

## Error Types

Use distinct error types when callers or reviewers need to understand or act on
specific failure modes. Good reasons to create a typed error include:

- The caller needs to match on variants and choose different recovery paths.
- The error crosses a crate or module boundary.
- The error is part of startup behavior, API behavior, database setup, or other
  code reviewers need to audit carefully.
- The type can attach useful, secret-safe context that the source error does
  not have.

`backend/nv-server/src/error.rs` is the main example for the server. Its
`FatalError` enum separates failures such as config loading, database
connection, runtime construction, OpenAPI writing, logger initialization, and
Dropshot startup. Each variant's `Display` text describes the operation that
failed, while `source()` preserves the lower-level cause.

`backend/nv-common/src/db.rs` is a smaller example. `DatabaseConnectionError`
distinguishes connection failures from migration failures, while preserving the
underlying `sea_orm::DbErr`.

Use `anyhow` when the error is for an application or developer-tool boundary
where callers will not match on variants. `nvdb` uses `anyhow::Result` for CLI
commands because failures are reported to the operator and then the process
exits. Even with `anyhow`, add context at the call site:

```rust
command
    .status()
    .context("failed to run sea-orm-cli migrate")?;
```

Do not use `anyhow` in reusable backend APIs when callers may need typed
matching, structured recovery, or precise test assertions.

## Wrapping Errors

When wrapping a lower-level error, make the wrapper describe the operation that
failed and keep the lower-level error as the source. The outer message should
not duplicate the inner message.

Prefer this shape:

```rust
impl std::fmt::Display for DatabaseConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect(_) => write!(f, "failed to connect to database"),
            Self::Migrate(_) => write!(f, "failed to run database migrations"),
        }
    }
}

impl std::error::Error for DatabaseConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Connect(err) | Self::Migrate(err) => Some(err),
        }
    }
}
```

Avoid this shape:

```rust
write!(f, "failed to connect to database: {err}")
```

Repeating `{err}` in the outer `Display` looks useful when someone prints only
the outer error, but it breaks full-chain printing by producing duplicated or
misleading output. This follows the same rule explained in Oxide's error-types
guide: outer errors should describe their own context; chain-aware formatting
should add sources.

Use `#[from]` carefully when deriving errors with `thiserror`. It is fine when
the wrapped error already carries all useful context or when the variant is
transparent. Do not use `#[from]` merely to make `?` shorter if doing so loses
the path, command, field name, package ID, request context, or operation that
would have made the failure understandable. Use explicit `map_err` or
`anyhow::Context` when the current layer can add useful context.

## Printing And Logging

Naively printing an error with `{err}` shows only the outer `Display` message.
That is acceptable for short user-facing status text only when the outer
message is enough. It is not enough for diagnostics.

At diagnostic boundaries, include the full source chain:

- For project typed errors, use `ErrorSourceIterator::sources_iter()` from
  `backend/nv-common/src/error.rs` or another chain-aware formatter.
- For `anyhow::Error`, use `{err:?}` for a multiline chain or `{err:#}` for a
  single-line chain.
- When adding structured logging later, use a chain-aware value rather than
  logging only `%err`.

`FatalError` and `ConfigLoadError` already implement `Debug` in the expected
style: a top-level message followed by a `Caused by:` list. Keep new fatal or
startup errors consistent with that output.

Do not solve missing source-chain output by putting the source error into every
outer `Display` message. Fix the printing or logging site instead.

## User-Facing Messages

User-facing messages should be short, actionable, and appropriate for their
audience.

For API clients:

- Return messages that help correct the request when that is safe.
- Do not expose internal stack traces, database errors, filesystem paths,
  credentials, tokens, or sensitive resource-existence details.
- Prefer stable status-code behavior over detailed internal diagnostics.
- Use the request ID to connect client reports to server-side diagnostics.

For CLI users:

- Say what command or operation failed.
- Add enough context to fix the local problem, such as the tool that could not
  be started or the config file that could not be parsed.
- Do not print real database URLs, secret values, process environments, or
  shell traces containing credentials.

For logs:

- Include the operation, object identifier, and retry or recovery decision when
  useful.
- Include full source chains for operator diagnostics.
- Treat logs as potentially persistent and widely visible; secret-safety rules
  still apply.

## HTTP Errors

Night Vision REST handlers return Dropshot `HttpError` for request-time
failures. Choose status codes using [HTTP Status Codes](./http-status-codes.md)
and keep this guide focused on error construction and reporting.

Use `HttpError` for errors that should become HTTP responses. For example,
`backend/nv-server/src/api.rs` returns `HttpError::for_not_found` when a
package-source ID is unknown:

```rust
Err(HttpError::for_not_found(
    None,
    format!("unknown package source {id}"),
))
```

That message is acceptable because the API is already answering a lookup for
the submitted ID and the message helps the caller correct the request.

Do not convert internal failures directly into detailed client messages. Map
internal errors to the correct status code and a safe message, while logging
the full internal chain server-side. For expected client mistakes, return a
`4xx` response. For bugs or unhandled server conditions, return `500 INTERNAL
SERVER ERROR`. For dependency unavailability, prefer `503 SERVICE UNAVAILABLE`
when that is the accurate condition.

Framework-generated failures, such as malformed path parameters or malformed
JSON, may be handled by Dropshot before the handler runs. Do not duplicate
framework behavior unless the API needs a more specific response or recovery
path.

## Fatal Startup Errors

Use fatal errors only when the server cannot start or cannot continue to serve
requests. `FatalError` covers this boundary for `nv-server`.

Current fatal categories include:

- Loading configuration.
- Building the Tokio runtime.
- Initializing the API description.
- Writing the OpenAPI description.
- Initializing logging.
- Connecting to the database and running migrations.
- Building or running the Dropshot server.

Fatal errors should:

- Name the failed startup operation.
- Preserve the source error chain.
- Avoid repeating source text in `Display`.
- Avoid printing secret values.
- Be tested when they encode important policy, such as config validation or
  secret redaction.

Startup config output intentionally redacts the database connection source. See
[`nv-server` Configuration](./nv-server-configuration.md) for the full config
and secret behavior.

## CLI Errors

Backend tools should use `anyhow` when failures are reported to a human and no
caller needs typed matching. This is the current `nvdb` pattern.

Use `anyhow::Context` to name failed subprocesses or operations:

```rust
.context("failed to run sea-orm-cli generate entity")?;
```

Use `bail!` for clear command failures after a subprocess exits
unsuccessfully:

```rust
if !status.success() {
    bail!("sea-orm-cli migrate failed");
}
```

Create typed CLI errors when the tool needs structured handling, when tests
need to assert distinct failure kinds, or when several commands share the same
recoverable error behavior.

At the top-level CLI boundary, prefer reporting the full error chain with
`{err:#}` or an equivalent chain-aware formatter when the source error is useful
for local diagnosis. It is acceptable to show only the outer message for short,
expected usage errors where extra causes would add noise.

For CLI commands that pass secrets to child processes, prefer environment
variables or files over command-line arguments. `nvdb` passes `DATABASE_URL` to
`sea-orm-cli` through the child environment so credentials do not appear in the
argument list. Do not print the child environment when real credentials are in
use.

## Background Workers

Background workers should report per-item failures without taking down the
whole server unless the failure means the process cannot safely continue.

When adding worker code:

- Define typed errors for failures the scheduler, retry policy, or status model
  needs to handle differently.
- Record enough safe context to debug the failed item, such as a package-source
  ID, external tool name, retry count, or durable status transition.
- Preserve source chains for lower-level failures.
- Mark whether failures are retryable when the worker or API exposes that
  distinction.
- Keep database and in-memory state consistent when futures are cancelled.
- Do not log hostile external text as trusted instructions or structured
  fields without validation.
- Do not include submitted source contents, credentials, tokens, or secret file
  contents in logs or durable error records.

The async cancellation guidance in
[Rust Best Practices](./rust-best-practices.md) applies to background workers
and request handlers. Prefer designs where cancellation or retry leaves durable
state understandable to operators and clients.

## Secret-Safe Diagnostics

Secret safety applies to every error boundary.

Never print or log:

- Database passwords or full credential-bearing database URLs.
- API tokens, service credentials, or secret file contents.
- Process environments that may contain credentials.
- User-submitted source contents unless the feature explicitly requires safe
  display of that content.
- Raw third-party tool stdout or stderr when it may include secrets.

Use redacted source descriptions where possible. `SecretSourceKind` displays
`<redacted inline secret>` or `<redacted file-backed secret>` instead of the
database connection value or secret file path in config summaries.

Secret file read failures may include the secret file path because the path is
needed for local diagnosis, but they must not print the secret value. This is
the current `ConfigLoadError::FailedToReadSecretFile` behavior.

When capturing stdout or stderr from tools such as `sea-orm-cli`, Hipcheck, or
future package analyzers, decide whether that output is safe before logging,
storing, or returning it. If it is needed for debugging but could contain
secret or hostile content, redact it, summarize it, or keep it behind an
operator-only diagnostic path.

For package-source and analyzer flows, use a stricter default: raw request
contents, dependency error text, stdout, and stderr are never normal log
fields, durable status diagnostics, or reflected API errors. Map failures to
stable Night Vision messages and record only safe structured context such as a
durable ID, operation, failure kind, retryability, exit status, and configured
numeric limit. Bounded raw analyzer evidence is a separate, explicitly
authorized diagnostic surface; it must not be treated as a safe error message
or log field.

## Review Checklist

Use this checklist when reviewing backend error-handling changes:

- Does each wrapper add useful context from the layer that knows the operation?
- Are source errors preserved through `source()` or `anyhow::Context`?
- Do `Display` implementations avoid repeating source errors?
- Are full chains printed or logged at diagnostic boundaries?
- Is `anyhow` limited to places where callers do not need typed matching?
- Are HTTP responses mapped to safe messages and status codes?
- Are fatal errors limited to startup or unrecoverable process failures?
- Are CLI errors useful without exposing command-line secrets?
- Do background worker errors preserve durable state and retry information?
- Are secrets, credentials, stack traces, and sensitive resource details kept
  out of user-facing messages and logs?

[oxide-error-types]: https://github.com/oxidecomputer/omicron/blob/main/docs/error-types-and-logging.adoc
