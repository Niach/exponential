# Exponential

Real-time issue tracker.

## Tech Stack

TanStack Start (React 19, TanStack Router/React DB) · PostgreSQL 17 via Drizzle (`snake_case` casing) · ElectricSQL (shape proxy pattern, `@tanstack/electric-db-collection`) · Better Auth (email/password, email OTP, passkeys, Google/Apple, OIDC `genericOAuth`, session-based, `tanstackStartCookies`) · tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync) · shadcn/ui on Tailwind v4 (OKLCH zinc, dark forced via `html.dark`; dates via `react-day-picker` + `date-fns`) · bun. Dev infra (Docker Compose): Postgres:54321, Electric:30000, Garage:3900 (S3), Caddy:3000 (copy `Caddyfile.example`), steer-relay:4002 (`--profile steer`).

## Monorepo Layout

```
apps/
├── web/ # TanStack Start app (the issue tracker)
├── push-relay/ # Push relay (Hono/Bun)
├── steer-relay/# Remote-start + live-steer WS hub (Bun)
├── marketing/ # Vite + React; the Remotion ClosedLoop hero (src/movie/)
├── ios/ # SwiftUI (Tuist + GRDB; ExpCore/ExpUI)
├── android/ # Kotlin / Jetpack Compose
├── styleguide/ # Shot gallery + REAL @exp/ui islands
└── desktop/ # Rust IDE (gpui); crates/cli = headless `exponential` daemon (EXP-403)
packages/
├── db-schema/ # Drizzle schema + shared zod/domain types
├── ui/ # @exp/ui: theme + shadcn set + shared primitives + islands
├── design-tokens/ # OKLCH→sRGB + motion tokens → Compose/SwiftUI/Rust
├── domain-contract/ # contract.json — canonical enums → per-language constants
├── icons/ # icons.json — the ONE icon registry → TS/Swift/Kotlin/Rust
├── electric-protocol/ # Shape wire contract + fixtures
├── emoji/ # emoji dataset → ONE json ×4
├── steer-ticket/ # HS256 ticket (web mints, relay verifies)
├── widget/ # Feedback widget (Preact + snapDOM)
├── view-catalog/ # views.json — every view × platform, drift-gated
├── shots/ # capture pipeline → shots/
└── tsconfig/
docs/ # third-party-licences.md + licences/
shots/ # COMMITTED webp store, <view>/<platform>.webp
docker-compose.yaml # DEV backend stack (not the self-host one)
selfhost/ # Pull-an-image compose; INSTALL.md = the runbook
Dockerfile{,.push-relay,.steer-relay} # build context = repo root
```

Workspace names: `@exp/<dir>`; `apps/desktop` = a Cargo workspace.

**Dead, never reintroduce:** releases + footage/fixtures (EXP-106), `@exp/video`, workspace/project vocabulary + `/w/`/`/projects/` URLs (EXP-180), board types, `agent_runs` + agent-core + the companion daemon + `isAgent` identity, the `assigned-issues` shape, `run_configs`, `claude_task` (EXP-259), `isProtected` boards + dogfood cases (EXP-364), due-date time-of-day (REV2-49), `SELF_HOSTED` (now `CLOUD_INSTANCE`), `GOOGLE_CALENDAR_ENABLED`/`DOGFOOD_REPO`, the builtin `todo` status (EXP-685), skip-permissions (EXP-690), PTY coding + `start_in_terminal` (EXP-773), the pi agent + external ACP agents (EXP-849/EXP-862), issue filtering (EXP-862), node budgets.

## Product Invariants

**Vocabulary (EXP-180):** the product says **team** and **board** EVERYWHERE: copy, URLs (`/t/$teamSlug/boards/$boardSlug/issues/$id`), identifiers, DB, routers, shapes, MCP tools. Boards have no types; `repository_id` NULLABLE; coding gates on repo PRESENCE.

**Client parity:** all four clients sync the same 25 Electric shapes (`routes/api/shapes/` IS the list). `pins` (EXP-778) + `issue_drafts` (EXP-878) = per-user static, never trash-scoped: a row renders only when its target/board resolves; draft attachments (`draft_id`, issue_id NULL) never sync (tRPC `issueDrafts.listAttachments`), upload eagerly to `/api/issue-drafts/{id}/files`, reparent on `issues.create({draftId})`. The `actions` shape EXCLUDES the `body` (tRPC `actions.get`). `devices` + `device_worktrees` (EXP-481) sync `user_id = me OR shared_team_ids && (member teams)` via trigger mirrors; devices rows = SERVER-AUTHORITATIVE (persisted `launch_defaults` the machine converges to; heartbeat ~30s; online = `last_seen_at` within `contract.device.onlineWindowSeconds`). `repositories`, `user_notification_prefs`, `email_deliveries`, `conversion_events`, `device_commands`, `passkeys`, helpdesk and widget tables = **server-only (tRPC), never synced**.

**Nothing is anonymously readable:** every shape = member-only (anonymous → impossible-match sentinel); no public tRPC; attachment reads need membership. ONLY anonymous endpoints: widget (`/api/widget/*`), helpdesk reporter magic-link (`/api/support/*` + `/support/$token`), invites, auth, `/about` + `/NOTICES.txt`, MCP OAuth CIMD + state-gated callback (`/api/mcp-oauth/{client.json,callback}`, EXP-792), `/api/{session-results,attachment-uploads}/$token` (HMAC uploads, EXP-879/929). Board-scoped shapes = TEAM-scoped with a STATIC trash predicate (REV2-5): `team_id IN (member teams) AND board_deleted_at IS NULL` (a trigger-fanned mirror on every issue child) and **shape identities rotate ONLY on membership changes**. Batch `coding_sessions` rows and issue-less notifications keep NULL `board_deleted_at` and always sync; the notifications shape = static per user (`user_id = me AND board_deleted_at IS NULL`; fan-out filters recipients at delivery, delivered rows outlive membership); issue-less `support_reply` rows carry a synced `team_id` for Support routing.

