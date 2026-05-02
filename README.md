# Exponential

A real-time, self-hosted issue tracker. Open source, MIT-licensed, and yours end-to-end.

## Why

Most issue trackers are SaaS. Exponential is a small, sharp, self-hostable alternative: one `docker-compose` brings up Postgres, Electric, Garage, and Caddy; one `.env` configures auth; you own the data and the box it sits on. Mutations are optimistic and reconcile through Electric, so every connected client stays live without spinners or stale lists.

## Features

- **Issues** with status, priority, labels, due dates, assignees, recurrence, image attachments, rich-text descriptions
- **Real-time sync** across clients via [ElectricSQL](https://electric-sql.com) shape proxies
- **Workspaces** with multi-user membership and email invites
- **Auth** — email/password and OIDC (Authentik, Keycloak, anything that speaks OIDC)
- **Google Calendar integration** — opt-in per user; due dates round-trip as all-day events ([scope: `calendar.events`](https://developers.google.com/identity/protocols/oauth2/scopes#calendar))
- **MCP server** — exposes issue/label/project tools to Claude Code, Cursor, etc. via static Bearer auth
- **Keyboard-friendly** — context menus, inline status/priority/label editing, save-on-blur

## Quick start

```sh
git clone https://github.com/Niach/exponential
cd exponential
cp .env.example .env             # set BETTER_AUTH_SECRET, optional integrations
openssl rand -hex 32 > infra/garage/secrets/rpc_secret
openssl rand -base64 32 > infra/garage/secrets/admin_token
docker compose up -d             # postgres, electric, garage, caddy
bun install
bun migrate
docker exec -i exponential-postgres-1 \
  psql -U postgres -d exponential < apps/web/src/db/out/custom/0001_triggers.sql
bun run storage:init             # creates Garage bucket + access key; paste output into .env
bun dev
```

The app runs at `https://localhost:3000` (Caddy proxies it) — Electric sync needs HTTPS locally for HTTP/2.

## Repo layout

This is a bun workspace.

```
.
├── src/                  # the app (TanStack Start + tRPC + Electric + Drizzle)
│   ├── db/               # Drizzle schema, migrations, custom triggers
│   ├── lib/              # auth, trpc routers, electric collections, google-calendar, mcp
│   ├── components/       # shadcn primitives + business components
│   └── routes/           # TanStack Router file routes (incl. /api/trpc, /api/shapes, /api/auth)
├── marketing/            # standalone Vite app for exponential.straehhuber.com
│   └── src/              # home + privacy + terms, deployed to Cloudflare Pages
├── docker-compose.yaml   # postgres:54321, electric:30000, garage:3900, caddy:3000
├── Dockerfile            # production app image
└── CLAUDE.md             # deeper architecture notes for AI coding assistants
```

## Tech stack

- **Framework**: TanStack Start (React 19, TanStack Router, TanStack React DB)
- **Database**: PostgreSQL 17 + Drizzle ORM (`snake_case` casing)
- **Real-time**: ElectricSQL shape-proxy pattern via `@tanstack/electric-db-collection`
- **API**: tRPC v11 with `authedProcedure` + `generateTxId` for Electric-aware mutations
- **Auth**: Better Auth (email/password, OIDC via `genericOAuth`, Google social provider for Calendar linking)
- **UI**: shadcn/ui on Tailwind v4, dark theme forced via `html.dark`, OKLCH zinc palette
- **Editor**: Tiptap with markdown + image extensions
- **Storage**: Garage (S3-compatible) for issue image attachments
- **Package manager**: bun

## Configuration

See [`.env.example`](./.env.example) for the full list. Highlights:

| Var | Required | Purpose |
|---|---|---|
| `DATABASE_URL` | yes | Postgres connection string |
| `BETTER_AUTH_SECRET` | yes | 32+ char session-signing secret |
| `BETTER_AUTH_URL` | yes | Public app URL |
| `ELECTRIC_URL` | yes | Electric service URL |
| `S3_*` | yes | Attachment storage (Garage by default) |
| `AUTH_OIDC_ENABLED` + `OIDC_*` | optional | Enable OIDC login |
| `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET` | optional | Enable Google Calendar integration |
| `MCP_API_TOKEN` + `MCP_USER_EMAIL` | optional | Enable MCP server |

After schema changes, regenerate + apply migrations:

```sh
bun migrate:generate
bun migrate
```

Custom SQL triggers (issue numbering, `updated_at` auto-touch) live in `src/db/out/custom/` and must be reapplied manually after a fresh DB.

## Development

```sh
bun dev               # dev server with HMR
bun build             # production build
bun start             # serve the built app
bun lint              # eslint --fix
bun format            # prettier --write
bun typecheck         # tsc --noEmit
bun run test          # vitest
bun run test:e2e      # playwright
```

### End-to-end tests

Playwright covers auth, workspace bootstrap, project + issue CRUD, and invite flow. Hits `https://localhost:3000` (Caddy proxy), so keep `docker compose up` running and the app on `:5173`.

```sh
bun backend:up
bun migrate
bunx playwright install chromium
bun run test:e2e
```

### Marketing site

```sh
cd marketing
bun run dev           # local preview at http://localhost:5173/
bun run build         # outputs dist/ for Cloudflare Pages
```

## Architecture notes (brief)

- **Auth gating** in `src/routes/_authenticated.tsx` redirects unauthenticated requests
- **tRPC mutations** enforce workspace-scoped authorization in `src/lib/trpc/*` (`assertProjectMember`, `assertWorkspaceMember`)
- **Electric shape proxies** in `src/routes/api/shapes/*` scope rows to the requesting user before forwarding to Electric
- **Collections** in `src/lib/collections.ts` use `snakeCamelMapper()` so `useLiveQuery` filters work against camelCase fields
- **Google Calendar sync** is fire-and-forget after each issue mutation commits — failures persist to `issues.googleCalendarLastSyncError` but never block the mutation
- **MCP** delegates every operation through the same tRPC routers as the UI, so write-path authorization stays single-source

For deeper guidance, see [`CLAUDE.md`](./CLAUDE.md).

## License

MIT.
