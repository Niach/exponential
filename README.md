# Exponential

**Issue tracking that ships code.** A real-time issue tracker with a built-in coding IDE — feedback in, pull requests out. Native on web, macOS, Windows, Linux, iOS, and Android.

- **Cloud**: [app.exponential.at](https://app.exponential.at) — free for individuals
- **Self-host**: one `docker compose`, every feature, free for companies under 10 people ([docs](https://exponential.at/docs/self-host/))
- **Download the desktop IDE**: [exponential.at/download](https://exponential.at/download/)

## What it does

- **Issues** — statuses, priorities, labels, due dates, assignees, recurring issues, GFM descriptions with @mentions and image attachments. Real-time sync on every client via [ElectricSQL](https://electric-sql.com); no spinners, no stale lists.
- **Every project is a GitHub repository** — connected through a GitHub App. One issue = one branch (`exp/EXP-42`) = one pull request, tracked on the issue.
- **Start coding** — the desktop IDE hands an issue to Claude Code on *your* machine, on your subscription: it creates a git worktree, plans, codes in the embedded terminal, and opens the PR itself. Bring your own agents — there is no cloud-agent billing, ever.
- **Desktop IDE** (Rust, [gpui](https://www.gpui.rs)) — issue board, file tree, source control with side-by-side diffs, embedded terminal.
- **Live steer** — watch and redirect a running coding session from your phone.
- **Feedback widget** — a script tag for your own site; visitors report bugs with annotated screenshots that land as issues.
- **MCP server** — point Claude Code, Cursor, or any MCP client at `/api/mcp` and work with issues, projects, and PRs as your real user (OAuth 2.1, no tokens to copy).

## The repo

Bun workspace monorepo:

```
apps/
  web/          TanStack Start app — the tracker, API, Electric shape proxies
  desktop/      Rust IDE (gpui + alacritty_terminal), Cargo workspace
  ios/          SwiftUI (Tuist + GRDB)
  android/      Kotlin / Jetpack Compose
  marketing/    exponential.at (Vite MPA)
  push-relay/   Push notification relay (Hono/Bun)
  steer-relay/  Remote-start + live-steer WebSocket hub (Bun)
packages/
  db-schema/    Drizzle schema + shared domain types
  widget/       Embeddable feedback widget (Preact)
  ...           design tokens, domain contract, steer tickets, tsconfig
```

**Stack**: TanStack Start (React 19) · PostgreSQL 17 + Drizzle · ElectricSQL · tRPC v11 · Better Auth · shadcn/ui + Tailwind v4 · bun.

## Self-host quick start

```sh
git clone https://github.com/Niach/exponential
cd exponential
cp .env.example .env             # set BETTER_AUTH_SECRET + GitHub App creds
cp Caddyfile.example Caddyfile   # gitignored — compose bind-mounts it
openssl rand -hex 32 > infra/garage/secrets/rpc_secret
openssl rand -base64 32 > infra/garage/secrets/admin_token
docker compose up -d             # postgres, electric, garage, caddy
bun install && bun migrate
docker exec -i exponential-postgres-1 \
  psql -U postgres -d exponential < apps/web/src/db/out/custom/0001_triggers.sql
bun run storage:init             # prints S3 keys — paste into .env
bun dev
```

App at `https://localhost:3000` (Caddy proxies for HTTP/2). A **GitHub App is required to create projects** (every project is a repository) — set `GITHUB_APP_ID`, `GITHUB_APP_SLUG`, `GITHUB_APP_PRIVATE_KEY`; see [`.env.example`](./.env.example) for these and everything else (OIDC, Google login, SMTP/Amazon SES, push, steer). Full guide: [self-host docs](https://exponential.at/docs/self-host/).

For production, build the web image and run it instead of `bun dev` — note that with `NODE_ENV=production`, password sign-up is **disabled by default**, so opt in (or configure an OAuth/OIDC provider):

```sh
docker build -t exponential-web .
docker run -d --name exponential-web --network host \
  --env-file .env -e PORT=5173 -e AUTH_SIGNUP_ENABLED=true \
  exponential-web   # Caddy proxies host port 5173; migrations run on boot
```

## Development

```sh
bun run backend       # local backend: docker compose up -d + web dev server (localhost:3000)
bun run ios           # iOS: tuist generate → Xcode (Mac-only)
bun run android       # Android: install productionDebug + launch on device/emulator
bun dev               # web app only (localhost:5173)
bun run typecheck     # tsc
bun run test          # vitest
bun run test:e2e      # playwright (needs docker compose up)
bun run dev:desktop   # Rust IDE against the local backend
bun run android:build # gradle assemble
bun run dev:marketing # marketing site
```

Deeper architecture notes live in [`CLAUDE.md`](./CLAUDE.md).

## Connect an MCP client

```sh
claude mcp add --transport http exponential https://app.exponential.at/api/mcp
```

First call opens a browser login; every tool call after runs as your user.

## License

[Exponential Small Team License 1.0](./LICENSE) — source-available, not open source. Read it, change it, and self-host it in production for free while your company, affiliates included, has **fewer than 10 people** (employees plus independent contractors). 10 or more needs a commercial license — dennis@straehhuber.com. Evaluation, development, testing, and non-commercial research stay free at any size. Nobody may offer it to third parties as a hosted or managed service, whatever their size.

Versions released before 2026-07-21 remain available under the [Elastic License 2.0](https://www.elastic.co/licensing/elastic-license) — the relicense applies going forward only.