**Permissions are membership-only:** `lib/auth/access.ts` `resolveTeamAccess` = the single authority (capabilities `read`/`comment`/`create_issue`/`mutate_resources`), mirrored by `use-team-permissions.ts` + the natives; every member moderates and handles support, owners own the destructive/settings surface (owner-only controls HIDDEN from non-owners; destructive native actions confirm). Signups get **no** team: first-run = "Create a team" (`teams.create`; creator = owner; cloud cap `FREE_OWNED_TEAMS_CAP`) or "Join a team" (invite link; `teamInvites.accept` stamps `onboardingCompletedAt`; invites may carry a synced `email`). `teams.getDefault` = the NON-CREATING resolver (oldest membership). An owner may delete ANY team incl. the last. **Billing and the admin console are web-only. Coding sessions run only on the desktop app and the headless `exponential` CLI daemon** (EXP-403; device-code login at `/auth/device`), both publishing scrubbed activity to the steer relay; a LIVE session = visible/steerable ONLY by its owner (EXP-312); teammates see the status badge.

The app = **noindex** (`__root.tsx` meta + `X-Robots-Tag`); marketing owns the indexed surface (`src/lib/seo.ts` `PAGES`).

## Shared Contracts

**Icons (EXP-273/317):** ONE icon set, Lucide, byte-identical ×4, generated from `packages/icons/icons.json` into COMMITTED per-platform outputs (web: `@exp/ui` `icons.generated.ts`). Change an icon in `icons.json` + regenerate; never hand-edit generated files nor add per-platform glyph maps. `pickable` (contract `boardIcon`) + `devicePickable` (`deviceIcon`) = **APPEND-ONLY**: reordering orphans rows. Multi-client surfaces name a CONCEPT (`conceptIcon(\`nav-search\`)` / `registry::NAV_SEARCH` / `AppIcons.navSearch` / `ExpIcons.navSearch`), never a raw glyph; iOS renders via `AppIcon`, NOT `Image(systemName:)`; `icons.test.ts` gates drift + the concept rules. Desktop assets hold hand-kept BRAND marks (`claude`/`codex`/`logo*`).

**Enums:** canonical values live in `packages/domain-contract/contract.json` (support-thread `status`/`direction`/`visibility` = documented varchars); a `db-schema/src/domain.ts` change = update `contract.json` + regenerate.

**Markdown:** `issues.description` + `comments.body` = plain `text` GFM, one interchange: web TipTap + tiptap-markdown, iOS cmark-gfm, desktop comrak + vendored WYSIWYG `crates/gpui-markdown-editor`, Android commonmark-java. The round-trippable feature set IS `CONTRACT_FIXTURES` (byte-locked ×4). No underline, no slash commands. **Tables** (EXP-726): canonical `| a | b |`/`| --- |` rows, `:---` alignment, one inline paragraph per cell, `\|` escape; top-level only, natives HOIST nested ones out (EXP-728). **Mentions** = plain `@<email>` (`lib/integrations/mentions.ts`; fires `issue_mention`, auto-subscribes); **issue mentions** = plain `#<IDENTIFIER>` (`lib/issue-refs.ts`, `#` autocomplete ×4; a pill renders only for a synced same-team issue; a ref auto-links both as `related`, source `reference`). Images stored relative `![alt](/api/attachments/{id})` (server canonicalizes; attachments carry probed `width`/`height`). **Inline media (EXP-824):** `video/*`/`audio/*` rows embed as a PLAIN LINK alone in a paragraph, `[clip.mp4](/api/attachments/{id})`, upgraded to a player from the synced row (`duration_ms`, `poster_storage_key` → `?poster=1`); normalisation = CLIENT-side (H.264+AAC MP4, mobile 720p); the server only probes MP4/MOV headers. **Emoji** (EXP-551) insert as unicode, never `:shortcode:`; picker + `:` typeahead data generated ONCE by `packages/emoji` (drift-gated).

## Commands

From repo root.

```bash
bun install
bun run backend  # docker compose up -d + dev server (:3000 via Caddy)
bun run ios / ios:test  # tuist+Xcode / ExpCore+ExpUI suites (Mac-only)
bun run android  # productionDebug install + launch
bun dev  # web dev server (:5173)
bun run {dev,build}:marketing / movie:{studio,render,poster,still}
bun run {dev,start}:push-relay / {dev,start,test}:steer-relay  # :4001 / :4002
bun run build  # widget FIRST, then web + marketing
bun run build:web / build:widget / test:widget / dev:widget (watch, /widget/v1/demo.html)
bun run typecheck / test / test:e2e  # web
bun run migrate / migrate:generate / psql / backend:{up,down,clear} (clear wipes volumes) / storage:init (Garage bootstrap)
bun run dev:desktop / {build,appimage,macapp,test}:desktop  # gpui IDE vs the local backend
bun run --filter @exp/{domain-contract,design-tokens,icons} generate
cd apps/web && bun run seed:screenshots  # demo data; then `bun run shots` → shots/
```

