# Spookey Format

Spookey is Night Vision's simple key-value configuration format. It is inspired
by Ghostty configuration files and is implemented by the `spookey` backend
crate.

## Syntax

A Spookey file is plain text. Each setting uses one line:

```spookey
key-name = value
```

Blank lines are allowed. Lines whose first non-whitespace character is `#` are
comments:

```spookey
# Bind the server to loopback for local development.
server-address = "127.0.0.1:8080"
```

The parser trims surrounding whitespace from each line, key, and value. Keys
are case-sensitive. Keys and values may contain Unicode, but project
configuration keys should use lowercase ASCII words separated by hyphens.

## Values

Values are parsed as strings. The caller is responsible for converting those
strings into addresses, numbers, enum values, paths, or other typed settings.

If a value begins or ends with a double quote, those quote characters are
removed:

```spookey
quoted = "value"
unquoted = value
```

Both examples parse to `value`.

An empty value is treated as unset:

```spookey
optional-setting =
```

This lets sample configuration files list optional keys without setting them.

## Required And Optional Keys

Spookey itself does not define the valid key set. Each caller passes a parser
configuration with required keys and optional keys. A key in the file must
match one of those configured names exactly.

Required keys must be present with non-empty values. Optional keys may be
missing or present with empty values.

## Warnings And Errors

Spookey reports fatal parse errors for:

- I/O errors while reading the file.
- Missing required keys.

Spookey reports warnings for:

- Lines that are not in `key = value` format.
- Lines with a value but no key.
- Keys set more than once.
- Keys not listed in the caller's required or optional key set.

When a key is set more than once, the later value wins and Spookey records a
warning.

`nv-server` treats Spookey warnings as configuration errors during startup.
That means unknown keys, malformed lines, duplicate keys, and missing required
keys all stop the server before it starts accepting requests.
