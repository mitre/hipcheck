
# Night Vision: 🔭 CTI for OSS Packages

__Night Vision__ is a cyber threat intelligence (CTI) system for open source
software (OSS) packages. It allows users to subscribe for alerts to new cyber
threat indicators on the open source software they use.

## What Night Vision Does

With Night Vision, users will provide their package files (like `package.json`
for most JavaScript packages) to subscribe to alerts for new cyber threats for
all the dependencies in those package files. Night Vision will thereafter
monitor those packages for new threats, and alert users when any are discovered
so they can take action.

## Repository Contents

This repository contains both the backend and frontend for Night Vision. The
backend is a REST API written in [Rust] and backed by [PostgreSQL]. The frontend
is a single page application (SPA) written in [TypeScript] with [Svelte] and
using [pnpm] as its package manager and build system.

## License




[Rust]: https://rust-lang.org/
[PostgreSQL]: https://www.postgresql.org/
[TypeScript]: https://www.typescriptlang.org/
[SvelteKit]: https://svelte.dev/
[pnpm]: https://pnpm.io/
