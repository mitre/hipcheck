# Hipcheck Rust Plugin SDK &#x2713;

A software development kit to help with writing plugins in Rust for the
[Hipcheck][website] dependency analysis tool.

## Overview

Hipcheck is a software dependency analyiss tool that helps identify risky
project management practices and potential supply-chain attacks. It uses a
plugin-based anaylsis architecture, such that Hipcheck users can write and
release their own plugins that integrate seamlessly with the core binary and
other analyses. The Rust plugin SDK provides the boilerplate code for defining a
plugin and communicating with Hipcheck core over gRPC, allowing plugin authors
to focus on the business logic of their plugin query endpoints.

## Getting Started

The Hipcheck website has a [guide][sdk-guide] for writing plugins using the Rust
SDK. For examples of using the SDK, the `plugins/` [subdirectory][plugins-src]
of the Hipcheck repository contains a suite of plugins maintained by the
Hipcheck team that are all written with the SDK. See the `docs.rs`
[page][sdk-docs] for the official documentation.

## Links

[Docs][sdk-docs] | [Guide][sdk-guide] | [Examples][plugin-src]

## License

Hipcheck's software is licensed under the Apache 2.0 license, which can be
found in the [`LICENSE`][license] file in this repository.

[license]: https://github.com/mitre/hipcheck/blob/main/LICENSE
[website]: https://hipcheck.mitre.org/
[sdk-guide]: https://hipcheck.mitre.org/docs/guide/making-plugins/rust-sdk/
[plugins-src]: https://github.com/mitre/hipcheck/tree/main/plugins
[sdk-docs]: https://docs.rs/crate/hipcheck-sdk/latest
