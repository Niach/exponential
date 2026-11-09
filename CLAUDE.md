# Exponential

Real-time issue tracker with coding agents. This file = REPO FACTS (< 12k chars, gated by `lib/emcp/context-budget.test.ts`). Product rules live in the team prompt every session gets (Settings → General) and in the fixtures under `packages/domain-contract/fixtures/*.json`, whose `$comment` IS the spec of each screen ×4; behaviour is fixture-locked, so read the fixture before the code.

## Tech stack

TanStack Start (React 19, TanStack Router/React DB) · PostgreSQL 18 via Drizzle (`snake_case`) · ElectricSQL (shape proxies, `@tanstack/electric-db-collection`) · Better Auth · tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync) · shadcn/ui on Tailwind v4 (OKLCH zinc, dark forced) · bun. Dev infra (compose): Postgres:54321, Electric:30000, Garage:3900, Caddy:3000, steer-relay:4002.

## Monorepo layout

```
apps/web          TanStack Start app (src/lib/trpc = one file per router; routes/api/shapes = one proxy per synced table; lib/sessions = the session tree; lib/emcp = eMCP's server half: the MCP server, the gate, audit, vault, upstream/)
apps/push-relay   Hono/Bun            apps/steer-relay  presence + control + eTalk calls WS hub (Bun; no session content since VAPP-43: viewers reach the device over eNet)
apps/site         exponential.at (Astro + Starlight: landing, docs, SDK docs, blog; README.md; ui.exponential.at 301s via scripts/ui-redirect.ts)   apps/styleguide   shot gallery + islands
apps/ios          SwiftUI (Tuist + GRDB; ExpCore/ExpUI)   apps/android  Jetpack Compose
apps/ede          Exponential Desktop (eDE internally, #NEW-10; package + binaries exponential-*): own Cargo workspace; crates/ede-compositor = smithay compositor (winit nested for dev, headless pixman tests)
apps/desktop      gpui IDE; crates/cli = the `exponential` daemon; crates/eagent = Agents, the server-free agent core (EXP-1426/1505, never an app-crate dep; `exponential-agent-runtime` = its one-release shim); crates/eagent-hub-core = the pure orchestration reducer replaying packages/eagent's fixtures (+ `-wasm`, EXP-1509); crates/coding = the launcher (runbook.md); crates/engine = ACP engine; crates/elocal = Local desktops (EDE-55); crates/eui-* + exponential-ui-ffi = the eUI SDK
packages/db-schema        Drizzle schema + zod/domain types (authoritative; never mirrored here)
packages/domain-contract  contract.json → per-language enums + the fixtures (×4 replay) + pure TS references
packages/etalk            @exp/etalk: eTalk's contract (voice, voice-call, talk-listen, meeting fixtures ×4 + TS references); Rust = crates/etalk*
packages/ecore            @exp/ecore: eCore's contract.json → Rust crate `ecore` + TS/Swift/Kotlin (ids, grants, verdicts, errors, envelope; types only, gated)
packages/eagent           @exp/eagent: Agents' orchestration spec (tree, bus, wake-ups, reports, caps, watchdog fixtures + a pure TS reducer); the server (lib/sessions) and eagent-hub-core replay it
packages/eengine          @exp/eengine: the Engine's contract (lifecycle, consent, capabilities, providers fixtures + the TS twin); Rust = crate eengine
packages/emcp             @exp/emcp: eMCP's contract (exposes, access, audit, vault, upstream fixtures + TS references); server half = apps/web/src/lib/emcp, Rust = crate emcp
packages/ui               @exp/ui: theme, shadcn set, shared primitives, islands
packages/design-tokens · icons (icons.json → TS/Swift/Kotlin/Rust) · emoji · electric-protocol · steer-ticket · widget · view-catalog (views.json, drift-gated) · shots (capture pipeline) · store-shots (store + social pictures) · exponential-ui{,-react,-solid,-swift,-compose} (the SDK, guides/ = the guide apps CI compiles; `-solid` = the Solid renderer + `./primitives` on `src/behaviors/`, parity-gated against React; both web renderers take their classes from the shared recipes `@exponential_app/eui/web/primitives`)
shots/            COMMITTED webp store, <view>/<platform>.webp + index.json
docs/             third-party-licences.md + licences/     selfhost/  pull-an-image compose + INSTALL.md
```