Workspace scripts: `bun --filter @exp/web <script>`; plain `cargo` in `apps/desktop/`. Never `bun run lint` (corrupts `typeof import()`) or `bun run format`.

## Deploys

Coolify (`coolify.home.straehhuber.com`, Hetzner), **home-LAN-only**: `coolify deploy uuid <uuid>` after a green Actions run. `build-web.yml` publishes multi-arch `ghcr.io/niach/exponential-web` on master pushes + `v*` tags; the SAME image = cloud, staging and self-host (package PUBLIC, self-hosters pin semver). Its runtime `bun install` = `--filter '@exp/web'` (EXP-380); licence/notice rules: `docs/third-party-licences.md` (gated by `third-party-licences.test.ts`). Native releases = tag-triggered (`build-{android,desktop,cli,ios}.yml`): `android-v*` (APK + Play bundle), `desktop-v*` (production + staging × 3 OSes, `make_latest: true`, self-update `crates/updater`), `cli-v*` (bare `exponential-<target>` binaries, marketing `install.sh`, self-host via `EXP_INSTANCE`), `ios-v*` (ASC upload).

**The operations runbook (infra uuids, buckets, signing, releases) lives OUTSIDE the repo**; read it first.

Every user-facing release PREPENDS a `ChangelogEntry` to `lib/changelog.ts` (gated by `changelog.test.ts`; head id = "What's new" on web + `crates/ui/src/changelog.rs`).

After schema changes: `bun run migrate:generate && bun run migrate`. Custom triggers (`db/out/custom/0001_triggers.sql`) auto-apply at boot (`applyCustomSql`, idempotent).

## Web App Structure (`apps/web/src/`)

The shadcn set, theme `styles.css`, `cn`, icon registry and shared primitives (`IssueChip`, `UserAvatar`, `LiveDot`, `StatusGlyph`, `Pill`, rows) = `@exp/ui` (`packages/ui/src`; `@exp/ui/island` = shadow-root islands for styleguide + marketing); app compositions stay flat in `components/` (`agent-session.tsx` = the steer/activity view). `lib/trpc/` = one file per router (`routes/api/trpc/$.ts` lists them). `lib/auth/membership.ts` = data lookups; `access.ts` = authorization. `lib/notification-email-policy.ts`/`-digest.ts`: push fires on create, email = a DIGEST of still-unread (DAILY at a user-chosen local hour, hourly legacy opt-in, atomic `emailed_at` claim; `server-bun.ts` schedules). EXP-801: MCP `exponential_notifications_send` = issue-less team-scoped `agent_message` row + push to members/self (one inbox row each ×4); prefs `allow_agent_messages=false` BLOCKS other members' agents (own always pass). EXP-980: `setBlocked` null→set sends `session_blocked` to EVERY run's OWNER (issue-less + synced `session_id`; row + push open the run ×4). Team routes under `t/$teamSlug/`: inbox `?tab=my-issues` = a TAB, not a route (`?tab=drafts` phone-only; `drafts` route + sidebar entry only while drafts exist; board `?draft=` reopens the create dialog, EXP-878); `reviews/$issueIdentifier` = the cross-board open-PR queue (confirmed squash merge); `agent` = the composer (every play button routes here with `?issues=|action=|pr=|device=|text=|icon=`, one-shot; `?from=` = where Back returns), `sessions/$sessionId` steers inside it (EXP-818). Also `auth/consent.tsx`, `invite/$token`. Entry: `router.tsx`, `start.tsx` (`defaultSsr: false`), `server{,-bun}.ts`.

## Database

`@exp/db-schema` = authoritative — never mirror it here.

### Conventions

Better Auth user IDs (so all user FKs) = `text`; app tables use UUID PKs (`gen_random_uuid()`) and timezone `created_at`/`updated_at`; sort orders = `doublePrecision` (fractional indexing).

### Non-obvious fields

Issues DUAL-WRITE `status` (the builtin ANCHOR enum) and `statusId` (nullable FK `issue_statuses` SET NULL, the precise per-team row); `creatorId` NULLABLE (widget issues have none), `source` = `user`/`widget`; comments thread ONE level (`parent_id`, replies re-parent to the root) + `source` user|mcp stamped from `ctx.viaMcp` ("via MCP" ×4, EXP-741); `duplicateOfId` pairs with status `duplicate` and dual-writes an `issue_relations` `duplicate` row (EXP-736: canonical-direction rows `blocks`/`parent`/`duplicate`/`related`, `source` user|reference, member-managed via `relations` + 2 MCP tools, events on both sides; EXP-980: `blocks`/`parent` refuse TRANSITIVE cycles, `lib/relation-cycles.ts`); the PR fields mean ONE PR per issue on `exp/<IDENTIFIER>` (batch issues share ONE `prUrl`). `coding_sessions` = issue XOR batch XOR action-scoped (action rows carry `action_id` [set null] + an `action_name` snapshot; batch rows `batch_issue_ids`, the covered set NAMING them `EXP-874 +2` ×4, `lib/batch-run.ts`). Teams carry a server-only `compTier` plus synced `helpdeskEnabled` and PR automation (`prOpened*`/`prMerged*` `StatusId` + `Automation`: nullable FKs SET NULL, NULL = builtin default target, `*Automation=false` = do nothing; member-gated `statuses.setPrAutomation`; UI web + desktop).

### Enum behavior

