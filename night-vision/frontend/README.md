# Night Vision Frontend

This is the frontend application for Night Vision. It uses SvelteKit with
TypeScript as the language and pnpm as the package manager / build tool.

## Key Commands

```sh
# Install dependencies
$ pnpm install
# Run development server
$ pnpm run dev
# Run development server and open the app in a new browser tab
$ pnpm run dev -- --open
# Create a production version of the app
$ pnpm run build
# Preview the production build
$ pnpm run preview
# Generate API Client Code - Static File Reference (Offline)
$ pnpm run generate:api
$ pnpm generate:api

# Validation check 
$ pnpm run check
```

## Docker

Build and run the production container from the repository root:

```sh
$ docker build -f frontend/Dockerfile -t nv-app:local frontend
$ docker run --rm -p 127.0.0.1:3000:3000 nv-app:local
```

## API Client Code Generation 
This project uses `@hey-api/openapi-ts` to generate TypeScript types and a Fetch client from the backend OpenAPI specification, ensuring the frontend stays synchronized with the API.

### Obtaining OpenAPI Input for Local Development

The definitive schema file is located at:
`../backend/openapi/nv-server-openapi.json`

To synchronize the TypeScript interfaces with recent backend changes, run:

```bash
pnpm run generate:api
```
or 
```bash
pnpm generate:api
```
*Note: You do not need to have the Rust backend running locally to use this command.*

### Running Locally - Base URL and Port
1. For local Docker Compose, copy the repository-level example file from the
   repository root:
   ```bash
   cp .env.local.example .env
   ```
   Compose passes `API_BASE_URL=http://nv-server:8080` to the frontend so the
   container can reach the backend over the Compose network.
2. When running `pnpm dev` on the host, create `frontend/.env` with
   `API_BASE_URL=http://127.0.0.1:8080` (or your backend's host and port).


### Check-in & Git Policy

* **Check In**: The generated pure type definitions file (`frontend/src/lib/api/generated`) **must be committed** to Git. This allows the frontend application to pass type-checks instantly on a fresh clone without forcing a developer to build or boot up the API.
* **Ignore Policy**: Never manually edit `generated`. Any temporary artifacts or lockfiles are excluded via `.gitignore`.
