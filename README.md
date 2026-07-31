# Exponential

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)

**Vibecode together.** Issues, customer support, and coding agents in one realtime tracker — agents run locally on your machines, on your subscription, with unlimited sessions at a flat price. Native apps for web, macOS, Linux, iOS, and Android (the Windows desktop is an unsigned build). Open source under Apache-2.0.

- **Cloud**: [app.exponential.at](https://app.exponential.at/?ref=github) — free for teams of up to three
- **Self-host**: one `docker compose`, free for everyone — no seat caps, no licence gate; mobile push is the one cloud-only capability ([docs](https://exponential.at/docs/self-host/?ref=github))
- **Download the desktop IDE**: [exponential.at/download](https://exponential.at/download/?ref=github)

## What it does

- **Issues** — statuses, priorities, labels, due dates, assignees, GFM descriptions with @mentions and image attachments. Real-time sync on every client via [ElectricSQL](https://electric-sql.com); no spinners, no stale lists.
- **Boards, optionally backed by a GitHub repository** — connect one through a GitHub App and the coding features turn on: one issue = one branch (`exp/EXP-42`) = one pull request, tracked on the issue — or one shared branch and one combined PR for a batch of issues. Boards without a repository stay plain issue tracking.
- **Start coding** — the desktop IDE hands an issue to Claude Code, Codex, or pi on *your* machine, on your subscription: it creates a git worktree, plans, codes in the embedded terminal, and opens the PR itself. Bring your own agents — there is no cloud-agent billing, ever.
- **Desktop IDE** (Rust, [gpui](https://www.gpui.rs)) — issue board, file tree, commit history and side-by-side diffs (view-only: the trunk stays clean and changes arrive via PRs), embedded terminal.
- **Live steer** — watch and redirect a running coding session from your phone.
- **Feedback widget** — a script tag for your own site; visitors report bugs with annotated screenshots that land as issues.
- **MCP server** — point Claude Code, Codex, Cursor, or any MCP client at `/api/mcp` and work with issues, boards, and PRs as your real user (OAuth 2.1, no tokens to copy).

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
  db-schema/         Drizzle schema + shared domain types
  widget/            Embeddable feedback widget (Preact)
  icons/             The one Lucide icon registry → TS/Swift/Kotlin/Rust
  electric-protocol/ Electric shape wire contract + fixtures
  ...                design tokens, domain contract, steer tickets, tsconfig
```

**Stack**: TanStack Start (React 19) · PostgreSQL 17 + Drizzle · ElectricSQL · tRPC v11 · Better Auth · shadcn/ui + Tailwind v4 · bun.

## Self-host quick start

No checkout, no build — two files and a `docker compose up`, pulling the published multi-arch image [`ghcr.io/niach/exponential-web`](https://github.com/Niach/exponential/pkgs/container/exponential-web):

```sh
mkdir exponential && cd exponential
curl -fsSLO https://raw.githubusercontent.com/Niach/exponential/master/selfhost/docker-compose.yaml
curl -fsSL https://raw.githubusercontent.com/Niach/exponential/master/selfhost/.env.example -o .env
# fill in .env: two openssl-generated secrets + your S3 credentials
docker compose up -d
```

App at `http://localhost` — migrations and the custom trigger SQL apply themselves at every boot. Bring any S3-compatible bucket for attachments (Hetzner, MinIO, R2, AWS, Garage, …); everything else is bundled. The full runbook — domain + automatic HTTPS, email, sign-in providers, steer relay, upgrades — is [`INSTALL.md`](./INSTALL.md) (written so you can also hand it to a coding agent), with a longer walkthrough in the [self-host docs](https://exponential.at/docs/self-host/?ref=github).

Teams, boards, and issues work out of the box: a **GitHub App is only needed for coding** — backing a board with a repository, and the PRs coding sessions open. Set `GITHUB_APP_ID`, `GITHUB_APP_SLUG`, `GITHUB_APP_PRIVATE_KEY` when you want that; see [`.env.example`](./.env.example) for these and everything else (OIDC, Google login, SMTP/Amazon SES, steer). Upgrades are `docker compose pull && docker compose up -d`; pin `IMAGE_TAG` to a [release tag](https://github.com/Niach/exponential/tags) if you'd rather move deliberately than track `master`.

## Development

Needs [bun](https://bun.sh), Node ≥ 20.19 (or ≥ 22.12), and Docker Compose for
the backend stack. First run:

```sh
bun install
cp .env.example .env
cp Caddyfile.example Caddyfile   # gitignored; the dev compose mounts it
bun run backend                  # docker compose up -d + web dev server
bun run storage:init             # one-time Garage bootstrap; prints the S3 keys for .env
```

Then:

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

Do **not** run `bun run lint` — its `--fix` corrupts `typeof import()` sites.
Use `bun run typecheck`.

Deeper architecture notes live in [`CLAUDE.md`](./CLAUDE.md) (also available as
`AGENTS.md`, for agents that look for that name).

## Connect an MCP client

```sh
claude mcp add --transport http exponential https://app.exponential.at/api/mcp
```

First call opens a browser login; every tool call after runs as your user.

## Push notifications on self-hosted instances

One capability doesn't travel: **mobile push notifications**. The store-distributed iOS and Android apps are compiled against the first-party Firebase/APNs project, so a self-hosted instance can't push to them. Web and desktop are fully featured either way; the mobile apps work against a self-hosted instance (build them from source in this repo), they just won't receive push. Push notifications are available on the cloud plans.

## License

[Apache-2.0](./LICENSE) — fully open source. Self-host it in production for free, for any team size, with no restrictions. Optional [Enterprise Support](https://exponential.at/contact/?ref=github) is available for self-hosters who want an SLA, priority support, deployment help, or custom development (support@exponential.at).