Values in `contract.json`. `issue_status`: `pr_open` flips linked issues to the team's PR-open target (default `in_review`), merge to the PR-merge target (default `done`). `coding_session_status` (running/in_review/ended): `in_review` = PR open; PR MERGE **ends** live sessions on EVERY path (EXP-498) unless the team's synced `endSessionsOnMerge` is false or MCP `pr_merge({endSessions})` overrides (EXP-711), never the session that merged its OWN PR (server-only `merged_own_pr`, EXP-637). Orphan PG labels: `merged`, `todo`; `ended` also via `killSession`/`codingSessions.end`. `ended_by` (agent|user|client|merge|system) records the path; `exponential_sessions_end` (report not stored, EXP-862) = REGISTERED only for UNATTENDED runs (`started_reason` schedule|event|`agent` = a `sessions_start` child; synced `parent_session_id` nests it, `session-tree` ×4; EXP-679/818) and ENDING them (EXP-673); a person-started run has NO idle bound (EXP-674) except a queued daemon update (FEED-36: idle ≥2h; `update_now`/cap `update-now` ends now); `needs_input` and `blocked` (EXP-804 jsonb, device-written, a walled run stays `running`; set ONLY by a refusal, never `allowed_warning`; `window` from claude's `rateLimitType`) land on every live status; `agent_busy` (device-written per turn edge, EXP-848) = the ONLY list-spinner input ×4; Automations lists finished AUTOMATED runs, Running/Recent the person-started ones; `resumed_from_id` links a resume to its predecessor; EXP-906: a resume INHERITS `parent_session_id`+`started_reason` (`codingSessions.start`; the resume frame re-brands `agent`), re-stamps its children; child messages follow the parent's resume succession (`resolveLiveParentSessionId`). Batch sessions (issue_id NULL) self-close on the desktop once their branch's issues sync `prState=merged`.

### Custom triggers

`db/out/custom/0001_triggers.sql` = 16 commented functions (identifiers, `updated_at`, scoping/board-hide/membership mirrors, trash/archive fan-outs, builtin statuses, `status_id` from the anchor, immutable `creem_subscription_id`).

## Patterns

### Electric shape proxies

One proxy per synced table in `routes/api/shapes/` (`createShapeRouteHandler`). **Every proxy pins a server-side `columns` allowlist clients cannot widen; new server-only columns go BEHIND it**. `users` pins exactly `id,name,email,image,created_at,updated_at`; `teams` its contract list (`comp_tier`, `agent_prompt` never); `issue-subscribers` drops reporter `email`, `actions` `body`, board-scoped shapes their scoping columns. A shape may FILTER on an excluded column (Electric evaluates `where` server-side). Hardening: `cache-control: private, no-store` + `vary: authorization, x-api-key, cookie`; bad token credentials → 401, never the anonymous clause; `buildWhereClause` SORTS id lists (= the shape identity); membership ids stay OUT of board-scoped clauses (`buildTeamScopedChildWhere`). A FIFO semaphore (`electric-proxy.ts`) bounds snapshot-class forwarding (REV-27), never live long-polls.

### Board trash (48h soft delete) + archive (EXP-500)

Owner-only `boards.delete` stamps `deleted_at`; **archive = same machinery minus the purge** (`archived_at` + a `board_archived_at` child mirror). Both vanish server-side: `boardVisible()` (`lib/board-visibility.ts`, every boards join takes it) + the shapes' static `IS NULL` suffixes + trigger-fanned mirrors (REV2-103). Both keep the `(team_id, slug)` reservation; web-only "Pending deletion"/"Archived boards" cards restore them. The purge sweep (`lib/board-trash.ts`) keys on `deleted_at` alone.

### Custom issue statuses (EXP-314)

Per-TEAM rows, six fixed categories (backlog/unstarted/started/completed/cancelled/duplicate; contract: values + ONE `displayOrder` + `startedMax: 4`). 6 LOCKED builtins per team (`builtin_key` = legacy enum values, never renamed/recolored/deleted; trigger-seeded; `unstarted` EMPTY since EXP-685). Customs = name+color, member-managed (`statuses` router; delete reassigns). `issues.status` STAYS the dual-written **anchor** (`CATEGORY_ANCHOR` in `db-schema/src/domain.ts`; unstarted customs anchor to `backlog`). `statusId` = the precise row (`populate_issue_status_id` re-anchors enum-only writers). Resolution/colors/glyphs: `lib/team-statuses.ts` + `lib/status-icons.ts` (×4, lock-tested). Lists group by status ROW; duplicate: no customs, enum+`duplicateOfId` lockstep.

### Web plumbing

- **Collections** (`lib/collections.ts`): all use `columnMapper: snakeCamelMapper()`, else `useLiveQuery` `where` silently fails. `undefined` (not `false`) skips a query; `and()`/`or()` from `@tanstack/react-db`, never `&&`/`||`.
- **Auth guard**: `_authenticated.tsx` `beforeLoad` + `throw redirect()`.
- **MCP OAuth consent**: `lib/auth/mcp-authorize-guard.ts` pre-flights every `mcp/authorize` (forces `prompt=consent`) → `/auth/consent` team/board multi-select → `mcp_grants` BEFORE the code mints. `lib/mcp/scope.ts` confines OAuth tokens to the grant (none = nothing) and to `/api/mcp`; cookies + `expu_` keys = full access. Login resumes interrupted authorizes.
- **Issue UI**: issue detail = a route fed a live Electric `issue` (md+: board list beside it); title/description save on blur, other fields at once; `completedAt` auto-managed. **Activity (EXP-900)**: `issue_events` fold at read time ×4 (`lib/activity/fold.ts`, fixture-locked): one actor+field ≤10 min → NET change (a no-op vanishes), broken by other actors' events/comments; "Show all" = raw.
- **Issue lists (EXP-980)**: sub-issues nest under the parent, the ROOT decides group + position (`lib/issue-nesting.ts`); ONE blocks badge per row opens THE mini-graph (`lib/issue-graph.ts`; also `PrGraphBadge`, blocked-start dialog, workflow detail); ×4, fixture-locked. EXP-998/1057: web md+ + desktop swap it for the RAIL (`lib/issue-rail.ts`, `components/issue-rail.tsx`; desktop `issue_rail.rs`): dots only, hover = mini-graph, geometry `issue-graph-geometry.json` ×4; phones keep the badge; tree connectors = 3px corner, unbroken tee.
- **Issue search (EXP-892)**: ONE engine ×4, `lib/issue-search.ts` (fixture-locked), `useIssueSearchResults` in every picker; `lib/issue-search-sql.ts` behind `issues.search` AND MCP `issues_list.search`; desktop search = issues only; lists: top row preselected, ↑/↓, Enter/Tab pick.

## Environment Variables

**Root `.env.example` = the CANONICAL reference**, `selfhost/.env.example` its self-host subset, relays own `apps/*/.env.example`; read them. Not obvious from those:

- `CLOUD_INSTANCE` = the opt-IN cloud marker (EXP-364): `'true'` = billing, plan limits, in-app widget, conversion tracking; unset = self-hosted, every FEATURE limit unlocked; `INITIAL_ADMIN_EMAILS` auto-promotes global admins.
- `AUTH_PASSWORD_ENABLED`/`AUTH_SIGNUP_ENABLED`: password login defaults true, public signup on in dev, OFF in production builds (`selfhost/docker-compose.yaml` re-defaults `true`). Auth posture = BUILD-derived (`lib/production-build.ts` `isProductionBuild`, REV-5), never runtime `NODE_ENV`. EXP-857: `AUTH_EMAIL_OTP_ENABLED` defaults on WITH a mail transport, `AUTH_PASSKEY_ENABLED` WITH an https base (rpID = host; Android origins from `ANDROID_APP_LINK_FINGERPRINTS`); login = ONE "Continue with …" list ×4; `mobile-oauth-start?provider=browser` = the native browser handoff.
- Mail: SES (`AWS_SES_REGION` + creds) OR `SMTP_*`, SES wins; neither = no mail.
- OIDC: `OIDC_PROVIDERS` (JSON array) = primary; `AUTH_OIDC_ENABLED`/`OIDC_*` = legacy, read only when unset.
- GitHub App installs are claimed PER TEAM (`github_installation_links`); `GITHUB_APP_CLIENT_SECRET` unset ⇒ install-page round-trip; `GITHUB_POLLING=true` = outbound merge cron for NAT'd self-hosts.
- `STEER_RELAY_URL` unset = remote start/steer off; HS256 `STEER_RELAY_SECRET` must match the relay; BOTH relays need `TRUST_PROXY=true` behind a proxy.
- `CLIENT_MIN_VERSION_{ANDROID,IOS,DESKTOP}` gate with HTTP 426 + a blocking update screen (unset = off); MARKETING versions, never build numbers; `CLIENT_LATEST_VERSION_*` = informational.
- Widget rate limits (`WIDGET_RATE_LIMIT_*`, `WIDGET_CONFIG_RATE_LIMIT_*`, REV-25): the per-KEY ones = self-host-only; cloud = per-TEAM plan ceiling (`lib/widget/submit-limit.ts`).

## Coding sessions & Actions

### The launcher

A thin launcher (`coding::prepare`, desktop + CLI): the issue's repo (tRPC) → a session-gated JIT GitHub-App token → a worktree + `exp/<IDENTIFIER>` branch with ambient git auth from a repo-local credential helper (EXP-73) → the `/api/mcp` MCP config with the user's `expu_` apikey, NOT `.mcp.json` → the system-prompt append (`skill::system_append`, claude `--append-system-prompt`/codex `developer_instructions`, rebuilt on EVERY start/resume/shell; the seed prompt never): the playbook `crates/coding/src/skill.md` (6 KiB, `context-budget.test.ts` + `skill.rs`) + the TEAM PROMPT (EXP-1025: server-only `teams.agent_prompt`, `teams.getAgentPrompt`/`update`, owner-edited in Settings → General web + IDE, cap contract `team.agentPromptMaxBytes`) → plan-first. The agent commits, pushes, opens its PR via MCP `open_pr` (the `X-Exp-Session-Id` MCP header names the run), unattended ones then `sessions_end`. Action and chat runs get their OWN worktree + branch (`exp/<slug>-<id8>` / `exp/chat-<id8>`); a repo-less chat runs in a scratch dir (EXP-739); agents never write to the trunk; clean worktrees go at end. Local deps: `git` + agent CLIs, never `gh`. Default branches resolve live (healed on `repositories.list`/`installationToken`), never assume `main`: `boards.default_branch` → team `default_branch_override` → GitHub.

### Agents & the engine

EXP-201/EXP-746; `coding/src/agent.rs`, contract `codingAgent`. ONE engine (`crates/engine`) runs every session as in-process ACP 2.x on the user's UNMODIFIED CLI (`adapters/` = claude stream-json + control protocol, codex app-server); ONE `SessionUpdate`→`ActivityEvent` mapper feeds relay + UI; never spoof `CLAUDE_CODE_ENTRYPOINT`; logins + agent shell tabs = the ONLY agent PTYs (EXP-773). Claude ALWAYS gets `--allow-dangerously-skip-permissions`; adapter auto-allows every `can_use_tool` but `ExitPlanMode`/`AskUserQuestion`; codex full-access; ultracode claude-only; plan mode claude+codex. `runs.json` records the ACP id + the agent's NATIVE id (re-read on `/clear`, EXP-784); a resume re-enters the RECORDED run (EXP-662, `RESUMED_IDLE_CAPTION` until its first prompt); a repo-less run purges WHOLE at end (EXP-764); `.exp-agents` gates worktree reuse. Doctor gates the SELECTED agent (git always) + ACP; pickers offer only `devices.register`'s runnable agents. Nav (EXP-870/923, desktop+web ≥md): the rail never leaves (`derive_origin`); issue + run = ONE top tab (Issue|Run); live own runs = the rail's Running section, NEVER a tab (a `running`-origin open reuses the issue tab, a merge end closes it); session lists ×4 = the ACTIVE team only, the team picker's corner dot = own live runs in OTHER teams, amber = needs input (EXP-1075); Recent behind the Agent page's history button; nested lists ×4 draw `tree-guides` (EXP-965); bottom bar = terminals. Phones (EXP-893): ONE Work screen, Issue|Run|Changes|Results faces as view state (Results = `coding_sessions.results` screenshots by topic, MCP `sessions_results`, EXP-879), bar `[context][capsule][switcher]` (`lib/work-faces.ts`), Stop/Resume top-right on Run only; list rows carry no buttons ×4. Steering: plan/model/effort launch-time (EXP-790); `config_state.options` carries ONLY `model` (EXP-877, `/model <alias>`); `config_state`/`usage`/`rate_limit`/`queue`/`task_list`/`context_layout` = latest-wins STATE (EXP-1051: `context_layout` = per-run segments, `base` measured from the first prefix, the rest bytes÷4; `lib/context-layout` ×4 fixture-locked; the usage popover's top row + segmented bar; switch refusals = tooltips/toasts); EXP-861/873: `queue` bar ×4 = UNREAD messages (mid-turn sent at once, `sent` until claude's replay lands the row; mid-compaction HELD, `unqueue` revokes held only; Stop returns all to the composer); `tool` carries `id`+contract `toolKind`, `tool_update` patches it, a collapsed run's caption = contract `toolGroupSummary` ×4 (EXP-785/786); contract `expToolDisplay` labels Exponential MCP calls (EXP-846); `turn` (started|ended) drives the spinner + `agent_busy`; a pending question/plan answers IN its card (EXP-820); `/` = contract `steerCommands` ∪ agent commands. Narration carries `messageId` (fragments merge) + on user rows `subagentId` (subagent view only ×4). Transcripts live ONLY on the device (`{data_dir}/journal/<id>.jsonl` + `.diff.json`, capped, pruned per `sessionRetentionDays`); a viewer join with no live room → `history_request`, the device republishes into a lingering room → `activity_synced`, else `device_offline`/`history_unavailable` (EXP-796). `subagent.title` = the spawning call's description (EXP-847); a read-only Plan chip mirrors `config_state.currentMode`; ending verb = Stop ×4; pin controls only beside a sidebar (EXP-858). **Accounts (EXP-849):** per-account config dirs (`agent_profiles`: `CLAUDE_CONFIG_DIR`/`CODEX_HOME`); credentials NEVER copied, synced or brokered, transcripts may be. `devices.agent_accounts`/`agent_usage` jsonb ride the heartbeat READ-ONLY; `health` (ok|needs_relogin|signed_out|unknown) = usage probe's 401; keep-alive: codex `account/read` 6-hourly; claude = OUR refresh-token POST under the CLI's `.oauth_refresh.lock`, written back ONLY to its source store (EXP-852/909). Accounts = decision surface; Devices = repair (never `codex logout` on the ambient login). Remote start/resume carry `account` (EXP-906); a mid-run switch = claude-only `startSession({resumeSessionId, account})` on an idle turn. A poll pins past a reset only when EVERY window is maxed with a stamp, ≤6h, re-probing once one passed (EXP-817/964); Merge sits in the Changes bar (diff vs merge-base of `origin/<default>`). EXP-792 symmetric strict: only Exponential-managed MCP servers connect on every agent (claude `--strict-mcp-config`, codex the WHOLE `mcp_servers` table); team rows = server-only, injected with device-held secrets as `${VAR}` refs. EXP-891: per-DEVICE rows too (`device_mcp_servers`, server-only tRPC, from `{data_dir}/mcp/device-servers.json`; autodetected, never secrets; enabled rows ride EVERY run, a team key wins).

### Batch runs

Multi-issue coding = **batch runs**. The Agent page composer = the ONE launcher ×4 (EXP-825; issue chips OR one action chip; free text = `prompt`; ONE repo per run): 1 picked issue = single-issue, 2+ = BATCH: ONE session on ONE branch `exp/batch-<id8>`, one prompt, ONE combined PR via `exponential_pr_open` with `issueIds` + `head` (same repo enforced); a PR resolves to ALL linked issues by exact `pr_url`. `codingSessions.start` takes exactly one of issueId/teamId. EXP-626: `pr_open` also takes `repositoryId` + `head` (`pr_merge` `repositoryId` + `prNumber`) for an issue-less PR: nothing linked or notified, it lands on the CALLER's synced row (`coding_sessions.pr_*`, EXP-734), merges from the run (`codingSessions.mergePr`), lists under Reviews → Agent runs.

### Stacked runs (EXP-897, FEED-43)

A PR based on another open PR's branch = a **stack**: `issues.pr_base_branch` (synced; edge `child.pr_base_branch == lower.branch`, nesting client-side, `lib/pr-stack.ts` ×4) + server-only `pr_stack_number` (a REAL GitHub stack, preview API `2026-03-10`, `github-pr.ts`; 404 = plain base-branch PR). Members merge ONLY via merge-async + poll (`mergePullRequestSmart`); merging PR k merges all below, GitHub retargets the next (`retargetChildrenOfMergedPr` skips real members); `retargetPr` refuses members by name. MCP: `pr_open({stackOnIssueId})` (base = lower's branch, stack extended, `blocks` lower→upper written, merged-branch `base` refused), `pr_merge({mergeStack})` (any member, merges the top), `sessions_start({stackOnIssueId, account})`, `sessions_ask_parent({to: parent|root|user})` (`user` = child `needs_input` + `agent_message`), `sessions_list({subtreeOf})`. **Third start mode:** a blocked issue's start (composer ×4) offers Cancel · Start anyway · Stacked PR (`steer.startSession({stack:true})`; batches ask too, resumes never; disabled per `stackDisabledReason` cycle|batch|cap `stacked-start`; `lib/stack-plan.ts` = transitive open blockers, same repo; `codingSessions.stackPlan` locally); the launcher cuts `exp/<IDENT>` from `origin/<lower.branch>` while that PR is open, else the default; the prompt's `## Stacked work` + playbook `## Stacked runs` drive it. Reviews nest members under the lowest (`Merge stack` there); ONE stack/batch badge + overlay in the work header (`lib/pr-graph.ts` ×4: Issue = blocked-by + batch, Run = session tree, Changes = PR stack).