Workspace names `@exp/<dir>`; `apps/desktop` = Cargo (shared `CARGO_TARGET_DIR`). **Brand:** one Exponential SDK; copy says genUI, Motion, Voice, P2P, Computer Use, MCP Proxy, Agents, Core, Apps, Engine (ACP = only the Agent Client Protocol). Packages, crates, tags and code keep `e<lib>` (eui, etalk, enet, epilot, emcp, eagent, ecore, eengine = the Engine, Apps' runtime, tag `eengine-v*`; no `eapps`).

## Commands

```bash
bun install
bun run backend                 # docker compose up -d + dev server (:3000 via Caddy)
bun dev                         # web dev server (:5173)
bun run typecheck / test / test:e2e
bun run migrate / migrate:generate / psql / backend:{up,down,clear} / storage:init
bun run --filter @exp/{domain-contract,design-tokens,icons} generate
bun run ios / ios:test · bun run android · bun run dev:desktop / {build,test}:desktop
bun run shots -- --platform <p> --views <id> --skip-seed   # natives + desktop captures
cd apps/web && bun run seed:screenshots && bun run capture:views -- --form-factor web|web-mobile --views <id>
bun run build                   # widget FIRST, then web + site
bun run dev:site / build:site    # exponential.at (apps/site)
scripts/lanes/lane.sh <ios|android|heavy> <owner> -- <cmd>   # device + heavy-build lanes
```

Lane locks (`scripts/lanes/lock.sh`): work only after your own `take_lock`; free one only with `release_lock <dir> <owner>`, never `rm` (it refuses a lock another owner holds); benches/captures: `--own-target` (README).

Never lint/format. Schema change = edit `packages/db-schema/src/schema.ts`, `bun run migrate:generate && bun run migrate`; custom triggers in `apps/web/src/db/out/custom/0001_triggers.sql` apply at boot (`applyCustomSql`; `bun dev` does not run it: apply by hand in dev). Enum change = `contract.json` + regenerate ×4. Icon change = `packages/icons/icons.json` + regenerate; `pickable`/`devicePickable` are APPEND-ONLY. New styleguide entry = five sites (sections.json, entries/index.ts, entries/<id>.tsx, page.ts ENTRY_BAND, desktop styleguide twin). Every user-facing release PREPENDS a `ChangelogEntry` to `lib/changelog.ts` (mirror `crates/ui/src/changelog.rs`).

## Deploys

Coolify (home-LAN-only): `coolify deploy uuid <uuid>` after a green Actions run and a clean `check:device-shares` on the target DB (`.github/workflows/README.md`). `build-web.yml` publishes `ghcr.io/niach/exponential-web` on master pushes + `v*` tags: ONE image for cloud, staging and self-host. Native releases = tags `{android,desktop,cli,ios}-v*` → `build-*.yml`. The operations runbook lives outside the repo.

**CI:** pull requests run ONE gate, `test.yml` (required check `gate`, < 5 min; ~15 with eUI): typecheck, web unit tests ×4 shards, web production build when web or `packages/` is touched, migrations from scratch + drift + triggers, desktop `cargo check` + eagent/coding/engine/ui tests when touched, eUI conformance ×5 renderers + `conformant` when eUI is touched (`exponential-ui.yml` `conformance_only`); the Mac-runner legs (`ios-core` ExpCore replay, `ios-app` app-target compile, `eui-swift`) run on PRs as informational checks outside `gate` and block in `release-suite.yml`. Everything heavy (the rest of the eUI suite + bench, iOS, Android, desktop `cargo test`, site + `shots:check`, package + real-DB suites, image smoke builds) = `release-suite.yml`, on the release tags (once per commit) and `workflow_dispatch`. Bench timings only warn.

## Conventions that gates enforce

- **Vocabulary:** the product says **team**, **board**, **session** everywhere (copy, URLs `/t/$teamSlug/boards/$boardSlug/issues/$id`, DB, routers, MCP); never workspace/project/run/thread.
- **Same UI ×4:** one styleguide; prefer existing components; a new one goes into the styleguide and, if low-level, into Exponential UI (`packages/exponential-ui*`). Fixture-locked behaviour (`fixtures/*.json`) replays on web, desktop, iOS and Android; a change edits the fixture first. Shots live in `shots/`; `pr open` refuses a UI diff without a picture per platform.
- **Shapes:** every synced table has one proxy in `routes/api/shapes/` pinning a server-side `columns` allowlist (server-only columns stay BEHIND it); every shape is member-only; board-scoped shapes carry `team_id` + static trash/archive mirrors; the session tree uses the visibility rule (`shared` or starter). Clients sync the same 33 shapes (#EXP-1730: messages keyed by `session_id`; `conversations` only for old builds); web and desktop add `skills` (34th; phones with #EXP-1661). Collections (`lib/collections.ts`) use `snakeCamelMapper`; `undefined` skips a query; `and()`/`or()` from `@tanstack/react-db`. Web `messages` is ON DEMAND (#EXP-1380): every query narrows by `rootId`/`sessionId` (the proxy forwards `subset__*` only over its own columns, `subsets: true`); lists read `sessions.digest` (`useDigestMessages`), never transcripts.
- **Access:** `lib/auth/access.ts` `resolveTeamAccess` is the one authority; owners own the destructive/settings surface. Billing + admin = web-only; coding sessions run only on the desktop app and the CLI daemon.
- **Markdown:** `issues.description` + `comments.body` = plain GFM, one interchange ×4 (`CONTRACT_FIXTURES`); mentions `@<email>`, issue refs `#IDENT`, images `![alt](/api/attachments/{id})`, emoji = unicode.
- **Statuses:** issues dual-write `status` (anchor enum) and `statusId` (per-team row); PR open/merge move linked issues; nothing else moves status (`issue-lifecycle.json`).
- **Sessions (N-10):** ONE `sessions` tree, root = worker; the server bus posts every child transition into its parent as a WAKING message; the watchdog runs every minute; `sessions.send` (`lib/sessions/send.ts`, tRPC + MCP) is the one entrypoint; agents = definitions (`agents`) running as one conversation session per member; prompts replace actions; events are the team stream.
- **MCP (NEW-4):** 22 tools in 7 groups (`expToolDisplay.groups`; nouns + the verbs sessions_send, show; results replaced guide_write; `lib/emcp/nouns.ts`, `op` routes to the per-operation definitions in `tools.ts`); display rows `<noun>_<op>` in `contract.expToolDisplay`; budgets: always-loaded < 10k, per tool < 1.8k (show: its own cap), instructions < 2k with a self-contained first 512, runbook < 6 KB, this file < 12k.
- **Style:** template literals; functional components; shadcn/ui from `@exp/ui` over raw elements; icons by CONCEPT (`conceptIcon(\`nav-search\`)`), never a raw glyph; feedback ×4 = `feedback-states.json` (failed action = toast, invalid input inline, skeletons); no explanatory subtitles.

## Environment variables

Root `.env.example` = CANONICAL, `selfhost/.env.example` its subset, relays own `apps/*/.env.example`. `CLOUD_INSTANCE=true` turns on billing, plan limits, the in-app widget and conversion tracking (unset = self-hosted, no feature limits). Auth posture is BUILD-derived (`lib/production-build.ts`). GitHub = ONE flow on the App's OAuth client (`GITHUB_APP_CLIENT_ID/SECRET`). `STEER_RELAY_URL` unset = device presence, nudges, kill/Stop/compact and calls off. `CLIENT_MIN_VERSION_*` gate with HTTP 426.

## Dead, never reintroduce

Releases/footage, `@exp/video`, workspace/project vocabulary, board types, `agent_runs` + agent-core + the companion daemon, `run_configs`, `claude_task`, `isProtected` boards, due-date time-of-day, `SELF_HOSTED`, the builtin `todo` status, skip-permissions, PTY coding, standalone automations, the pi agent + external ACP agents, issue filtering, workflows, stacked runs + `pr_stack_number`, the helpdesk, `coding_sessions`/`threads`/`thread_items`/`session_attachments`, batch/chat/action session types, builtin actions, Reviews, right sidebars, merge-ends-session, actions (now prompts), device-armed triggers, `/clear` + `/btw`, the CLIs' native plan modes, the 79 per-operation MCP tools as a surface (they are ops now), the voice tool policy (every session bypasses permissions), threads (a chat IS a root session; OS threads aside).
