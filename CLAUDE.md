# Exponential

Real-time issue tracker.

## Tech Stack

TanStack Start (React 19, TanStack Router/React DB) · PostgreSQL 17 via Drizzle (`snake_case` casing) · ElectricSQL (shape proxy pattern, `@tanstack/electric-db-collection`) · Better Auth (email/password, email OTP, passkeys, Google/Apple, OIDC `genericOAuth`, session-based, `tanstackStartCookies`) · tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync) · shadcn/ui on Tailwind v4 (OKLCH zinc, dark forced via `html.dark`; dates via `react-day-picker` + `date-fns`) · bun. Dev infra: Docker Compose: Postgres:54321, Electric:30000, Garage:3900 (S3), Caddy:3000 (HTTP/2 proxy; `Caddyfile` gitignored, copy `Caddyfile.example`), optional steer-relay:4002 (`--profile steer`).

## Monorepo Layout

```
apps/
├── web/        # TanStack Start app (the issue tracker)
├── push-relay/ # Push notification relay (Hono/Bun)
├── steer-relay/# Remote-start + live-steer WS hub (Bun; in-memory rooms)
├── marketing/  # Vite + React; owns the Remotion ClosedLoop hero (src/movie/)
├── ios/        # SwiftUI (Tuist + GRDB; ExpCore/ExpUI)
├── android/    # Kotlin / Jetpack Compose
├── styleguide/ # Shot gallery (shots/ + @exp/view-catalog) + code-rendered Components group
└── desktop/    # Rust IDE (gpui + gpui-component + rio-vt); crates/cli = headless `exponential` daemon (EXP-403, cli-v* train)
packages/
├── db-schema/         # Drizzle schema + shared zod/domain types
├── design-tokens/     # OKLCH→sRGB + motion tokens → Compose/SwiftUI/Rust
├── domain-contract/   # contract.json — canonical enums → per-language constants
├── icons/             # icons.json — the ONE icon registry → TS/Swift/Kotlin/Rust
├── electric-protocol/ # Shape wire contract + cross-platform fixtures
├── emoji/             # emoji dataset generator → ONE json ×4
├── steer-ticket/      # HS256 ticket sign/verify (web mints, relay verifies)
├── widget/            # Feedback widget (Preact + snapDOM) → apps/web/public/widget/v1/
├── view-catalog/      # views.json — every view × platform, drift-gated
├── shots/             # capture pipeline (sharp diff-skip writer) → shots/
└── tsconfig/
docs/                  # third-party-licences.md + licences/
shots/                 # COMMITTED webp store, <view>/<platform>.webp
docker-compose.yaml    # DEV backend stack (not the self-host one)
selfhost/              # Pull-an-image compose; INSTALL.md = agent-followable runbook
Dockerfile{,.push-relay,.steer-relay}   # build context = repo root
```

Workspace names: `@exp/<dir>`; `apps/desktop` is a Cargo workspace, not a bun one. The only movie is the ClosedLoop hero in `apps/marketing/src/movie/` (`@remotion/player` lazy-chunked; `LoopMovie.tsx` remotion-free; `scripts/prerender.tsx` renders under Bun).

**Dead, never reintroduce:** releases + footage/fixtures (EXP-106), `@exp/video`, workspace/project vocabulary + `/w/`/`/projects/` URLs (EXP-180), board types, `agent_runs` + agent-core + the companion daemon + the synthetic `isAgent` identity, the `assigned-issues` shape, `run_configs`, the one-shot `claude_task` primitive (EXP-259), `isProtected` boards + dogfood cases (EXP-364), due-date time-of-day (REV2-49), `SELF_HOSTED` (now `CLOUD_INSTANCE`), `GOOGLE_CALENDAR_ENABLED`/`DOGFOOD_REPO`, the builtin `todo` status (EXP-685), the skip-permissions setting (EXP-690), PTY/terminal-mode coding + `start_in_terminal` (EXP-773), the pi agent (EXP-849; `ExternalAgent` is the escape hatch).

## Product Invariants

**Vocabulary (EXP-180):** the product says **team** and **board** EVERYWHERE: copy, URLs (`/t/$teamSlug/boards/$boardSlug/issues/$id`), identifiers, DB, routers, shapes, MCP tools. Boards have no types; `repository_id` is NULLABLE (FK restrict); coding gates on repo PRESENCE.

**Client parity:** all four clients sync the same 21 Electric shapes (`routes/api/shapes/` IS the list). `pins` (EXP-778) is per-user static, never trash-scoped: a row renders only when its target resolves. The `actions` shape EXCLUDES the `body` (tRPC `actions.get`). `devices` + `device_worktrees` (EXP-481) sync `user_id = me OR shared_team_ids && (member teams)` (FEED-33: uuid[], per-team toggles) via trigger mirrors; identity rotates only on membership changes; devices rows are SERVER-AUTHORITATIVE (persisted `launch_defaults` the machine converges to; heartbeat ~30s; online = `last_seen_at` within `contract.device.onlineWindowSeconds`). `repositories`, `user_notification_prefs`, `email_deliveries`, `conversion_events`, `device_commands`, `passkeys`, helpdesk and widget tables are **server-only (tRPC), never synced**.

