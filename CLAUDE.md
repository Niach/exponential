# Exponential

Real-time issue tracker.

## Tech Stack

TanStack Start (React 19, TanStack Router/React DB) · PostgreSQL 17 via Drizzle (`snake_case` casing) · ElectricSQL (shape proxy pattern, `@tanstack/electric-db-collection`) · Better Auth (email/password + OIDC `genericOAuth`, session-based, `tanstackStartCookies`) · tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync) · shadcn/ui on Tailwind v4 (OKLCH zinc, dark forced via `html.dark`; dates via `react-day-picker` + `date-fns`) · bun. Dev infra: Docker Compose — Postgres:54321, Electric:30000, Garage:3900 (S3), Caddy:3000 (HTTP/2 proxy; `Caddyfile` gitignored, copy `Caddyfile.example`), optional steer-relay:4002 (`--profile steer`).

## Monorepo Layout

```
apps/
├── web/        # TanStack Start app (the issue tracker)
├── push-relay/ # Push notification relay (Hono/Bun)
├── steer-relay/# Remote-start + live-steer WS hub (Bun; in-memory presence/rooms)
├── marketing/  # Vite + React; owns the Remotion ClosedLoop hero movie (src/movie/)
├── ios/        # SwiftUI (Tuist + GRDB; ExpCore/ExpUI)
├── android/    # Kotlin / Jetpack Compose
├── styleguide/ # Static shot-gallery site (reads shots/ + @exp/view-catalog)
└── desktop/    # Rust IDE (gpui + gpui-component + rio-vt; embedded coding sessions)
                # + crates/cli: the headless `exponential` CLI/daemon (EXP-403), gpui-free via the
                # default-on `gpui` cargo feature on terminal/coding; own cli-v* release train
packages/
├── db-schema/         # Drizzle schema + shared zod/domain types
├── design-tokens/     # OKLCH→sRGB + motion tokens → Compose/SwiftUI/Rust
├── domain-contract/   # contract.json — canonical enums → per-language constants
├── icons/             # icons.json — the ONE icon registry → TS/Swift/Kotlin/Rust + SVGs
├── electric-protocol/ # Shape wire contract + cross-platform fixtures
├── emoji/             # emoji dataset generator → ONE json into all four clients
├── steer-ticket/      # HS256 ticket sign/verify (web mints, relay verifies)
├── widget/            # Feedback widget (Preact + snapDOM) → apps/web/public/widget/v1/
├── view-catalog/      # views.json — every product view × platform, drift-gated
├── shots/             # capture pipeline (sharp diff-skip writer) → shots/
└── tsconfig/
docs/                  # third-party-licences.md + licences/ (runbooks live outside git)
shots/                 # COMMITTED webp store, <view>/<platform>.webp (EXP-566)
docker-compose.yaml    # DEV backend stack (not the self-host one)
selfhost/              # Pull-an-image compose; INSTALL.md = agent-followable runbook
Dockerfile{,.push-relay,.steer-relay}   # build context = repo root
```

Workspace names: `@exp/<dir>`; `apps/desktop` is a Cargo workspace, not a bun workspace. The only movie is the ClosedLoop hero in `apps/marketing/src/movie/`, embedded via `@remotion/player` in the lazy `LoopMoviePlayer` chunk; `LoopMovie.tsx` stays remotion-free (`scripts/prerender.tsx` renders it under Bun).

**Dead — never reintroduce:** releases + release footage/fixtures (EXP-106), an `@exp/video` workspace, workspace/project vocabulary and `/w/`+`/projects/` URLs (EXP-180, no redirects), board types, `agent_runs` + agent-core + the companion daemon + the synthetic `isAgent` identity, the `assigned-issues` shape, `run_configs`, the one-shot `claude_task` primitive (EXP-259), `isProtected` boards and every `feedback`-team/dogfood case (EXP-364), due-date time-of-day (REV2-49), `SELF_HOSTED` (replaced by `CLOUD_INSTANCE`), and env vars `GOOGLE_CALENDAR_ENABLED`/`DOGFOOD_REPO`.

## Product Invariants

**Vocabulary (EXP-180):** the product says **team** and **board** EVERYWHERE — copy, URLs (`/t/$teamSlug/boards/$boardSlug/issues/$id`), identifiers, DB, routers, shapes, MCP tools. Boards have no types; `repository_id` is NULLABLE (FK restrict); coding gates purely on repo PRESENCE.

