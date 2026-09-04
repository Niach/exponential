# Exponential

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)
[![App Store](https://img.shields.io/badge/App_Store-iOS-0D96F6?logo=apple&logoColor=white)](https://apps.apple.com/app/exponential-vibecode-together/id6788189402)
[![Google Play](https://img.shields.io/badge/Google_Play-Android-34A853?logo=googleplay&logoColor=white)](https://play.google.com/store/apps/details?id=at.exponential)
[![Desktop](https://img.shields.io/badge/Desktop-macOS_·_Linux_·_Windows-18181b?logoColor=white)](https://exponential.at/download/?ref=github)

**The next generation dev platform for teams.** Issues, customer feedback, and coding agents in one realtime tracker. Agents run on your own machines, on your own subscription. Web, macOS, Linux, Windows, iOS, Android.

<p align="center">
  <img src="./docs/images/hero.webp" alt="The Exponential board on desktop, with the Start coding screen on iPhone" width="100%">
</p>

- **Cloud**: [app.exponential.at](https://app.exponential.at/?ref=github), free for up to three people
- **Self-host**: one `docker compose`, no seat caps, no licence gate
- **Desktop app**: [exponential.at/download](https://exponential.at/download/?ref=github)
- **Mobile apps**: [App Store](https://apps.apple.com/app/exponential-vibecode-together/id6788189402) · [Google Play](https://play.google.com/store/apps/details?id=at.exponential)

## What you get

- **Issues** with statuses, priorities, labels, due dates, markdown, @mentions. Realtime sync on every client via [ElectricSQL](https://electric-sql.com).
- **Boards backed by a GitHub repo**: one issue, one branch, one PR, tracked on the issue. Or one combined PR for a batch.
- **Start coding**: hand an issue to Claude Code, Codex, or pi from the desktop app. It plans, codes in a worktree, and opens the PR.
- **Live steer**: watch and redirect a running session from your phone.
- **Actions & automations**: reusable team prompts, run on demand or on a schedule or an issue event, on your own machines.
- **Headless CLI**: `exponential` turns any Linux or macOS box into an always-on agent machine your team starts runs on.
- **Feedback widget & helpdesk**: a script tag for your site; bug reports with annotated screenshots land as issues, support requests as email tickets in a shared inbox.
- **MCP server** at `/api/mcp` for Claude Code, Codex, Cursor, or any MCP client.

## Self-host

```sh
mkdir exponential && cd exponential
curl -fsSLO https://raw.githubusercontent.com/Niach/exponential/master/selfhost/docker-compose.yaml
curl -fsSL https://raw.githubusercontent.com/Niach/exponential/master/selfhost/.env.example -o .env
# fill in .env: two secrets + S3 credentials
docker compose up -d
```

App at `http://localhost`. Migrations run on boot. A GitHub App is only needed for the coding features. Full runbook: [`INSTALL.md`](./INSTALL.md), env reference: [`.env.example`](./.env.example). Upgrade with `docker compose pull && docker compose up -d`.

The only cloud-only feature is mobile push (store apps are built against the first-party Firebase project).

## Connect an MCP client

```sh
claude mcp add --transport http exponential https://app.exponential.at/api/mcp
```

## Development

Needs [bun](https://bun.sh), Node ≥ 20.19, and Docker Compose.

```sh
bun install
cp .env.example .env
cp Caddyfile.example Caddyfile
bun run backend        # docker compose up -d + web dev server (localhost:3000)
bun run storage:init   # one-time S3 bootstrap, prints keys for .env
```

```
apps/web        TanStack Start app: tracker, API, Electric shape proxies
apps/desktop    Rust IDE (gpui)
apps/ios        SwiftUI
apps/android    Kotlin / Compose
apps/marketing  exponential.at
packages/       db schema, widget, icons, contracts
```

Other scripts: `bun run ios`, `bun run android`, `bun run dev:desktop`, `bun run typecheck`, `bun run test`. Don't run `bun run lint` (its `--fix` breaks `typeof import()` sites). Architecture notes live in [`CLAUDE.md`](./CLAUDE.md).

## License

[Apache-2.0](./LICENSE). [Enterprise Support](https://exponential.at/contact/?ref=github) is available for self-hosters who want an SLA or deployment help.