### Actions (EXP-253)

`actions` rows (per team, markdown `body` ≤64KB, optional `repository_id` SET NULL + curated `icon`, ≤10 typed PICK inputs `repo|board|pr|icon`; EXP-825: free text = `steer.startSession({prompt})` → an "Additional instructions" section (hint `actions.prompt_placeholder`), images via `POST /api/teams/$teamId/session-files` + `codingSessions.start({attachmentIds})`, cap `start-prompt` gates the Chat/Create builtins): tRPC CRUD (member list/get, owner writes) + 4 MCP tools + the body-less shape. **Automations (EXP-583) = their OWN rows + shape** (`automations`: `action_id` target, `device_id` runner, nullable `agent`/`account` (EXP-995, a profile id on the runner)/`model`/`effort`, `trigger` jsonb schedule|event, `enabled`), never a field on actions; LOCAL-ONLY (no server scheduler: the bound device self-starts its enabled rows with `startedReason`+`automationId`); owner-only `automations` router + MCP `exponential_automations_*`; an enabled automation needs every input optional; withdrawing a device share disables its automations; Automations tab ×4. Runs = `coding_sessions` rows executed LOCALLY in a per-run worktree, a PR branch's worktree (fix-conflicts) or a scratch dir; never server-side secrets. Remote start = `steer.startSession({actionId, deviceId, agent?, model?, effort?})`; `resumeSessionId` + cap `resume-run` relaunch an ended run. All four clients edit actions in full (EXP-694); the creator run authors new ones, not a form.

