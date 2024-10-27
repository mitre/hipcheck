---
title: Using a Debugger
weight: 3
---

# Debugger

Hipcheck can be run under a debugger like `gdb` or `lldb`. Because Hipcheck is
written in Rust, we recommend using the Rust-patched versions of `gdb` or `lldb`
which ship with the Rust standard tooling. These versions of the tools include
specific logic to demangle Rust symbols to improve the experience of debugging
Rust code.

You can install these tools by following the standard [Rust installation
instructions](https://www.rust-lang.org/tools/install).

With one of these debuggers installed, you can then use them to set breakpoints
during Hipcheck's execution, and do all the normal program debugging processes
you're familiar with if you've used a debugger before. Explaining the use of
these tools is outside of the scope of the Hipcheck documentation, so we defer
to their respective documentation sources.
