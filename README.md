# Exponential

**Vibecode together.** Issues, customer support, and coding agents in one realtime workspace — agents run locally on your machines, on your subscription, with unlimited sessions at a flat price. Native on web, macOS, Windows, Linux, iOS, and Android.

- **Cloud**: [app.exponential.at](https://app.exponential.at) — free for teams of up to three
- **Self-host**: one `docker compose`, every feature, free for companies under 10 people ([docs](https://exponential.at/docs/self-host/))
- **Download the desktop IDE**: [exponential.at/download](https://exponential.at/download/)

## What it does

- **Issues** — statuses, priorities, labels, due dates, assignees, GFM descriptions with @mentions and image attachments. Real-time sync on every client via [ElectricSQL](https://electric-sql.com); no spinners, no stale lists.
- **Boards, optionally backed by a GitHub repository** — connect one through a GitHub App and the coding features turn on: one issue = one branch (`exp/EXP-42`) = one pull request, tracked on the issue. Boards without a repository stay plain issue tracking.
- **Start coding** — the desktop IDE hands an issue to Claude Code on *your* machine, on your subscription: it creates a git worktree, plans, codes in the embedded terminal, and opens the PR itself. Bring your own agents — there is no cloud-agent billing, ever.
- **Desktop IDE** (Rust, [gpui](https://www.gpui.rs)) — issue board, file tree, source control with side-by-side diffs, embedded terminal.
- **Live steer** — watch and redirect a running coding session from your phone.
- **Feedback widget** — a script tag for your own site; visitors report bugs with annotated screenshots that land as issues.
- **MCP server** — point Claude Code, Cursor, or any MCP client at `/api/mcp` and work with issues, boards, and PRs as your real user (OAuth 2.1, no tokens to copy).

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

No checkout, no build — two files and a `docker compose up`, pulling the published multi-arch image [`ghcr.io/niach/exponential-web`](https://github.com/Niach/exponential/pkgs/container/exponential-web):

```sh
mkdir exponential && cd exponential
curl -fsSLO https://raw.githubusercontent.com/Niach/exponential/master/selfhost/docker-compose.yaml
curl -fsSL https://raw.githubusercontent.com/Niach/exponential/master/selfhost/.env.example -o .env
# fill in .env: two openssl-generated secrets + your S3 credentials
docker compose up -d
```

App at `http://localhost` — migrations and the custom trigger SQL apply themselves at every boot. Bring any S3-compatible bucket for attachments (Hetzner, MinIO, R2, AWS, Garage, …); everything else is bundled. The full runbook — domain + automatic HTTPS, email, sign-in providers, steer relay, upgrades — is [`INSTALL.md`](./INSTALL.md) (written so you can also hand it to a coding agent), with a longer walkthrough in the [self-host docs](https://exponential.at/docs/self-host/).

Teams, boards, and issues work out of the box: a **GitHub App is only needed for coding** — backing a board with a repository, and the PRs coding sessions open. Set `GITHUB_APP_ID`, `GITHUB_APP_SLUG`, `GITHUB_APP_PRIVATE_KEY` when you want that; see [`.env.example`](./.env.example) for these and everything else (OIDC, Google login, SMTP/Amazon SES, steer). Upgrades are `docker compose pull && docker compose up -d`; pin `IMAGE_TAG` to a [release tag](https://github.com/Niach/exponential/tags) if you'd rather move deliberately than track `master`.

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

[Exponential Small Team License 1.0](./LICENSE) — source-available, not open source. Read it, change it, and self-host it in production for free while your company, affiliates included, has **fewer than 10 people** (employees plus independent contractors). 10 or more needs a commercial self-host license — €590/yr up to 25 people, €1,900/yr up to 100, custom above ([pricing](https://exponential.at/pricing/), support@exponential.at). Evaluation, development, testing, and non-commercial research stay free at any size. Nobody may offer it to third parties as a hosted or managed service, whatever their size.