**Client parity:** all four clients (web, iOS, Android, desktop) sync the same 19 Electric shapes — the file list in `routes/api/shapes/` IS the list. The `actions` shape EXCLUDES the ≤64KB `body` (tRPC `actions.get`). `devices` + `device_worktrees` (EXP-481) sync `user_id = me OR shared_team_id IN (member teams)` via trigger mirrors — identity rotates only on membership changes; devices rows are SERVER-AUTHORITATIVE (persisted `launch_defaults` the machine's settings.json converges to; heartbeat ~30s doubles as the work pull, online = `last_seen_at` freshness per `contract.device.onlineWindowSeconds`). `repositories`, `user_notification_prefs`, `email_deliveries`, `conversion_events`, `device_commands` (EXP-481 owner→device queue: worktree remove/prune, delivered on heartbeat + relay `check_in`), helpdesk and widget tables are **server-only (tRPC), never synced**.

**Nothing is anonymously readable:** every shape is member-only (anonymous → impossible-match sentinel); no public tRPC; attachment reads require membership. The ONLY anonymous endpoints: widget (`/api/widget/*`), helpdesk reporter magic-link (`/api/support/*` + `/support/$token`), invites, auth, and the static licence notices `/about` + `/NOTICES.txt` (EXP-262). Board-scoped shapes are TEAM-scoped with a STATIC trash predicate (REV2-5): `team_id IN (member teams) AND board_deleted_at IS NULL`, a trigger-maintained mirror of the board's `deleted_at` on every issue child — trashing a board moves rows out incrementally instead of rewriting a where clause, so **shape identities rotate ONLY on team-membership changes**. Batch `coding_sessions` rows and issue-less notifications keep NULL `board_deleted_at` and always sync; the notifications shape is fully static per user (`user_id = me AND board_deleted_at IS NULL` — fan-out filters recipients at delivery, delivered rows outlive membership); issue-less `support_reply` rows carry a synced `team_id` for Support routing.

**Permissions are membership-only:** `lib/auth/access.ts` `resolveTeamAccess` is the single authority (capabilities `read`/`comment`/`create_issue`/`mutate_resources`), mirrored by `use-team-permissions.ts` + the natives; every member moderates and handles support, owners own the destructive/settings surface (owner-only controls are HIDDEN for non-owners; destructive native actions confirm). Signups get **no** team: first-run is create-or-join — "Create a team" (`teams.create`, any authed user; creator = owner; cloud free tier capped by `FREE_OWNED_TEAMS_CAP`) or "Join a team" (invite link; `teamInvites.accept` stamps `onboardingCompletedAt`; invites may carry a synced `email` and the server mails the link). `teams.getDefault` is the NON-CREATING default-team resolver (oldest membership or null). An owner may delete ANY team incl. the last (cloud: only after its subscription is cancelled). **Billing and the admin console are web-only. Coding sessions run only on the desktop app and the headless `exponential` CLI daemon** (EXP-403: same launcher/agents/steer machinery, per-user devices, login via RFC 8628 device code at `/auth/device`), both publishing to the steer relay (scrubbed activity channel only); since EXP-312 a LIVE session is visible/steerable ONLY by its owner — no view/steer perm split, no `perm`/`name` ticket claims; teammates see the synced status badge.

The app is **noindex** everywhere: `__root.tsx` meta + ungated `X-Robots-Tag: noindex` from `server-bun.ts` (self-hosted too); `/robots.txt` ALLOWS crawling (`Disallow: /api/` only) so crawlers see the noindex; no sitemap. Marketing owns the indexed surface (own robots.txt + `dist/sitemap.xml`/`llms.txt` from the `PAGES` manifest in `src/lib/seo.ts`).

## Shared Contracts

**Icons (EXP-273/317):** ONE icon set — Lucide — byte-identical on all four clients, generated from `packages/icons/icons.json` into COMMITTED per-platform outputs. Change an icon by editing `icons.json` + `bun run --filter @exp/icons generate`; never hand-edit generated files or add per-platform glyph maps. `pickable` (the 60 board/action picker names, byte-equal to contract `boardIcon.values`) is **APPEND-ONLY** — reordering orphans stored rows. Multi-client surfaces name a CONCEPT (`conceptIcon(\`nav-search\`)` / `registry::NAV_SEARCH` / `AppIcons.navSearch` / `ExpIcons.navSearch`), never a raw glyph; iOS renders via `AppIcon`, NOT `Image(systemName:)`; `apps/web/src/lib/icons.test.ts` gates drift and the concept rules. Desktop assets separately hold hand-maintained BRAND marks (`claude`/`codex`/`pi`/`logo*`/`apple`/`google`) the generator must never own.

**Enums:** canonical values live in `packages/domain-contract/contract.json` (support-thread `status`/`direction`/`visibility` are documented varchars in `domain.ts` only). Changing `db-schema/src/domain.ts` means updating `contract.json` + regenerating.

**Markdown:** `issues.description` + `comments.body` are plain `text` GFM — one interchange across web (TipTap + tiptap-markdown), iOS (cmark-gfm), desktop (comrak + the vendored WYSIWYG `crates/gpui-markdown-editor` [Apache-2.0 Velotype, see NOTICE]) and Android (`ui/markdown/`, commonmark-java). The round-trippable feature set IS `CONTRACT_FIXTURES`, byte-locked by desktop `wysiwyg_parity.rs` and the native mirrors — read the fixtures, not a prose list. **Underline intentionally unsupported**; tables, slash commands, image resize are out of scope. **Mentions** are plain `@<email>` — the server resolves members, fires `issue_mention` and auto-subscribes (`lib/integrations/mentions.ts`); clients render known members as pills and @-autocomplete inserts the plain form. **Issue mentions** are plain `#<IDENTIFIER>` (token contract `lib/issue-refs.ts`, per-client lookup, `#` autocomplete ×4); a pill renders only when it resolves to a synced same-team issue, and `resolveIssueRefs` fires NO notifications yet (deliberate). Images are stored relative `![alt](/api/attachments/{id})` — the server canonicalizes on save (`canonicalizeMarkdownImageUrls`), clients absolutize at fetch, attachments carry probed `width`/`height`. **Emoji** (EXP-551) are inserted as unicode, never `:shortcode:`; picker + `:` typeahead data is generated ONCE by `packages/emoji` into all four clients (drift-gated).

## Commands

From repo root unless noted.

```bash
bun install
bun run backend                    # docker compose up -d + dev server (app on :3000 via Caddy)
bun run ios                        # tuist generate + Xcode (Mac-only)
bun run ios:test                   # ExpCore+ExpUI suites (Mac-only; NOT in CI — run before ios-v* tags)
bun run android                    # install productionDebug + launch
bun dev                            # web dev server (localhost:5173)
bun run dev:marketing / build:marketing
bun run movie:studio / movie:render / movie:poster / movie:still <out.png> --frame=N
bun run dev:push-relay / start:push-relay        # localhost:4001
bun run dev:steer-relay / start:steer-relay / test:steer-relay   # localhost:4002
bun run build                      # widget + web + marketing (widget MUST build before web)
bun run build:web / build:widget / test:widget
bun run dev:widget                 # watch-build (with bun dev; /widget/v1/demo.html)
bun run typecheck / test / test:e2e   # web app
bun run migrate / migrate:generate / psql
bun run backend:up / backend:down / backend:clear   # clear wipes volumes
bun run storage:init               # one-time Garage bootstrap
bun run format
bun run android:build / android:install
bun run dev:desktop                # gpui IDE against local backend
bun run build:desktop / appimage:desktop / macapp:desktop / test:desktop
bun run clean:desktop              # on zed/gpui rev bumps
bun run --filter @exp/{domain-contract,design-tokens,icons} generate
cd apps/web && bun run seed:screenshots        # demo data (shots + store captures reuse it)
bun run shots                                  # all-platform view captures → shots/ (--platform/--since)
bun run screenshots:store                      # ASO slide compositor → store upload dirs (--proposals)
```

Workspace scripts: `bun --filter @exp/web <script>` or `cd apps/web && bun run <script>`; plain `cargo` from `apps/desktop/`. The generated Rust files (`contract.generated.rs`, `tokens.generated.rs`) are committed; regenerate only when `contract.json`/`tokens.json` change. Do NOT run `bun run lint` — its --fix corrupts `typeof import()` sites (EXP-13).

## Deploys

Everything runs on Coolify (`coolify.home.straehhuber.com`, Hetzner). **Coolify is home-LAN-only, no auto-redeploy webhooks**; after a green Actions run, deploy from a LAN machine with `coolify deploy uuid <uuid>`. `build-web.yml` publishes `ghcr.io/niach/exponential-web` on master pushes + `v*` tags, multi-arch; the SAME image is cloud, staging and the self-host distribution (`selfhost/`), so the ghcr package stays PUBLIC and self-hosters pin semver tags. Its runtime `bun install` is `--filter '@exp/web'` (EXP-380: unfiltered redistributed marketing's source-available Remotion); non-OSS components and their notices rules live in `docs/third-party-licences.md`, gated by `lib/third-party-licences.test.ts`. Native releases are tag-triggered: `android-v*` (APK + Play bundle, `make_latest: false`), `desktop-v*` (`build-desktop.yml`: codegen-drift guard, production + staging × macOS/Linux/Windows; `make_latest: true`, self-update in `crates/updater`), `cli-v*` (`build-cli.yml`: bare `exponential-<target>` binaries, `make_latest: false`; installed via `apps/marketing/public/install.sh`, one script for cloud AND self-host via `EXP_INSTANCE`), `ios-v*` (`build-ios.yml`: ASC upload from a release-macOS runner, beta-macOS ipas are rejected; `ASC_*` secrets, manual review).

**The operations runbook lives OUTSIDE the repo** (infra uuids/domains stay out of git; the operator keeps it in agent memory). It covers app/DB/Electric uuids, buckets, staging, the relays' required `TRUST_PROXY=true`, per-platform release steps and signing, and the release checklist (changelog, GitHub App settings, deep-link assets, cloud auth env, health checks, DNS). Consult it before touching anything deploy-shaped; never re-inline it.

Every user-facing release PREPENDS a `ChangelogEntry` to `apps/web/src/lib/changelog.ts` (`changelog.test.ts` enforces conventions; head id drives the "What's new" card).

After schema changes, always: `bun run migrate:generate && bun run migrate`. Custom SQL triggers (`db/out/custom/0001_triggers.sql`) auto-apply at every app boot (`bootstrap-cloud.ts` `applyCustomSql`, idempotent, self-hosted too); only never-booting contexts need manual psql (CI's schema job does).

## Web App Structure (`apps/web/src/`)

Trust the filesystem; this records only what `ls` can't tell you. Shadcn lives in `components/ui/`, feature components flat in `components/` (`agent-session.tsx` is the custom steer/activity view, no xterm). `lib/trpc/` is one file per router and `routes/api/trpc/$.ts` is the authoritative router list. In `lib/auth/`, `membership.ts` holds data lookups and `access.ts` authorization (`resolveTeamAccess`) — nothing else decides "can user X do Y". `lib/notification-email-policy.ts`/`-digest.ts`: push fires on create, email is a DIGEST of still-unread (DAILY at a user-chosen local hour, hourly legacy opt-in, atomic `emailed_at` claim, scheduled from `server-bun.ts`). Team routes under `t/$teamSlug/`: index, inbox (`?tab=my-issues` is a TAB, not a route), agents (the Actions surface), reviews + `reviews/$issueIdentifier` (cross-board open-PR queue with a confirmed one-click squash merge), support, settings/*, boards/$boardSlug + issues/$issueIdentifier. Also `auth/consent.tsx` (MCP OAuth scope picker), `invite/$token`, `api/shapes/`, `api/mcp.ts`, `api/webhooks/github.ts`. Entry: `router.tsx`, `start.tsx` (`defaultSsr: false`), `server{,-bun}.ts`.

## Database

`@exp/db-schema` is authoritative — never mirror it here.

### Conventions

Better Auth user IDs are `text` — all user FKs must be `text`; app tables use UUID PKs (`gen_random_uuid()`). All tables carry timezone `created_at`/`updated_at`; sort orders are `doublePrecision` (fractional indexing); rich text is `text` GFM. Due date is `date` only.

### Non-obvious fields

Issues DUAL-WRITE `status` (the builtin ANCHOR enum) and `statusId` (nullable FK `issue_statuses` SET NULL — the precise per-team row); `creatorId` is NULLABLE (widget issues have none) and `source` is `user`/`widget`; `duplicateOfId` pairs with status `duplicate`; the PR fields (`prUrl`/`prNumber`/`prState`/`branch`/`prMergedAt`) mean ONE PR per issue on branch `exp/<IDENTIFIER>`, and batch issues share ONE `prUrl`. `coding_sessions` is issue XOR batch XOR action-scoped (action rows carry `action_id` [set null] + an `action_name` snapshot). Teams carry a server-only `compTier` plus synced `helpdeskEnabled` and PR automation (`prOpenedStatusId`/`prOpenedAutomation`/`prMergedStatusId`/`prMergedAutomation`: nullable FKs SET NULL where NULL = the builtin default target and `*Automation=false` = "do nothing"; member-gated `statuses.setPrAutomation`; UI web + desktop).

### Enum behavior

Values in `contract.json` (§Shared Contracts). `issue_status` — `pr_open` flips linked issues to the team's PR-open target (default `in_review`), merge to the PR-merge target (default `done`). `coding_session_status` (running/in_review/ended): `in_review` = PR open; PR MERGE **ends** live sessions on EVERY path (EXP-498: `applyPrMergeState`, the `endMergedPrSessions` sweep, `endSessionsOnMergedBranch` for chore PRs, relay kill) EXCEPT the session that merged its OWN PR — server-only `merged_own_pr` (EXP-637), stamped by MCP `pr_merge` via the session header, filters every merge-driven end; that run ends only via its own exit/tab close (or its close-out, if automation-started). Legacy `merged` is an orphan PG enum label only (EXP-540); `ended` also comes from `killSession`/`codingSessions.end`; the desktop CLOSES the tab on its own row's →ended edge. `ended_by` (agent|user|client|merge|system) records WHICH path; `summary`+`outcome` (done|blocked|no_changes) are written ONLY by `exponential_sessions_end`, which ENDS the row only for automation-started runs (`started_reason` set, EXP-673) — a person-started run records them and stays live for replies with NO idle bound, CLI daemon included (EXP-674); the Automations tab's "Recent automated runs" is the ONLY finished-runs list (EXP-676), keyed on `outcome`, not `ended_by`; `resumed_from_id` links a resumed run to the one it continues. Batch sessions (issue_id NULL) self-close on the desktop when their branch's issues sync `prState=merged`.

### Custom triggers

`apps/web/src/db/out/custom/0001_triggers.sql` holds 15 functions, each commented with its rationale — read it before changing anything trigger-adjacent. They guarantee: per-board issue numbers + `{prefix}-{number}` identifiers; `updated_at` maintenance (an issue bumps when its comments change, NOT when the trash fan-out rewrites them); denormalized `team_id`/`board_id` + the two board-hide mirrors onto every issue child and onto notifications (no-op when `issue_id` is NULL, so batch rows carry explicit ids); board trash and archive fan-outs to those mirrors; the 7 builtin `issue_statuses` per team from contract `issueStatusDefaults`; `status_id` derived from the anchor for enum-only writers (explicit dual-writes are never overridden); `user_id`/`shared_team_id` mirrored onto `device_worktrees` with share-change fan-out (EXP-481); a `team_ids` membership mirror on `users` (+ boot heal pass) powering the users-shape where clause (REV-37); and `creem_subscriptions.creem_subscription_id` immutable once set (REV-12, blocks the Creem plugin's customer-id fallback re-keying a row).

## Patterns

### Electric shape proxies

One proxy per synced table in `routes/api/shapes/`, built with `createShapeRouteHandler`; all member-only. **Every proxy pins a server-side `columns` allowlist clients cannot widen, and a new server-only column on a synced table goes BEHIND it** — an unknown column reaching a native client bricks its sync loop. `users` pins exactly `id,name,email,image,created_at,updated_at`; `teams` its contract list (`comp_tier` never); `issue-subscribers` drops reporter `email` (PII); `actions` drops `body`; board-scoped shapes drop their scoping columns. A shape may FILTER on a column its allowlist excludes — Electric evaluates `where` server-side. Non-negotiable hardening: responses always carry `cache-control: private, no-store` + `vary: authorization, x-api-key, cookie`; failing token credentials get an explicit 401, never the anonymous where clause (cookie-only still falls back anonymously); `buildWhereClause` SORTS id lists because the where clause is part of Electric's shape identity, and membership id lists stay OUT of board-scoped where clauses (`buildTeamScopedChildWhere`). A FIFO semaphore in `electric-proxy.ts` bounds ALL snapshot-class forwarding — any request without `live=true` (REV-27); live long-polls never gated. Wire format, control messages and long-poll timeout floors: `packages/electric-protocol/README.md`.

### Board trash (48h soft delete) + archive (EXP-500)

Owner-only `boards.delete` stamps `deleted_at`; **archive is the same machinery minus the purge** (`archived_at` + a `board_archived_at` child mirror + its own fan-out). Both vanish server-side — `boardVisible()` (`lib/board-visibility.ts`, the ONE predicate every boards join takes) + the shapes' static `IS NULL` suffixes + trigger-fanned mirrors — never client-side: that leaked once (REV2-103), so `archived_at` is in no allowlist. Both keep the `(team_id, slug)` reservation. `restore`/`listDeleted` power the web-only "Pending deletion" card, `archive`/`unarchive`/`listArchived` an "Archived boards" one. The purge sweep (`lib/board-trash.ts`) keys on `deleted_at` alone: hard-delete after 48h + S3 reclaim, as do team/admin deletes (collect `storage_key`s in-tx, delete after commit). Widgets mutations + the helpdesk toggle are owner-only (helpdesk is paid-gated).

### Custom issue statuses (EXP-314)

Per-TEAM rows in six fixed categories (backlog/unstarted/started/completed/cancelled/duplicate; contract carries values + ONE `displayOrder` — settings sections, pickers and list groups all speak it since EXP-448 — + `startedMax: 4`). 7 LOCKED builtins per team (`builtin_key` = legacy enum values, never renamed/recolored/deleted, movable within category; seeded by trigger + migration; defaults live ONCE in `issueStatusDefaults`). Customs are name+color, member-managed (`statuses` router; started ≤4; delete requires reassign; management UI web+desktop, mobile renders/picks). `issues.status` STAYS the dual-written **anchor** (`CATEGORY_ANCHOR`: backlog→backlog, unstarted→todo, started→in_progress, completed→done, cancelled→cancelled) so enum-keyed subsystems (completedAt derivation, pr-sync from-sets, MCP enum tools, old clients) keep working; PR open/merge TARGETS are per-team configurable. `statusId` carries the precise row; `populate_issue_status_id` re-anchors enum-only writers. Resolution, colors and the positional pie-clock glyphs derive in `lib/team-statuses.ts` + `lib/status-icons.ts` — hand-mirrored ×4, lock-tested, fallback chain never fails. Lists group by status ROW; filters are group-key sets (web accepts legacy enum URL tokens). Duplicate: no customs, out of pickers, enum+`duplicateOfId` lockstep.

### Web plumbing

- **Collections** (`lib/collections.ts`): all use `columnMapper: snakeCamelMapper()` — without it `useLiveQuery` `where` silently fails. `undefined` (not `false`) skips a query; use `and()`/`or()` from `@tanstack/react-db`, not `&&`/`||`.
- **Auth guard**: `_authenticated.tsx` `beforeLoad` + `throw redirect()`; session fetched once via `fetchSessionOnce()` and cached. Mutations via tRPC; `generateTxId` lets the client await Electric sync.
- **MCP OAuth consent**: `lib/auth/mcp-authorize-guard.ts` pre-flights every `mcp/authorize` (actionable HTML errors; forces `prompt=consent`) → `/auth/consent` team/board multi-select persisted to `mcp_grants` BEFORE the code mints. `lib/mcp/scope.ts` confines OAuth tokens to the grant (no grant row = nothing); OAuth tokens are accepted ONLY at `/api/mcp` (shapes/tRPC reject); cookies + `expu_` keys keep full access. Login resumes interrupted authorizes and drops the `oidc_login_prompt` cookie (`lib/auth/oauth-resume.ts`).
- **Issue UI**: issue detail is a full-page route fed a live `issue` from Electric; title/description save on blur, other fields mutate immediately; `completedAt` is auto-managed by the update mutation. Filters in `lib/filters.ts`. Borderless dialog inputs `border-none shadow-none focus-visible:ring-0`; icon-only dropdown triggers `Button variant="ghost"` `h-5 w-5 p-0`.

## Environment Variables

**`.env.example` at the repo root is the CANONICAL reference** (required core, optional subsystems, cloud-only) and `selfhost/.env.example` is its self-host subset — read them instead of a list here; each relay has its own `apps/*/.env.example`. What is not obvious from those files:

- `CLOUD_INSTANCE` is the opt-IN cloud marker (EXP-364): `'true'` turns on billing, plan limits, the in-app widget and conversion tracking; unset = self-hosted, every FEATURE limit unlocked.
- `AUTH_PASSWORD_ENABLED`/`AUTH_SIGNUP_ENABLED`: password login defaults true, public signup is on in dev and OFF in production builds — but `selfhost/docker-compose.yaml` re-defaults it to `true`. Auth posture is BUILD-derived (`lib/production-build.ts` `isProductionBuild`, REV-5) — never key it on runtime `NODE_ENV`.
- Mail: SES (`AWS_SES_REGION` + creds) OR `SMTP_*` for ALL mail — SES wins if both; unset region = email off.
- OIDC: `OIDC_PROVIDERS` (JSON array) is the primary mechanism; the single-provider `AUTH_OIDC_ENABLED`/`OIDC_*` vars are legacy and only read when it is unset.
- GitHub App installations are claimed PER TEAM (`github_installation_links`); `GITHUB_APP_CLIENT_SECRET` unset ⇒ install-page round-trip fallback; `GITHUB_POLLING=true` = outbound merge cron for NAT'd self-hosts.
- `STEER_RELAY_URL` unset = remote start/steer off (LAN is fine, the desktop dials OUT); the HS256 `STEER_RELAY_SECRET` must match the relay, and BOTH relays need `TRUST_PROXY=true` behind a reverse proxy.
- `INITIAL_ADMIN_EMAILS` auto-promotes global admins — without it the admin console is unreachable.
- `CLIENT_MIN_VERSION_{ANDROID,IOS,DESKTOP}` gate with HTTP 426 + a blocking update screen (unset = off); always MARKETING versions, never versionCode/build numbers. `CLIENT_LATEST_VERSION_*` is informational.
- `BUN_CONFIG_MAX_HTTP_REQUESTS` is baked to 65336 in the image (REV2-6: long-polls saturate Bun's 256 default).
- Widget submit limits: refill is `WIDGET_RATE_LIMIT_PER_{KEY,IP}_HOURLY` (60/60), burst is `WIDGET_RATE_LIMIT_{KEY,IP}_BURST` — the burst names have no `PER_`. The KEY pair is self-host-only: cloud swaps the per-key bucket for a per-TEAM plan ceiling (`lib/widget/submit-limit.ts` — free 60/h, paid/comp unlimited); the IP pair applies everywhere, and `RECIPIENT` (6/h, burst 3) bounds support-mode confirmation mail per typed-in address on every tier. The anonymous config GET has its own generous `WIDGET_CONFIG_RATE_LIMIT_{PER_IP_HOURLY,IP_BURST}` (600/60, REV-25 — one NAT'd IP fronts many visitors).

## Coding sessions & Actions

### The launcher (v2 — "Start coding")

A thin launcher in the desktop IDE: resolve the issue's repo from the team registry (tRPC) → mint a session-gated JIT GitHub-App installation token → create a worktree + `exp/<IDENTIFIER>` branch with ambient git auth via a repo-local credential helper (EXP-73: `origin` stays the bare URL; token in `<clone>/.git/exp-git-credentials`, refreshed on GitHub's REAL `expires_at`; every writer goes through `coding::git_credentials::ensure`) → write the MCP config pointing at `/api/mcp` with the user's personal `expu_` apikey — deliberately NOT `.mcp.json`, which agent CLIs auto-scan and prompt on; it rides `--mcp-config` → spawn the agent CLI with a plan-first prompt. The agent commits, pushes, opens its own PR via MCP `open_pr`, then reports via `exponential_sessions_end` (EXP-637; the MCP config carries `X-Exp-Session-Id: <coding_sessions.id>` so each tool call knows its run). Action and chat runs get their OWN worktree + branch (`exp/<slug>-<id8>` / `exp/chat-<id8>`, lowercase so `parseIssueIdentifierFromBranch` can't match) — agents never write to the trunk; clean run worktrees are removed at run end. Local deps: `git` + agent CLIs — never `gh`. A slim synced `coding_sessions` row powers the cross-client "coding now" badge. Repo default branches resolve live at connect and heal on `repositories.list`/`installationToken` — never assume `main`; a team-pinned `default_branch_override` (EXP-462) wins everywhere and every `defaultBranch` payload field already carries it. `docs/masterplan.md` is gitignored.

### Three agents

EXP-201; `coding/src/agent.rs`, contract `codingAgent`; savable default + per-agent path/model/effort in settings. **claude** (default: `--permission-mode auto` guarded, `--dangerously-skip-permissions` via checkbox, `--permission-mode plan`), **codex** (workspace-write Auto preset or `--dangerously-bypass-approvals-and-sandbox`; MCP via `-c mcp_servers.*` argv, key rides spawn env `EXP_MCP_TOKEN` only), **pi** (no permission system/native MCP: the launcher writes a `.exp-pi-mcp.ts` extension loaded via `-e` bridging MCP over HTTP; `EXP_MCP_URL`/`EXP_MCP_TOKEN`). Ultracode is Claude-only; plan mode is claude+pi (pi via `.exp-pi-plan.ts`; remote-approvable plan card); skip-permissions is claude+codex. The doctor probes all three but gates only the SELECTED agent (git always required); `devices.register` records the runnable agents so remote pickers offer only what the device runs (`steer.startSession` validates server-side; absent agent = claude). Resume relaunches the newest `runs.json` record for the subject (EXP-662; issue/batch/action/chat alike); `.exp-agents` gates the native tail.

### Batch runs

Multi-issue coding = **batch runs, desktop-only, any agent**. The ONE Start-coding dialog (`crates/ui/src/start_coding_dialog.rs`; ONE repo per run; defaults per AGENT, not per mode) launches single-issue for 1 checked issue, BATCH for 2+: ONE session on ONE pushed branch `exp/batch-<id8>`, one prompt listing all issues, ending in ONE combined PR via `exponential_pr_open` with `issueIds` + `head` — the server links EVERY issue (same repo enforced); webhook/poller resolve a PR to ALL linked issues by exact `pr_url`, so merging completes them all (unless the team retargeted the automation). `codingSessions.start` takes exactly one of issueId/teamId; batch/action sessions steer exactly like issue sessions. EXP-626: `pr_open` also takes `repositoryId` + `head` (and `pr_merge` `repositoryId` + `prNumber`) for a PR with NO issue — nothing is linked, moved or notified, it only parks the calling session.

### Actions (EXP-253)

`actions` rows (per team, markdown `body` ≤64KB, optional `repository_id` SET NULL, optional curated `icon`, up to 10 typed inputs) — tRPC CRUD (member list/get, owner writes) + 4 MCP tools + the body-less shape. **Automations (EXP-583) are their OWN rows + shape** (`automations`: `action_id` target, `device_id` runner, nullable `agent`/`model`/`effort`, when-part `trigger` jsonb schedule|event, `enabled`) — never a field on actions; LOCAL-ONLY (no server scheduler: the bound device selects its enabled rows off Electric and self-starts via `codingSessions.start` with `startedReason`+`automationId`); owner-only `automations` router + MCP `exponential_automations_*`; an enabled automation needs every action input optional; withdrawing a device share disables its automations; all four clients have an Automations tab and seeds may carry an `automation` (×4). Runs are `coding_sessions` rows (`action_id` + `action_name` snapshot) executed LOCALLY by the desktop as interactive agent sessions (any agent, per-agent model/effort) in a per-run worktree, a PR branch's worktree (fix-conflicts), or a per-run scratch dir — never server-side secrets (they use the user's own device auth); NO per-device trust prompt. Remote start rides the steer rails: `steer.startSession({actionId, deviceId, agent?, model?, effort?})` gates on the device's registered agents; `resumeSessionId` + the `resume-run` cap relaunch an ended run. All four clients have an Actions surface (web `t/$teamSlug/agents` + desktop rail: list/run; mobile: view + run; EDITING is web/desktop-only, and since EXP-257 new actions are authored by the builtin creator run, not a manual form).

TWO virtual builtins are NOT DB rows: each client CONSTRUCTS them locally, so their name/description strings stay **byte-identical** across `lib/builtin-actions.ts` (web), `api::actions::builtin_*` (desktop) and `ActionsApi` (iOS, Android); MCP's list appends both, tRPC's does not. They pin FIRST by the `builtin` flag, every builtin start carries `teamId`, and `get/update/delete` reject the reserved ids. **"Create action"** (`builtin:create-action`) is the describe-it-and-the-agent-writes-it creator, scratch cwd. **"Fix merge conflicts"** (`builtin:fix-conflicts`) takes a required `pr` input (the representative ISSUE id of an issue-linked open PR, deduped by prUrl for batch PRs) and runs in that PR branch's WORKTREE: rebase onto origin/<default>, resolve, force-push, merge via `exponential_pr_merge`; desktop Reviews offers it on a failed merge; remote start needs only a runnable agent. Both get normal per-agent MCP wiring and prompts from shipped constants (`body` empty). Suggestion seeds (`action-suggestions.ts`, ×4) prefill the creator run; nothing is seeded as rows.

### Desktop IDE & mobile

The desktop IDE is master-only + autopull: no branch switch; changes land via PRs or Source Control's CONFIRMED commit-and-push; escape hatch = Discard-and-reset; the headless `trunk_sync` engine has a rail badge and a conflict banner (Open terminal / Abort / Discard & reset). Mobile ships full onboarding: a server-gated first-run wizard (`onboardingCompletedAt`, `lib/auth/onboarding.ts`) — create-or-join team, then a guided first board with optional repo picker + inline GitHub App connect; no account-level Integrations menu.

## Billing (per-seat, Creem — cloud only)

Subscriptions bind to a TEAM (`creem_subscriptions.team_id` + `seats`; checkout `billing.createSeatCheckout` with Creem `units` = the authoritative seat count) and belong to the TEAM, not the purchaser (REV2-55, `lib/billing/billing-handover.ts`): `reference_id` nullable/set-null — account deletion is NEVER blocked by billing (store policy) and only cancels a SOLO team's subscription it destroys; team deletes REFUSE a live subscription (`PRECONDITION_FAILED`, cancel in settings → Billing first; a period-end cancellation passes), natives point at web. ONE subscription per team: `createSeatCheckout` refuses duplicates; `billing.updateSeats`/`changePlan` mutate the EXISTING subscription with `update_behavior: proration-charge-immediately`. Free = 3 seats, 250MB storage, 1 widget; **Team** = the ONE paid tier, €15/seat/mo or €12 yearly — 10GB, unlimited widgets, helpdesk (`PlanTier = free|team|unlimited`; comp tiers `team|unlimited`). Unlimited boards/repos/coding sessions on every tier; push + steer never plan-gated; over-seat teams only block new invites.

**Billing and every limit exist only when `CLOUD_INSTANCE=true`** — self-hosted (the default) unlocks every FEATURE limit, a product switch, not a licence: Apache-2.0 (EXP-352), no licence gate in code. Enterprise Support has NO published pricing (EXP-218) — marketing routes to `/contact/`; never reintroduce price points. Self-host's one limitation: no MOBILE push (store apps compile against first-party Firebase).

## Feedback widget & helpdesk

marker.io-style widget via an async `<script>` snippet. Source `packages/widget` (Preact shadow root + `@zumer/snapdom`); the build emits an IIFE loader (command queue, floating button, config prefetch) + a lazy panel chunk into `apps/web/public/widget/v1/`. API `window.ExponentialWidget`: `init({key})`, `identify`, `setCustomData`, `open`, `close`, `submit`. Screenshots never block submission; annotatable.

Server: server-only `widget_configs` (public `expw_` key + domain allowlist) + `widget_submissions`; public CORS routes `/api/widget/config` + `/submit` (origin/rate-limit/honeypot in `lib/widget/`). **Modes** `feedback`/`support`/both (`form_config.modes`; absent = feedback-only). EXP-435: `form_config` also carries `labelIds` (≤10, served as `form.labels`; picks land in `issue_labels`) and `theme` dark/light/auto + `backgroundColor`/`textColor` (`setTheme()`, palettes in `src/theme.ts`); ONE capture button (EXP-488, engine by capability: getDisplayMedia on desktop, snapDOM on mobile or a dismissed picker) + an Off/3s/5s hold segment (FEED-18, runs post-picker via `beforeFrame`); settings preview the panel from the real `widget.css`. Feedback files an ordinary issue onto `widget_configs.board_id` (NULLABLE, required iff feedback mode; set-null so trashing the board degrades feedback, not the config); the submission creates issue + screenshot attachment (null `uploader_id`) + submission row in ONE transaction, `creator_id NULL` + `source='widget'`. Support files a STANDALONE ticket (`support_threads` + opening `support_messages` row, NO issue) gated on `teams.helpdesk_enabled` + paid plan (`assertCanUseHelpdesk`), re-checked per submit; reporter credential = emailed magic link (`lib/helpdesk/token.ts`); members use the `helpdesk` router (close/reopen, `escalate` → linked issue); notify via issue-less `support_reply` fan-out. Rate limiting = in-process token buckets (§Environment Variables); the ONLY cloud upsell is the free-tier usage bar in widget settings. Managed in team settings → "Feedback widget" (owner-only, Helpdesk toggle too). The in-app widget key is HARDCODED in `lib/runtime-config.ts` (cloud-only; `FEEDBACK_WIDGET_KEY` overrides); self-hosted renders no sidebar FeedbackButton.

## Conversion tracking (EXP-362, cloud only)

`lib/conversion/` + the `adminConversions` router + `admin/conversions.tsx`, all no-ops unless `CLOUD_INSTANCE=true` — self-hosted never profiles users. COOKIELESS: visitors are a daily-rotating salted HMAC of ip+ua (`anonymous.ts`; needs a proxy-attested `X-Forwarded-For`); attribution rides URL params only (marketing forwards `ref`/`utm_*`; `first-touch.ts` keeps them in memory). `events.ts` owns the closed vocabulary: `landing` ONLY on entry paths `/`+`/auth/*` (anonymous, non-prefetch, non-bot), `return_visit` daily per signed-in user (EXP-522), then signup → onboarding_completed → team_created → invite_sent/accepted → first_issue_created → checkout_started → the Creem lifecycle in `subscription-events.ts`; idempotency = PARTIAL UNIQUE INDEXES + unconditional `onConflictDoNothing` (re-fires are free no-ops).

## Style Conventions

- Template literals for strings (backticks, not quotes); functional components only
- shadcn/ui from `src/components/ui/` — ALWAYS over raw `<input>`/`<button>`/`<textarea>`/`<label>`; on multi-client surfaces use an icon CONCEPT, never a raw lucide import
- Business logic components in `src/components/`, not `ui/`

## Agent context budget (EXP-353/EXP-637)

Keep this file under 40k chars. MCP clients DEFER tool defs behind tool search (only names + server `instructions` load at session start; `_meta["anthropic/alwaysLoad"]` opts back in), so the budget is: the always-loaded set == `lib/mcp/always-load.ts` and <10k serialized, whole surface <60k, per-tool <1.8k, `MCP_SERVER_INSTRUCTIONS` <2k with a self-contained first 512. Gated by `lib/mcp/context-budget.test.ts`. **Compress instead of appending**: a rule over its rationale, a test or file citation over a restated list, one canonical statement over three.