**Nothing is anonymously readable:** every shape is member-only (anonymous → impossible-match sentinel); no public tRPC; attachment reads need membership. The ONLY anonymous endpoints: widget (`/api/widget/*`), helpdesk reporter magic-link (`/api/support/*` + `/support/$token`), invites, auth, the licence notices `/about` + `/NOTICES.txt`, plus the MCP OAuth CIMD + state-gated callback (`/api/mcp-oauth/{client.json,callback}`, EXP-792). Board-scoped shapes are TEAM-scoped with a STATIC trash predicate (REV2-5): `team_id IN (member teams) AND board_deleted_at IS NULL`, a trigger-maintained mirror of the board's `deleted_at` on every issue child, so trashing moves rows out incrementally and **shape identities rotate ONLY on membership changes**. Batch `coding_sessions` rows and issue-less notifications keep NULL `board_deleted_at` and always sync; the notifications shape is static per user (`user_id = me AND board_deleted_at IS NULL`; fan-out filters recipients at delivery, delivered rows outlive membership); issue-less `support_reply` rows carry a synced `team_id` for Support routing.

**Permissions are membership-only:** `lib/auth/access.ts` `resolveTeamAccess` is the single authority (capabilities `read`/`comment`/`create_issue`/`mutate_resources`), mirrored by `use-team-permissions.ts` + the natives; every member moderates and handles support, owners own the destructive/settings surface (owner-only controls HIDDEN for non-owners; destructive native actions confirm). Signups get **no** team: first-run = "Create a team" (`teams.create`, any authed user; creator = owner; cloud free tier capped by `FREE_OWNED_TEAMS_CAP`) or "Join a team" (invite link; `teamInvites.accept` stamps `onboardingCompletedAt`; invites may carry a synced `email` the server mails). `teams.getDefault` = the NON-CREATING default-team resolver (oldest membership or null). An owner may delete ANY team incl. the last (cloud: once its subscription is cancelled). **Billing and the admin console are web-only. Coding sessions run only on the desktop app and the headless `exponential` CLI daemon** (EXP-403; device-code login at `/auth/device`), both publishing to the steer relay (scrubbed activity only); a LIVE session is visible/steerable ONLY by its owner (EXP-312); teammates see the synced status badge.

The app is **noindex** everywhere (`__root.tsx` meta + `X-Robots-Tag` from `server-bun.ts`; `/robots.txt` disallows only `/api/`); marketing owns the indexed surface (`PAGES` in `src/lib/seo.ts` → `sitemap.xml`/`llms.txt`).

## Shared Contracts

**Icons (EXP-273/317):** ONE icon set, Lucide, byte-identical on all four clients, generated from `packages/icons/icons.json` into COMMITTED per-platform outputs. Change an icon by editing `icons.json` + `bun run --filter @exp/icons generate`; never hand-edit generated files nor add per-platform glyph maps. `pickable` (the 60 picker names, byte-equal to contract `boardIcon.values`) is **APPEND-ONLY**: reordering orphans stored rows. Multi-client surfaces name a CONCEPT (`conceptIcon(\`nav-search\`)` / `registry::NAV_SEARCH` / `AppIcons.navSearch` / `ExpIcons.navSearch`), never a raw glyph; iOS renders via `AppIcon`, NOT `Image(systemName:)`; `icons.test.ts` gates drift + the concept rules. Desktop assets also hold hand-maintained BRAND marks (`claude`/`codex`/`logo*`/`apple`/`google`).

**Enums:** canonical values live in `packages/domain-contract/contract.json` (support-thread `status`/`direction`/`visibility` are documented varchars in `domain.ts`); changing `db-schema/src/domain.ts` means updating `contract.json` + regenerating.

**Markdown:** `issues.description` + `comments.body` are plain `text` GFM — one interchange: web TipTap + tiptap-markdown, iOS cmark-gfm, desktop comrak + vendored WYSIWYG `crates/gpui-markdown-editor` (Velotype, see NOTICE), Android `ui/markdown/` commonmark-java. The round-trippable feature set IS `CONTRACT_FIXTURES` (byte-locked by desktop `wysiwyg_parity.rs` + native mirrors). **Underline unsupported**, slash commands out of scope. **Tables** (EXP-726): canonical `| a | b |`/`| --- |` rows, `:---` alignment, one inline paragraph per cell, `\|` escape; top-level only, natives HOIST nested ones out (EXP-728). **Mentions** are plain `@<email>` (`lib/integrations/mentions.ts` resolves members, fires `issue_mention`, auto-subscribes); clients render member pills, @-autocomplete inserts the plain form. **Issue mentions** are plain `#<IDENTIFIER>` (token contract `lib/issue-refs.ts`, per-client lookup, `#` autocomplete ×4); a pill renders only for a synced same-team issue; a ref auto-links both as `related` (source `reference`, delta-removed; no notifications). Images are stored relative `![alt](/api/attachments/{id})` (server canonicalizes, clients absolutize; attachments carry probed `width`/`height`). **Inline media (EXP-824):** `video/*`/`audio/*` rows embed as a PLAIN LINK alone in a paragraph, `[clip.mp4](/api/attachments/{id})` (never the image form; `?w=` allowed), upgraded to a player from the synced row (`duration_ms`, `poster_storage_key` → `?poster=1`); a link in running text stays a link. Normalisation is CLIENT-side (H.264+AAC MP4, mobile 720p); the server only probes MP4/MOV headers (`lib/storage/video-metadata.ts`); the poster is a second blob on the row. **Emoji** (EXP-551) are inserted as unicode, never `:shortcode:`; picker + `:` typeahead data generated ONCE by `packages/emoji` (drift-gated).

