# Frontend Developer Guide

This guide covers the Night Vision SvelteKit frontend. Use it when changing
the app under `frontend/`, updating frontend workflows, or wiring frontend code
to the backend REST API.

[[_TOC_]]

## App Layout

The frontend app lives in `frontend/` and uses SvelteKit, TypeScript, pnpm, and
Tailwind CSS.

Important paths:

- `frontend/src/routes/`: SvelteKit route tree.
- `frontend/src/routes/+layout.svelte`: root layout for all routes.
- `frontend/src/routes/+page.svelte`: current root page.
- `frontend/src/routes/layout.css`: global CSS imported by the root layout.
- `frontend/src/lib/`: shared frontend code imported through the `$lib` alias.
- `frontend/src/lib/assets/`: shared static assets imported by code, such as
  the favicon.
- `frontend/static/`: files served directly as static assets.
- `frontend/svelte.config.js`: SvelteKit config and adapter setup.
- `frontend/vite.config.ts`: Vite plugin setup.

Generated outputs and installed dependencies are not source. Do not edit or
commit `frontend/node_modules/`, `frontend/.svelte-kit/`, or `frontend/build/`.

## Routing

Use SvelteKit file-based routing under `frontend/src/routes/`.

- Put a route page in `+page.svelte`.
- Put layout shared by a route subtree in `+layout.svelte`.
- Keep route-only helpers close to the route that owns them.
- Move reusable components, types, and utilities to `frontend/src/lib/` when
  more than one route needs them.

The current app has a root layout and root page only. Add deeper route
directories when the product flow needs them, and keep URLs readable and stable
because backend and documentation examples may point to them over time.

## Components And Styling

Write UI in Svelte single-file components. Use `<script lang="ts">` for
component scripts.

Tailwind CSS is configured through the Vite plugin in `frontend/vite.config.ts`.
Global CSS is imported from `frontend/src/routes/+layout.svelte` and currently
lives in `frontend/src/routes/layout.css`:

```css
@import 'tailwindcss';
@plugin '@tailwindcss/typography';
```

Prefer component-local markup and classes for route-specific UI. Put shared
components under `frontend/src/lib/` when reuse is real. Avoid adding a new
styling framework or global convention without updating this guide.

## TypeScript And Svelte

The project uses Svelte 5 and TypeScript. `frontend/svelte.config.js` forces
runes mode for project code, except files under `node_modules`:

```js
runes: ({ filename }) =>
	filename.split(/[/\\]/).includes('node_modules') ? undefined : true
```

Keep frontend changes clean under `svelte-check`. Add explicit types where
they make contracts clearer, especially for data passed between routes,
components, and API helpers.

## Backend API Integration

Use the backend REST API docs for current endpoint behavior:

- [REST API Usage](../backend/rest-api-usage.md)
- [`nv-server` Configuration](../backend/nv-server-configuration.md)

The local backend sample configuration binds `nv-server` to
`127.0.0.1:8080`. The local Docker Compose override also publishes the backend
on `127.0.0.1:8080` and the frontend on `127.0.0.1:3000`.

The repository does not yet contain generated frontend API client code. Until
that exists, keep API integration code small, typed, and isolated under
`frontend/src/lib/` instead of spreading fetch calls throughout route
components. When generated client code is added, document the generation
command, source schema, output path, and review expectations in this guide.

Do not duplicate backend development guidance here. Frontend docs should point
to backend docs for server setup, API behavior, status codes, and operational
details.

## Local Development

Use Flox for normal development from the repository root:

```sh
flox activate
```

Then work from the frontend directory:

```sh
cd frontend
pnpm install
pnpm run dev
```

To open the app automatically in a browser:

```sh
pnpm run dev -- --open
```

Tool versions are declared in `frontend/package.json`:

- Node.js `>=24.15.0 <25.0.0`
- pnpm `>=11.4.0 <12.0.0`

Flox also installs `nodejs_24` and pnpm. Keep frontend tool versions in sync
between `frontend/package.json`, `.flox/env/manifest.toml`, and
`frontend/Dockerfile`.

## Checks

Run focused frontend checks from `frontend/` before opening an MR:

```sh
pnpm run check
pnpm run build
```

Useful related commands:

```sh
pnpm run check:watch
pnpm run preview
```

`pnpm run check` runs `svelte-kit sync` and `svelte-check` with the project
TypeScript config. `pnpm run build` creates the production SvelteKit build.

If dependencies are missing or stale, run `pnpm install` first. Keep
`pnpm-lock.yaml` changes only when dependency inputs actually changed.

## Docker

For local full-stack development, use the repository root Compose wrapper
described in the main README:

```sh
cp .env.local.example .env
scripts/setup-compose-secrets.sh -x
scripts/docker-compose-local.sh up --build
```

The local Compose override builds `nv-app:local` from `frontend/Dockerfile` and
binds it to `127.0.0.1:3000`.

To build and run only the frontend production container from the repository
root:

```sh
docker build -f frontend/Dockerfile -t nv-app:local frontend
docker run --rm -p 127.0.0.1:3000:3000 nv-app:local
```

The Dockerfile uses SvelteKit's Node adapter, prepares pnpm with Corepack, runs
`pnpm install --frozen-lockfile`, builds with `pnpm run build`, and starts the
runtime image with `node build`.

Keep these Dockerfile arguments aligned with project tooling:

- `NODE_VERSION`
- `PNPM_VERSION`

## Merge Request Checklist

For frontend changes, include this in the PR description:

- What changed and why.
- Frontend checks run, especially `pnpm run check` and `pnpm run build`.
- Any Docker or Compose validation, if the change affects runtime behavior.
- Any backend API assumptions or docs used.
- AI tool use, if applicable, following the [AI Policy](../project/ai-policy.md).
