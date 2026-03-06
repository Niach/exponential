# Exponential

Exponential is a real-time issue tracker built with TanStack Start, TanStack DB, Electric SQL, Drizzle, and Better Auth.

## Stack

- TanStack Start for routing and server handlers
- TanStack DB + Electric SQL for live client collections
- tRPC for authenticated mutations
- Drizzle ORM for schema and database access
- Better Auth for email/password authentication
- Tailwind CSS + shadcn primitives for UI

## Local development

### Prerequisites

- Node 20.19+ or 22.12+
- `bun`
- Docker
- Caddy

Electric sync performs much better over HTTPS locally, so the app expects Caddy-backed local HTTPS during development.

### Setup

1. Install dependencies:

```sh
bun install
```

2. Copy your environment file if needed:

```sh
cp .env.example .env
```

3. Start backend services:

```sh
bun backend:up
```

4. Run database migrations:

```sh
bun migrate
```

5. Start the app:

```sh
bun dev
```

The app runs on `https://localhost:5173`.

## Useful scripts

```sh
bun dev
bun build
bun serve
bun backend:up
bun backend:down
bun backend:clear
bun migrate
bun migrate:generate
bun psql
bun lint:check
bun lint
bun format:check
bun format
bun typecheck
bun run test
```

## Architecture notes

### Auth and authorization

- Better Auth manages sessions and users.
- Route guards in `src/routes/_authenticated.tsx` gate authenticated screens.
- tRPC mutations enforce workspace-scoped authorization in `src/lib/trpc/*`.
- Workspace, project, and issue-label mutations now validate workspace membership or owner role before mutating data.

### Real-time data flow

- Electric-backed collections live in `src/lib/collections.ts`.
- Authenticated shape proxy routes live in `src/routes/api/shapes/*`.
- Each shape route scopes rows to the current user before proxying to Electric.
- User sync is also scoped, so clients only receive users who share at least one workspace with the current user.

### Main client areas

- Workspace shell and navigation: `src/routes/_authenticated/w/$workspaceSlug/route.tsx`
- Project board: `src/routes/_authenticated/w/$workspaceSlug/projects/$projectSlug/index.tsx`
- Workspace settings: `src/routes/_authenticated/w/$workspaceSlug/settings/index.tsx`
- Shared issue editor shell: `src/components/issue-editor-dialog-shell.tsx`

## Testing and quality gates

The repo is expected to stay clean on:

```sh
bun lint:check
bun typecheck
bun run test
```

Current tests cover:

- workspace authorization and scoping helpers
- project board filtering and grouping helpers
- shape route handler behavior
- shared issue editor shell wiring

## Notes

- `src/routeTree.gen.ts` is generated and should not be edited manually.
- Electric shape routes and tRPC procedures intentionally mirror workspace boundaries. If you add a new synced entity, wire both read scoping and mutation authorization at the same time.