## Commands

From repo root unless noted.

```bash
bun install
bun run backend                    # docker compose up -d + dev server (:3000 via Caddy)
bun run ios                        # tuist generate + Xcode (Mac-only)
bun run ios:test                   # ExpCore+ExpUI suites (Mac-only)
bun run android                    # productionDebug install + launch
bun dev                            # web dev server (localhost:5173)
bun run dev:marketing / build:marketing
bun run movie:{studio,render,poster,still}
bun run {dev,start}:push-relay     # localhost:4001
bun run {dev,start,test}:steer-relay   # localhost:4002
bun run build                      # widget + web + marketing (widget FIRST)
bun run build:web / build:widget / test:widget
bun run dev:widget                 # watch-build (/widget/v1/demo.html)
bun run typecheck / test / test:e2e   # web app
bun run migrate / migrate:generate / psql
bun run backend:{up,down,clear}    # clear wipes volumes
bun run storage:init               # one-time Garage bootstrap
bun run dev:desktop                # gpui IDE against the local backend
bun run {build,appimage,macapp,test}:desktop
bun run --filter @exp/{domain-contract,design-tokens,icons} generate
cd apps/web && bun run seed:screenshots        # demo data (shots + store captures)
bun run shots                                  # all-platform view captures → shots/
```

Workspace scripts: `bun --filter @exp/web <script>`; plain `cargo` from `apps/desktop/`. Never `bun run lint` (--fix corrupts `typeof import()` sites) nor `bun run format`.

## Deploys

Everything runs on Coolify (`coolify.home.straehhuber.com`, Hetzner), **home-LAN-only, no redeploy webhooks**: after a green Actions run, `coolify deploy uuid <uuid>` from the LAN. `build-web.yml` publishes `ghcr.io/niach/exponential-web` on master pushes + `v*` tags, multi-arch; the SAME image is cloud, staging and self-host (`selfhost/`), so the ghcr package stays PUBLIC and self-hosters pin semver. Its runtime `bun install` is `--filter '@exp/web'` on purpose (EXP-380); non-OSS components + notices rules: `docs/third-party-licences.md`, gated by `lib/third-party-licences.test.ts`. Native releases are tag-triggered (`build-{android,desktop,cli,ios}.yml`): `android-v*` (APK + Play bundle, `make_latest: false`), `desktop-v*` (codegen-drift guard, production + staging × macOS/Linux/Windows, `make_latest: true`, self-update `crates/updater`), `cli-v*` (bare `exponential-<target>` binaries, `apps/marketing/public/install.sh`, cloud AND self-host via `EXP_INSTANCE`), `ios-v*` (ASC upload from a release-macOS runner, `ASC_*` secrets).

**The operations runbook lives OUTSIDE the repo** (infra uuids/domains, buckets, staging, signing, release checklist); consult it before anything deploy-shaped.

Every user-facing release PREPENDS a `ChangelogEntry` to `lib/changelog.ts` (gated by `changelog.test.ts`; its head id drives "What's new" on web AND `crates/ui/src/changelog.rs`).

After schema changes, always: `bun run migrate:generate && bun run migrate`. Custom SQL triggers (`db/out/custom/0001_triggers.sql`) auto-apply at boot (`bootstrap-cloud.ts` `applyCustomSql`, idempotent); only never-booting contexts (CI's schema job) need psql.

## Web App Structure (`apps/web/src/`)

Shadcn lives in `components/ui/`, feature components flat in `components/` (`agent-session.tsx` = the steer/activity view). `lib/trpc/` is one file per router; `routes/api/trpc/$.ts` lists them. `lib/auth/`: `membership.ts` = data lookups, `access.ts` = authorization (`resolveTeamAccess`). `lib/notification-email-policy.ts`/`-digest.ts`: push fires on create, email is a DIGEST of still-unread (DAILY at a user-chosen local hour, hourly legacy opt-in, atomic `emailed_at` claim; `server-bun.ts` schedules). EXP-801: MCP `exponential_notifications_send` = issue-less team-scoped `agent_message` row + push to members/self (one inbox row each ×4); prefs `allow_agent_messages=false` BLOCKS other members' agents (own always pass). Team routes under `t/$teamSlug/` (`ls` lists them): inbox `?tab=my-issues` is a TAB, not a route; `reviews/$issueIdentifier` = the cross-board open-PR queue with confirmed squash merge; `agent` = sessions list + the composer (every play button routes here with `?issues=|action=|pr=|device=|text=|icon=`, one-shot; `?from=` = the origin the session's Back returns to), `sessions/$sessionId` steers inside it (EXP-818). Also `auth/consent.tsx`, `invite/$token`. Entry: `router.tsx`, `start.tsx` (`defaultSsr: false`), `server{,-bun}.ts`.

## Database

`@exp/db-schema` is authoritative — never mirror it here.

### Conventions

Better Auth user IDs are `text`, so all user FKs are `text`; app tables use UUID PKs (`gen_random_uuid()`). All tables carry timezone `created_at`/`updated_at`; sort orders are `doublePrecision` (fractional indexing); rich text is `text` GFM. Due date is `date` only.

### Non-obvious fields