Virtual builtins = client-CONSTRUCTED, never DB rows, **byte-identical** strings (`lib/builtin-actions.ts`, `api::actions::builtin_*`, `ActionsApi` ×2); MCP's list appends the two listed ones, tRPC's does not. They pin FIRST by the `builtin` flag, every builtin start carries `teamId`, `get/update/delete` reject the reserved ids. **"Create action"** (`builtin:create-action`, inputs `[repo?, icon?]`, the request = `prompt`) = the describe-it creator run, scratch cwd; the hidden Chat builtin (`[repo?]`, `prompt` required) = the composer with no subject. **"Fix merge conflicts"** (`builtin:fix-conflicts`) takes a required `pr` input (an open PR's representative issue id, batch PRs deduped by prUrl) and runs in that PR branch's WORKTREE (rebase, resolve, force-push, `exponential_pr_merge`); Reviews offers it on a failed merge. Prompts = shipped constants (`body` empty). Suggestion seeds (`action-suggestions.ts`, ×4) prefill the creator run's `text`.

### Workflows (EXP-978)

A **workflow** (contract `wf*`, NOT the feed's `workflowStatus`) = backlog issues of ONE repo run as a DAG: synced `blocks` = the edges, parent + sub-issues = ONE compound node (batch). `workflows` + `workflow_nodes` = team shapes; `wave`/`lane`/`metrics` = the SERVER layout (`lib/workflow-layout.ts`, re-derived by `lib/workflows.ts` on every `blocks`/`parent` write); clients only draw `lib/workflow-view.ts` (×4, fixture-locked). Member router + MCP `workflows_*`; hidden builtin `plan-workflow`. **The orchestrator is CODE**: pure `crates/coding/src/workflows/` `evaluate(snapshot)`, hosted by desktop + CLI, single writer = `workflows.device_id`, NO server scheduler; node runs = issue/batch runs (`started_reason=workflow`) based on `exp/wf-<id8>`, the one open blocker's branch, or a synthetic `exp/wf-<id8>-base-<IDENT>` merge (`start_on` contract|pr_open|landed); propagation by MERGE, never rebase; conflicts = `after_node_ids`. The GATE = server-side (`landNode`), agent review ONLY (EXP-1010): `approved_at` = a `builtin:review-node` run blind to the author's reasoning (≤3 rounds, void on a FAILED oracle) or a person; a merge into a blocker's branch waits for it. Mid-run follow-ups join as nodes (additive = admitted, else `proposed`). A node PR merges there: `pr_state=merged`, the issue STATUS waits for the final PR (`lib/workflow-final-pr.ts`). `pr_open` derives the base from membership; node questions = `ask_parent` → user + a `Proposal:` line, answers → `decisions`.

### Desktop IDE & mobile

Desktop IDE = master-only + autopull (no branch switch; changes land via PRs or Source Control's CONFIRMED commit-and-push; Discard-and-reset; `trunk_sync` badge + banner). Mobile first-run wizard (`lib/auth/onboarding.ts`, server-gated): create-or-join team, then a board with optional repo + GitHub App. Lists ×4 = a filled group band over FLAT hairline-divided rows (EXP-818/1076, settings too); `GlassGroup` = form fields only.

## Billing (per-seat, Creem — cloud only)

Subscriptions bind to a TEAM (`creem_subscriptions.team_id` + `seats`; `billing.createSeatCheckout`, Creem `units` = seats), not the purchaser (REV2-55, `lib/billing/billing-handover.ts`): `reference_id` nullable/set-null; account deletion NEVER blocked by billing, it only cancels a SOLO team's subscription it destroys; team deletes REFUSE a live subscription (`PRECONDITION_FAILED`; a period-end cancellation passes), natives point at web. ONE subscription per team: `createSeatCheckout` refuses duplicates; `billing.updateSeats`/`changePlan` mutate the EXISTING subscription with `update_behavior: proration-charge-immediately`. Free = 3 seats, 250MB, 1 widget; **Team** = the ONE paid tier, €15/seat/mo or €12 yearly: 10GB, unlimited widgets, helpdesk (`PlanTier = free|team|unlimited`). Boards/repos/coding sessions, push + steer = never plan-gated; over-seat teams only block invites.

**Limits exist only when `CLOUD_INSTANCE=true`** (a product switch; Apache-2.0, no licence gate). Enterprise Support: NO published pricing (EXP-218), marketing routes to `/contact/`. Self-host's one limit: no MOBILE push (store apps embed Firebase).

## Feedback widget & helpdesk

`packages/widget` (Preact shadow root + snapdom) builds an async IIFE loader + lazy panel into `apps/web/public/widget/v1/`; API = `window.ExponentialWidget`.

Server-only `widget_configs` (public `expw_` key + domain allowlist) + `widget_submissions`; public CORS routes `/api/widget/{config,submit}` (origin/rate-limit/honeypot in `lib/widget/`). **Modes** `feedback`/`support`/both (`form_config.modes`; absent = feedback-only). `form_config` also carries `labelIds` (≤10) and `theme` dark/light/auto + colors; ONE capture button (getDisplayMedia desktop, snapDOM mobile) + an Off/3s/5s hold segment. Feedback files an ordinary issue onto `widget_configs.board_id` (NULLABLE, required iff feedback mode): issue + screenshot attachment (null `uploader_id`) + submission row in ONE transaction, `source='widget'`. Support files a STANDALONE ticket (`support_threads` + opening `support_messages` row, NO issue) gated on `teams.helpdesk_enabled` + paid plan (`assertCanUseHelpdesk`, per submit); reporter auth = emailed magic link (`lib/helpdesk/token.ts`); members use the `helpdesk` router (close/reopen, `escalate` → linked issue); notify = issue-less `support_reply`. Rate limits = in-process token buckets; the ONLY cloud upsell = the widget settings' usage bar. Settings "Feedback widget" + "Helpdesk" = owner-only, web + IDE (read-only there). The in-app widget = HEADLESS behind the sidebar "Report bug" button (cloud-only; key in `lib/runtime-config.ts`, `FEEDBACK_WIDGET_KEY` overrides).

## Conversion tracking (EXP-362, cloud only)

`lib/conversion/` + `adminConversions` router, no-ops unless `CLOUD_INSTANCE=true`. COOKIELESS: visitors = daily-rotating salted HMAC of ip+ua; attribution = URL params only (`ref`/`utm_*`). `events.ts` owns the closed vocabulary; idempotency = PARTIAL UNIQUE INDEXES + `onConflictDoNothing`.

## Style Conventions

- Template literals; functional components only
- shadcn/ui from `@exp/ui` ALWAYS over raw `<input>`/`<button>`/`<textarea>`/`<label>`; icons by CONCEPT (above)

## Agent context budget (EXP-353/EXP-637)

Keep this file under 40k chars. MCP clients DEFER tool defs behind tool search (`_meta["anthropic/alwaysLoad"]` opts back in): the always-loaded set == `lib/mcp/always-load.ts`, <10k serialized, whole surface <60k, per-tool <1.8k, `MCP_SERVER_INSTRUCTIONS` <2k with a self-contained first 512; gated by `lib/mcp/context-budget.test.ts`. `exponential_issues_list` defaults to OPEN work, limit 50/max 1000, descriptions cut at 200 chars. **Compress, never append**: a rule over its rationale, a citation over a list.