Issues DUAL-WRITE `status` (the builtin ANCHOR enum) and `statusId` (nullable FK `issue_statuses` SET NULL, the precise per-team row); `creatorId` is NULLABLE (widget issues have none), `source` is `user`/`widget`; comments thread ONE level (`parent_id`, replies re-parent to the root) + `source` user|mcp stamped from `ctx.viaMcp` ("via MCP" ×4, EXP-741); `duplicateOfId` pairs with status `duplicate` and dual-writes an `issue_relations` `duplicate` row (EXP-736: canonical-direction rows `blocks`/`parent`/`duplicate`/`related`, `source` user|reference, member-managed via `relations` + 2 MCP tools, events on both sides); the PR fields mean ONE PR per issue on `exp/<IDENTIFIER>` (batch issues share ONE `prUrl`). `coding_sessions` is issue XOR batch XOR action-scoped (action rows carry `action_id` [set null] + an `action_name` snapshot). Teams carry a server-only `compTier` plus synced `helpdeskEnabled` and PR automation (`prOpened*`/`prMerged*` `StatusId` + `Automation`: nullable FKs SET NULL, NULL = builtin default target, `*Automation=false` = do nothing; member-gated `statuses.setPrAutomation`; UI web + desktop).

### Enum behavior

Values in `contract.json` (§Shared Contracts). `issue_status`: `pr_open` flips linked issues to the team's PR-open target (default `in_review`), merge to the PR-merge target (default `done`). `coding_session_status` (running/in_review/ended): `in_review` = PR open; PR MERGE **ends** live sessions on EVERY path (EXP-498) unless the team's synced `endSessionsOnMerge` is false or MCP `pr_merge({endSessions})` overrides it (EXP-711), never the session that merged its OWN PR (server-only `merged_own_pr`, EXP-637). Orphan PG labels: `merged` (EXP-540), `todo` (EXP-685); `ended` also via `killSession`/`codingSessions.end`. `ended_by` (agent|user|client|merge|system) records the path; `summary` is written ONLY by `exponential_sessions_end`, REGISTERED only for UNATTENDED runs (`started_reason` schedule|event|`agent` = a `sessions_start` child; synced `parent_session_id` nests it, `session-tree` ×4; EXP-679/818) and ENDING them (EXP-673); a person-started run has NO idle bound (EXP-674) except a queued daemon update (FEED-36: sessions idle ≥2h end for it; `update_now`/cap `update-now` ends them now); `needs_input` and `blocked` (EXP-804 jsonb, device-written, orthogonal to status: a walled run stays `running`; set ONLY by a refusal (`rejected`/a notice, never `allowed_warning`), `window` from claude's `rateLimitType`, FEED-34/35) land on every live status; `agent_busy` (device-written per turn edge, EXP-848) is the ONLY list-spinner input ×4; Automations lists finished AUTOMATED runs, the Agent page's Running/Past the person-started ones; `resumed_from_id` links a resumed run to its predecessor. Batch sessions (issue_id NULL) self-close on the desktop once their branch's issues sync `prState=merged`.

### Custom triggers

`apps/web/src/db/out/custom/0001_triggers.sql` holds 17 commented functions; read it before anything trigger-adjacent. They guarantee: per-board issue numbers + `{prefix}-{number}` identifiers; `updated_at` maintenance (comments bump the issue, the trash fan-out doesn't); denormalized `team_id`/`board_id` + both board-hide mirrors on every issue child and on notifications (no-op when `issue_id` is NULL, so batch rows carry explicit ids); trash/archive fan-outs to those mirrors; the 6 builtin `issue_statuses` per team (`issueStatusDefaults`); `status_id` from the anchor for enum-only writers (explicit dual-writes win); `user_id`/`shared_team_ids` mirrors on `device_worktrees` with share-change fan-out (EXP-481) + team-delete unshare (FEED-33); the `team_ids` membership mirror on `users` (+ boot heal) behind the users-shape where clause (REV-37); `creem_subscription_id` immutable once set (REV-12).

## Patterns

### Electric shape proxies

One proxy per synced table in `routes/api/shapes/`, built with `createShapeRouteHandler`; all member-only. **Every proxy pins a server-side `columns` allowlist clients cannot widen; a new server-only column on a synced table goes BEHIND it**. `users` pins exactly `id,name,email,image,created_at,updated_at`; `teams` its contract list (`comp_tier` never); `issue-subscribers` drops reporter `email`; `actions` drops `body`; board-scoped shapes drop their scoping columns. A shape may FILTER on a column its allowlist excludes — Electric evaluates `where` server-side. Hardening: `cache-control: private, no-store` + `vary: authorization, x-api-key, cookie`; failing token credentials get a 401, never the anonymous clause (cookie-only still falls back anonymously); `buildWhereClause` SORTS id lists (= the shape identity); membership ids stay OUT of board-scoped clauses (`buildTeamScopedChildWhere`). A FIFO semaphore in `electric-proxy.ts` bounds ALL snapshot-class forwarding (any request without `live=true`, REV-27); live long-polls never gated. Wire format, control messages, long-poll timeout floors: `packages/electric-protocol/README.md`.

### Board trash (48h soft delete) + archive (EXP-500)

Owner-only `boards.delete` stamps `deleted_at`; **archive is the same machinery minus the purge** (`archived_at` + a `board_archived_at` child mirror + its own fan-out). Both vanish server-side, `boardVisible()` (`lib/board-visibility.ts`, the ONE predicate every boards join takes) + the shapes' static `IS NULL` suffixes + trigger-fanned mirrors, never client-side (REV2-103): `archived_at` is in no allowlist. Both keep the `(team_id, slug)` reservation; `restore`/`listDeleted` power the web-only "Pending deletion" card, `archive`/`unarchive`/`listArchived` "Archived boards". The purge sweep (`lib/board-trash.ts`) keys on `deleted_at` alone: hard-delete after 48h + S3 reclaim, like team/admin deletes (`collectAttachmentStorageKeys` in-tx, delete after commit).

### Custom issue statuses (EXP-314)

Per-TEAM rows in six fixed categories (backlog/unstarted/started/completed/cancelled/duplicate; contract: values + ONE `displayOrder` (EXP-448) + `startedMax: 4`). 6 LOCKED builtins per team (`builtin_key` = legacy enum values, never renamed/recolored/deleted, movable within category; seeded by trigger + migration; defaults ONCE in `issueStatusDefaults`; `unstarted` EMPTY since EXP-685). Customs are name+color, member-managed (`statuses` router; started ≤4; delete requires reassign; web+desktop manage, mobile renders/picks). `issues.status` STAYS the dual-written **anchor** (`CATEGORY_ANCHOR` in `db-schema/src/domain.ts`; unstarted customs anchor to `backlog`) so enum-keyed subsystems keep working; PR open/merge TARGETS are per-team configurable. `statusId` carries the precise row; `populate_issue_status_id` re-anchors enum-only writers. Resolution, colors and pie-clock glyphs derive in `lib/team-statuses.ts` + `lib/status-icons.ts` (hand-mirrored ×4, lock-tested). Lists group by status ROW; filters are group-key sets (web accepts legacy enum URL tokens). Duplicate: no customs, out of pickers, enum+`duplicateOfId` lockstep.

### Web plumbing

- **Collections** (`lib/collections.ts`): all use `columnMapper: snakeCamelMapper()` — without it `useLiveQuery` `where` silently fails. `undefined` (not `false`) skips a query; `and()`/`or()` from `@tanstack/react-db`, never `&&`/`||`.
- **Auth guard**: `_authenticated.tsx` `beforeLoad` + `throw redirect()`; `fetchSessionOnce()`.
- **MCP OAuth consent**: `lib/auth/mcp-authorize-guard.ts` pre-flights every `mcp/authorize` (forces `prompt=consent`) → `/auth/consent` team/board multi-select persisted to `mcp_grants` BEFORE the code mints. `lib/mcp/scope.ts` confines OAuth tokens to the grant (no grant row = nothing) and to `/api/mcp`; cookies + `expu_` keys keep full access. Login resumes interrupted authorizes (`lib/auth/oauth-resume.ts`).
- **Issue UI**: issue detail is a route fed a live Electric `issue` (md+: the board list beside it); title/description save on blur, other fields mutate immediately; `completedAt` is auto-managed. Filters: `lib/filters.ts`.

## Environment Variables

**`.env.example` at the repo root is the CANONICAL reference** and `selfhost/.env.example` its self-host subset; read them, no list here; each relay has its own `apps/*/.env.example`. Not obvious from those files:

- `CLOUD_INSTANCE` is the opt-IN cloud marker (EXP-364): `'true'` turns on billing, plan limits, the in-app widget and conversion tracking; unset = self-hosted, every FEATURE limit unlocked.
- `AUTH_PASSWORD_ENABLED`/`AUTH_SIGNUP_ENABLED`: password login defaults true, public signup on in dev, OFF in production builds (`selfhost/docker-compose.yaml` re-defaults it `true`). Auth posture is BUILD-derived (`lib/production-build.ts` `isProductionBuild`, REV-5), never runtime `NODE_ENV`. EXP-857: `AUTH_EMAIL_OTP_ENABLED` defaults on WITH a mail transport, `AUTH_PASSKEY_ENABLED` WITH an https base (rpID = host; Android origins from `ANDROID_APP_LINK_FINGERPRINTS`); login = ONE "Continue with …" list ×4; `mobile-oauth-start?provider=browser` = the native browser handoff.
- Mail: SES (`AWS_SES_REGION` + creds) OR `SMTP_*` for ALL mail, SES wins if both; neither = email off.
- OIDC: `OIDC_PROVIDERS` (JSON array) is primary; the single-provider `AUTH_OIDC_ENABLED`/`OIDC_*` vars are legacy, read only when it is unset.
- GitHub App installations are claimed PER TEAM (`github_installation_links`); `GITHUB_APP_CLIENT_SECRET` unset ⇒ install-page round-trip; `GITHUB_POLLING=true` = outbound merge cron for NAT'd self-hosts.
- `STEER_RELAY_URL` unset = remote start/steer off; HS256 `STEER_RELAY_SECRET` must match the relay; BOTH relays need `TRUST_PROXY=true` behind a reverse proxy.
- `INITIAL_ADMIN_EMAILS` auto-promotes global admins.
- `CLIENT_MIN_VERSION_{ANDROID,IOS,DESKTOP}` gate with HTTP 426 + a blocking update screen (unset = off); MARKETING versions, never build numbers; `CLIENT_LATEST_VERSION_*` = informational.
- `BUN_CONFIG_MAX_HTTP_REQUESTS` is baked to 65336 in the image (REV2-6).
- Widget rate limits: refill `WIDGET_RATE_LIMIT_PER_{KEY,IP}_HOURLY`, burst `WIDGET_RATE_LIMIT_{KEY,IP}_BURST`; KEY self-host-only (cloud: per-TEAM plan ceiling, `lib/widget/submit-limit.ts`), IP everywhere, `RECIPIENT` bounds support mail per address; config GET: `WIDGET_CONFIG_RATE_LIMIT_{PER_IP_HOURLY,IP_BURST}` (REV-25).

## Coding sessions & Actions

### The launcher

A thin launcher (`coding::prepare`, desktop + CLI): the issue's repo from the team registry (tRPC) → a session-gated JIT GitHub-App installation token → a worktree + `exp/<IDENTIFIER>` branch with ambient git auth from a repo-local credential helper (EXP-73, refreshed on the real `expires_at`) → the `/api/mcp` MCP config with the user's personal `expu_` apikey, NOT `.mcp.json` → the playbook `crates/coding/src/skill.md` appended to the system prompt on EVERY start/resume (claude `--append-system-prompt`, codex `developer_instructions`; gated by `context-budget.test.ts`, EXP-763) → plan-first. The agent commits, pushes, opens its PR via MCP `open_pr`, then, if unattended, reports via `exponential_sessions_end` (the `X-Exp-Session-Id` MCP header names the run). Action and chat runs get their OWN worktree + branch (`exp/<slug>-<id8>` / `exp/chat-<id8>`); a repo-less chat runs in a scratch dir (EXP-739); agents never write to the trunk; clean run worktrees go at end. Local deps: `git` + agent CLIs, never `gh`. Default branches resolve live (healed on `repositories.list`/`installationToken`), never assume `main`: `boards.default_branch` → team `default_branch_override` → GitHub.

### Agents & the engine

EXP-201/EXP-746; `coding/src/agent.rs`, contract `codingAgent`. ONE engine (`crates/engine`) runs every session as in-process ACP 2.x over the user's UNMODIFIED CLI: `adapters/` = claude stream-json + control protocol, codex app-server, `ExternalAgent` any ACP stdio binary (opt-in, local only); ONE `SessionUpdate`→`ActivityEvent` mapper feeds relay + UI; never spoof `CLAUDE_CODE_ENTRYPOINT`; logins + agent shell tabs are the ONLY agent PTYs (EXP-773). Claude ALWAYS gets `--allow-dangerously-skip-permissions`; the adapter auto-allows every `can_use_tool` except `ExitPlanMode`/`AskUserQuestion`, plan mode included; codex full-access; ultracode claude-only; plan mode claude+codex. `runs.json` records the ACP id + the agent's NATIVE id (claude's `--session-id`, re-read on `/clear`, EXP-784; 10-day TTL); a resume re-enters the RECORDED run (EXP-662); a repo-less run is purged WHOLE at end, unresumable (EXP-764); `.exp-agents` gates worktree reuse. The doctor gates the SELECTED agent (git always) + ACP readiness (`acp` cap; below the floor = REFUSED); `devices.register` records runnable agents (remote pickers offer only those; absent = claude). A terminal is FULL-WIDTH (EXP-791); a session sits beside the list it came from (`navigation::derive_origin`; Agent rail = `ToolWindow::Sessions` + `Screen::Chat`, EXP-818); the bottom bar lists terminals. Steering: NO mid-session control (EXP-790: plan/model/effort are launch-time; `config_state.options` ALWAYS empty); `config_state`/`usage`/`rate_limit` (EXP-784) are latest-wins STATE (one slot, never feed rows), like `compaction`; `tool` carries `id`+contract `toolKind`, `tool_update` patches that row (failed, edit diff ≤ `steerFeed` caps), a collapsed run's caption is contract `toolGroupSummary` ×4 (EXP-785/786); contract `expToolDisplay` labels Exponential MCP calls + a `preview` off their result (EXP-846); `turn` (started|ended) is the latest-wins busy slot driving the spinner + synced `agent_busy` (EXP-848); a pending question/plan is answered IN its card (free-text/reject rows open an inline field, the composer hides, answered ask steps reopen until submit, EXP-820); `/` = contract `steerCommands` ∪ agent commands. Narration carries `messageId` (same-id fragments merge) and, with user rows, `subagentId` (subagent view only ×4; a subagent's first prompt is never emitted). Transcripts persist ONLY on the device (`{data_dir}/journal/<id>.jsonl`, `steer::history`, 60-day prune, EXP-773): viewer tickets carry `deviceId`; a join with no live room sends `history_request` down the device's control socket (EXP-796); the device republishes the journal into a room that LINGERS after `bye {outcome:history}` → `activity_synced`; `device_offline`/`history_unavailable` are terminal. `subagent.title` = the spawning call's description (EXP-847); a READ-ONLY Plan chip mirrors `config_state.currentMode`; the ending verb is Stop ×4; an unknown agent id renders concept `settings-agents`; pins come from a header/`…`, never a list row. **Accounts (EXP-849):** per-account config dirs (`agent_profiles`: `CLAUDE_CONFIG_DIR`/`CODEX_HOME`); credentials are NEVER copied, synced or brokered (rotating refresh families; policy), transcripts may be. `devices.agent_accounts`/`agent_usage` jsonb ride the heartbeat READ-ONLY; `health` (ok|needs_relogin|signed_out|unknown) comes from the usage probe's 401, never `auth status`; codex keep-alive = `account/read` refreshToken every 6h for logins this machine RUNS; claude refresh deferred (EXP-852). Accounts page = decision surface (per-agent tabs, usage, health); Devices = repair surface (`agent_login` signs in, `agent_profile_use` activates a signed-in profile; never `codex logout` on the ambient login, it revokes server-wide). Remote start/resume carry `account`; a mid-run switch = claude-only `startSession({resumeSessionId, account})` on an idle turn (transcript JSONL copied into the profile, `resumed_from_id` = continuation). A poll pins past a reset only when EVERY window is maxed (EXP-817); Merge sits in the Changes bar (diff vs merge-base of `origin/<default>`). EXP-792 symmetric strict: only Exponential-managed MCP servers connect on every agent (claude `--strict-mcp-config`, codex the WHOLE `mcp_servers` table); user servers are server-only `mcp_servers` rows the launcher injects with device-held secrets (`${VAR}` env refs).

### Batch runs

Multi-issue coding = **batch runs, desktop-only, any agent**. The Agent page composer is the ONE launcher ×4 (EXP-825; `components/launch-composer.tsx` / `chat_screen.rs` / `AgentPage`: issue chips OR one action chip; free text = `prompt`; ONE repo per run; defaults per AGENT): 1 picked issue = single-issue, 2+ = BATCH: ONE session on ONE pushed branch `exp/batch-<id8>`, one prompt listing all issues, ONE combined PR via `exponential_pr_open` with `issueIds` + `head` (the server links EVERY issue, same repo enforced); webhook/poller resolve a PR to ALL linked issues by exact `pr_url`. `codingSessions.start` takes exactly one of issueId/teamId; batch/action sessions steer like issue ones. EXP-626: `pr_open` also takes `repositoryId` + `head` (`pr_merge` `repositoryId` + `prNumber`) for a PR with NO issue: nothing is linked or notified, it lands on the CALLER's synced row (`coding_sessions.pr_*`, EXP-734), merges from the run (`codingSessions.mergePr`), lists under Reviews → Agent runs; `applySessionPrState` tracks every merge path.

### Actions (EXP-253)

`actions` rows (per team, markdown `body` ≤64KB, optional `repository_id` SET NULL + curated `icon`, ≤10 typed PICK inputs `repo|board|pr|icon`; EXP-825: free text = `steer.startSession({prompt})` → an "Additional instructions" section hinted by `actions.prompt_placeholder`, images = steer embeds via `POST /api/teams/$teamId/session-files` bound by `codingSessions.start({attachmentIds})`, cap `start-prompt` gates the Chat/Create builtins): tRPC CRUD (member list/get, owner writes) + 4 MCP tools + the body-less shape. **Automations (EXP-583) are their OWN rows + shape** (`automations`: `action_id` target, `device_id` runner, nullable `agent`/`model`/`effort`, `trigger` jsonb schedule|event, `enabled`), never a field on actions; LOCAL-ONLY (no server scheduler: the bound device picks its enabled rows off Electric and self-starts with `startedReason`+`automationId`); owner-only `automations` router + MCP `exponential_automations_*`; an enabled automation needs every input optional; withdrawing a device share disables its automations; Automations tab ×4. Runs are `coding_sessions` rows (`action_id` + `action_name` snapshot) executed LOCALLY (any agent) in a per-run worktree, a PR branch's worktree (fix-conflicts) or a scratch dir; never server-side secrets; NO per-device trust prompt. Remote start rides the steer rails (`steer.startSession({actionId, deviceId, agent?, model?, effort?})`); `resumeSessionId` + the `resume-run` cap relaunch an ended run. All four clients edit actions in full (EXP-694; mobile fetches `body` via `actions.get`); the builtin creator run authors new ones, not a form (EXP-257).

TWO virtual builtins are NOT DB rows: each client CONSTRUCTS them locally with **byte-identical** name/description strings (`lib/builtin-actions.ts`, `api::actions::builtin_*`, `ActionsApi` ×2); MCP's list appends both, tRPC's does not. They pin FIRST by the `builtin` flag, every builtin start carries `teamId`, and `get/update/delete` reject the reserved ids. **"Create action"** (`builtin:create-action`, inputs `[repo?, icon?]`, the request = `prompt`) is the describe-it creator run, scratch cwd; the hidden Chat builtin (`[repo?]`, `prompt` required) is the composer with no subject. **"Fix merge conflicts"** (`builtin:fix-conflicts`) takes a required `pr` input (the representative ISSUE id of an issue-linked open PR, deduped by prUrl for batch PRs) and runs in that PR branch's WORKTREE (rebase, resolve, force-push, `exponential_pr_merge`); Reviews offers it on a failed merge. Both get per-agent MCP wiring; prompts are shipped constants (`body` empty). Suggestion seeds (`action-suggestions.ts`, ×4) prefill the creator run's `text`, never rows.

### Desktop IDE & mobile

Desktop IDE = master-only + autopull (no branch switch; changes land via PRs or Source Control's CONFIRMED commit-and-push; escape hatch Discard-and-reset; `trunk_sync` badge + conflict banner). Mobile first-run wizard (`lib/auth/onboarding.ts`, server-gated): create-or-join team, then a first board with optional repo + GitHub App; no account-level Integrations menu. Lists ×4 = a filled group band over FLAT rows (EXP-818); cards only in settings sections.

## Billing (per-seat, Creem — cloud only)

Subscriptions bind to a TEAM (`creem_subscriptions.team_id` + `seats`; checkout `billing.createSeatCheckout`, Creem `units` = the seat count) and belong to the TEAM, not the purchaser (REV2-55, `lib/billing/billing-handover.ts`): `reference_id` nullable/set-null; account deletion is NEVER blocked by billing and only cancels a SOLO team's subscription it destroys; team deletes REFUSE a live subscription (`PRECONDITION_FAILED`; a period-end cancellation passes), natives point at web. ONE subscription per team: `createSeatCheckout` refuses duplicates; `billing.updateSeats`/`changePlan` mutate the EXISTING subscription with `update_behavior: proration-charge-immediately`. Free = 3 seats, 250MB storage, 1 widget; **Team** = the ONE paid tier, €15/seat/mo or €12 yearly: 10GB, unlimited widgets, helpdesk (`PlanTier = free|team|unlimited`; comp tiers `team|unlimited`). Unlimited boards/repos/coding sessions on every tier; push + steer never plan-gated; over-seat teams only block invites.

**Limits exist only when `CLOUD_INSTANCE=true`**, a product switch, not a licence: Apache-2.0 (EXP-352), no licence gate in code. Enterprise Support has NO published pricing (EXP-218): marketing routes to `/contact/`. Self-host's one limit: no MOBILE push (store apps embed Firebase).

## Feedback widget & helpdesk

Async `<script>` snippet; `packages/widget` (Preact shadow root + `@zumer/snapdom`) builds an IIFE loader + lazy panel chunk into `apps/web/public/widget/v1/`. API `window.ExponentialWidget`: `init({key})`, `identify`, `setCustomData`, `open`, `close`, `submit`.

Server-only `widget_configs` (public `expw_` key + domain allowlist) + `widget_submissions`; public CORS routes `/api/widget/config` + `/submit` (origin/rate-limit/honeypot in `lib/widget/`). **Modes** `feedback`/`support`/both (`form_config.modes`; absent = feedback-only). `form_config` also carries `labelIds` (≤10 → `issue_labels`) and `theme` dark/light/auto + colors (`setTheme()`); ONE capture button (EXP-488: getDisplayMedia on desktop, snapDOM on mobile) + an Off/3s/5s hold segment (FEED-18); settings preview the real `widget.css`. Feedback files an ordinary issue onto `widget_configs.board_id` (NULLABLE, required iff feedback mode; set-null): issue + screenshot attachment (null `uploader_id`) + submission row in ONE transaction, `creator_id NULL`, `source='widget'`. Support files a STANDALONE ticket (`support_threads` + opening `support_messages` row, NO issue) gated on `teams.helpdesk_enabled` + paid plan (`assertCanUseHelpdesk`), re-checked per submit; reporter credential = emailed magic link (`lib/helpdesk/token.ts`); members use the `helpdesk` router (close/reopen, `escalate` → linked issue); notify via issue-less `support_reply` fan-out. Rate limiting = in-process token buckets (§Environment Variables); the ONLY cloud upsell is widget settings' free-tier usage bar. Settings entries "Feedback widget" + "Helpdesk" (owner-only) on web AND the IDE (read-only + Manage on the web). The in-app widget is HEADLESS behind the sidebar "Report bug" button; key in `lib/runtime-config.ts` (cloud-only; `FEEDBACK_WIDGET_KEY` overrides); self-hosted shows no button.

## Conversion tracking (EXP-362, cloud only)

`lib/conversion/` + `adminConversions` router + `admin/conversions.tsx`, no-ops unless `CLOUD_INSTANCE=true`. COOKIELESS: visitors = daily-rotating salted HMAC of ip+ua (`anonymous.ts`, needs a proxy-attested `X-Forwarded-For`); attribution = URL params only (`ref`/`utm_*`, `first-touch.ts`). `events.ts` owns the closed vocabulary: `landing` ONLY on entry paths `/`+`/auth/*` (anonymous, non-prefetch, non-bot), `return_visit` daily per signed-in user (EXP-522), then the signup→checkout funnel + Creem lifecycle (`subscription-events.ts`); idempotency = PARTIAL UNIQUE INDEXES + `onConflictDoNothing`.

## Style Conventions

- Template literals for strings; functional components only
- shadcn/ui from `src/components/ui/` — ALWAYS over raw `<input>`/`<button>`/`<textarea>`/`<label>`; multi-client surfaces use an icon CONCEPT, never a raw lucide import
- Business logic components in `src/components/`, not `ui/`

## Agent context budget (EXP-353/EXP-637)

Keep this file under 40k chars. MCP clients DEFER tool defs behind tool search (only names + server `instructions` load at start; `_meta["anthropic/alwaysLoad"]` opts back in): the always-loaded set == `lib/mcp/always-load.ts` and <10k serialized, whole surface <60k, per-tool <1.8k, `MCP_SERVER_INSTRUCTIONS` <2k with a self-contained first 512. Gated by `lib/mcp/context-budget.test.ts`. `exponential_issues_list` defaults to OPEN work (`includeClosed`), limit 50/max 1000, descriptions cut at 200 chars (EXP-847). **Compress instead of appending**: a rule over its rationale, a citation over a restated list.
