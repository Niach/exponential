# Exponential

Real-time issue tracker.

## Tech Stack

TanStack Start (React 19, TanStack Router/React DB) · PostgreSQL 17 via Drizzle (`snake_case` casing) · ElectricSQL (shape proxy pattern, `@tanstack/electric-db-collection`) · Better Auth (email/password, email OTP, passkeys, Google/Apple, OIDC `genericOAuth`, session-based, `tanstackStartCookies`) · tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync) · shadcn/ui on Tailwind v4 (OKLCH zinc, dark forced via `html.dark`; dates via `react-day-picker` + `date-fns`) · bun. Dev infra (Docker Compose): Postgres:54321, Electric:30000, Garage:3900 (S3), Caddy:3000 (`Caddyfile.example`), steer-relay:4002 (`--profile steer`).

## Monorepo Layout

```
apps/
├── web/ # TanStack Start app
├── push-relay/ # Hono/Bun
├── steer-relay/# remote-start + steer WS hub (Bun)
├── marketing/ # Vite + React; Remotion hero (src/movie/)
├── ios/ # SwiftUI (Tuist + GRDB; ExpCore/ExpUI)
├── android/ # Jetpack Compose
├── styleguide/ # Shot gallery + REAL @exp/ui islands
└── desktop/ # gpui IDE; crates/cli = the `exponential` daemon (EXP-403)
packages/
├── db-schema/ # Drizzle schema + shared zod/domain types
├── ui/ # @exp/ui: theme + shadcn set + shared primitives + islands
├── design-tokens/ # OKLCH→sRGB + motion tokens → Compose/SwiftUI/Rust
├── domain-contract/ # contract.json → per-language enums
├── icons/ # icons.json → TS/Swift/Kotlin/Rust
├── electric-protocol/ # Shape wire contract + fixtures
├── emoji/ # ONE emoji json ×4
├── steer-ticket/ # HS256 ticket (web mints, relay verifies)
├── widget/ # feedback widget (Preact + snapDOM)
├── view-catalog/ # views.json—every view × platform, drift-gated
├── shots/ # capture pipeline
└── tsconfig/
docs/ # third-party-licences.md + licences/
shots/ # COMMITTED webp store, <view>/<platform>.webp
docker-compose.yaml # DEV backend stack
selfhost/ # Pull-an-image compose; INSTALL.md = the runbook
Dockerfile{,.push-relay,.steer-relay} # build context = repo root
```

Workspace names: `@exp/<dir>`; `apps/desktop` = Cargo.

**Dead, never reintroduce:** releases + footage/fixtures (EXP-106), `@exp/video`, workspace/project vocabulary + `/w/`/`/projects/` URLs (EXP-180), board types, `agent_runs` + agent-core + the companion daemon + `isAgent` identity, the `assigned-issues` shape, `run_configs`, `claude_task` (EXP-259), `isProtected` boards + dogfood cases (EXP-364), due-date time-of-day (REV2-49), `SELF_HOSTED` (now `CLOUD_INSTANCE`), `GOOGLE_CALENDAR_ENABLED`/`DOGFOOD_REPO`, the builtin `todo` status (EXP-685), skip-permissions (EXP-690), PTY coding + `start_in_terminal` (EXP-773), standalone automations, the pi agent + external ACP agents (EXP-849/EXP-862), issue filtering (EXP-862), SLOP-3: workflows (EXP-978), stacked runs + the stack plan + `pr_stack_number` (EXP-897), Reviews nesting; SLOP-4: the helpdesk (`support_*` tables, the `helpdesk` router + MCP tools, `teams.helpdesk_enabled`, `support_reply`, widget `modes`, `/api/support/poll`, 4 Support screens).

## Product Invariants

**Vocabulary (EXP-180):** the product says **team** and **board** EVERYWHERE: copy, URLs (`/t/$teamSlug/boards/$boardSlug/issues/$id`), identifiers, DB, routers, shapes, MCP tools. Boards have no types; `repository_id` NULLABLE; coding gates on repo PRESENCE.

**Client parity:** all four clients sync the same 21 Electric shapes (`routes/api/shapes/` minus `automations` and 3 EMPTY `workflow*` sentinels old apps still poll, SLOP-14). `pins` (EXP-778) + `issue_drafts` (EXP-878) = per-user static, never trash-scoped: a row renders only when its target/board resolves; draft attachments (`draft_id`, issue_id NULL) never sync (`issueDrafts.listAttachments`; eager upload `/api/issue-drafts/{id}/files`), reparent on `issues.create({draftId})`, which stamps synced `issues.draft_id` (EXP-1231: an open page elsewhere lands on the issue, a seen row gone = discarded, `draftFate` ×4; a consumed id's upsert = CONFLICT). The `actions` shape EXCLUDES the `body` (tRPC `actions.get`) and carries `triggers`. `devices` + `device_worktrees` (EXP-481) sync `user_id = me OR shared_team_ids && (member teams)` via trigger mirrors; devices rows = SERVER-AUTHORITATIVE (persisted `launch_defaults`; heartbeat ~30s; online = `last_seen_at` within `contract.device.onlineWindowSeconds`). `repositories`, `user_notification_prefs`, `email_deliveries`, `conversion_events`, `device_commands`, `passkeys` and the widget tables = **server-only (tRPC), never synced**.

**Nothing is anonymously readable:** every shape = member-only (anonymous → impossible-match sentinel); no public tRPC; attachment reads need membership. ONLY anonymous endpoints: widget (`/api/widget/*`), the reporter magic-link page (`/api/support/{thread,reply,attachment}` + `/support/$token`, SLOP-4), invites, auth, `/about` + `/NOTICES.txt`, MCP OAuth CIMD + state-gated callback (`/api/mcp-oauth/{client.json,callback}`, EXP-792), `/api/{session-results,attachment-uploads}/$token` (HMAC uploads, EXP-879/929), `/api/cli/install-token/redeem` (EXP-1111). Board-scoped shapes = TEAM-scoped with a STATIC trash predicate (REV2-5): `team_id IN (member teams) AND board_deleted_at IS NULL` (a trigger-fanned mirror on every issue child) and **shape identities rotate ONLY on membership changes**. Batch `coding_sessions` rows and issue-less notifications keep NULL `board_deleted_at` and always sync; the notifications shape = static per user (`user_id = me AND board_deleted_at IS NULL`; fan-out filters recipients at delivery, delivered rows outlive membership); issue-less rows (`agent_message`, `session_blocked`) carry a synced `team_id`.

**Permissions are membership-only:** `lib/auth/access.ts` `resolveTeamAccess` = the single authority (capabilities `read`/`comment`/`create_issue`/`mutate_resources`), mirrored by `use-team-permissions.ts` + the natives; every member moderates and answers reporters, owners own the destructive/settings surface (owner-only controls HIDDEN from non-owners; destructive native actions confirm). Signups get **no** team: first-run = "Create a team" (`teams.create`; creator = owner; cloud cap `FREE_OWNED_TEAMS_CAP`) or "Join a team" (`teamInvites.accept` stamps `onboardingCompletedAt`; invites may carry a synced `email`). `teams.getDefault` = the non-creating resolver (oldest membership). An owner may delete ANY team incl. the last. **Billing and the admin console are web-only. Coding sessions run only on the desktop app and the headless `exponential` CLI daemon** (EXP-403; device-code login at `/auth/device`), both publishing scrubbed activity to the steer relay; a LIVE session = visible/steerable ONLY by its owner (EXP-312); teammates see the status badge.

The app = **noindex** (`__root.tsx` meta + `X-Robots-Tag`); marketing owns the indexed surface (`seo.ts` `PAGES`).

## Shared Contracts

**Icons (EXP-273/317):** ONE icon set, Lucide, byte-identical ×4, generated from `packages/icons/icons.json` into COMMITTED per-platform outputs (web: `@exp/ui` `icons.generated.ts`). Change an icon in `icons.json` + regenerate; never hand-edit generated files nor add per-platform glyph maps. `pickable` (contract `boardIcon`) + `devicePickable` (`deviceIcon`) = **APPEND-ONLY**: reordering orphans rows. Multi-client surfaces name a CONCEPT (`conceptIcon(\`nav-search\`)`, `registry::NAV_SEARCH`, `AppIcons`/`ExpIcons.navSearch`), never a raw glyph; iOS renders via `AppIcon`, NOT `Image(systemName:)`; `icons.test.ts` gates drift + the concept rules. Desktop assets hold hand-kept BRAND marks (`claude`/`codex`/`logo*`); MCP catalog marks = icons.json `brand` (selfh.st light SVGs, CC BY 4.0; rows = contract `mcpCatalog`).

**Enums:** canonical values live in `packages/domain-contract/contract.json`; a `db-schema/src/domain.ts` change = update `contract.json` + regenerate.

**Markdown:** `issues.description` + `comments.body` = plain `text` GFM, one interchange: web TipTap + tiptap-markdown, iOS cmark-gfm, desktop comrak + vendored WYSIWYG `crates/gpui-markdown-editor`, Android commonmark-java. The round-trippable feature set IS `CONTRACT_FIXTURES` (byte-locked ×4). No underline, no slash commands. **Tables** (EXP-726): canonical `| a | b |`/`| --- |` rows, `:---` alignment, one inline paragraph per cell, `\|` escape; top-level only, natives HOIST nested ones out (EXP-728). **Mentions** = plain `@<email>` (`lib/integrations/mentions.ts`; fires `issue_mention`, auto-subscribes); **issue mentions** = plain `#<IDENTIFIER>` (`lib/issue-refs.ts`, `#` autocomplete ×4; a pill renders only for a synced same-team issue; a ref auto-links both as `related`, source `reference`). Images stored relative `![alt](/api/attachments/{id})` (server canonicalizes; attachments carry probed `width`/`height`). **Inline media (EXP-824):** `video/*`/`audio/*` rows embed as a PLAIN LINK alone in a paragraph, `[clip.mp4](/api/attachments/{id})`, upgraded to a player from the synced row (`duration_ms`, `poster_storage_key` → `?poster=1`); normalisation = CLIENT-side (H.264+AAC MP4, mobile 720p); the server only probes MP4/MOV headers. **Emoji** (EXP-551) insert as unicode, never `:shortcode:`; picker + `:` typeahead data generated ONCE by `packages/emoji` (drift-gated).

## Commands

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

Workspace scripts: `bun --filter @exp/web <script>`; `cargo` in `apps/desktop/`. Never `bun run lint`/`format`.

## Deploys

Coolify (`coolify.home.straehhuber.com`, Hetzner), **home-LAN-only**: `coolify deploy uuid <uuid>` after a green Actions run. `build-web.yml` publishes multi-arch `ghcr.io/niach/exponential-web` on master pushes + `v*` tags; ONE image for cloud, staging and self-host (public package, semver pins). Its runtime `bun install` = `--filter '@exp/web'` (EXP-380); licence/notice rules: `docs/third-party-licences.md` (gated by `third-party-licences.test.ts`). Native releases = tag-triggered (`build-{android,desktop,cli,ios}.yml`): `android-v*` (APK + Play bundle), `desktop-v*` (production + staging × 3 OSes, `make_latest: true`, self-update `crates/updater`), `cli-v*` (bare `exponential-<target>` binaries, marketing `install.sh`, self-host via `EXP_INSTANCE`), `ios-v*` (ASC upload).

**The operations runbook (infra uuids, buckets, signing, releases) lives OUTSIDE the repo.**

Every user-facing release PREPENDS a `ChangelogEntry` to `lib/changelog.ts` (gated; mirror `crates/ui/src/changelog.rs`).

After schema changes: `bun run migrate:generate && bun run migrate`; custom triggers auto-apply at boot (`applyCustomSql`).

## Web App Structure (`apps/web/src/`)

The shadcn set, theme `styles.css`, `cn`, icon registry and shared primitives (`IssueChip`, `UserAvatar`, `LiveDot`, `StatusGlyph`, `Pill`, rows, toasts—`Toaster` + `toast-stack.json` ×4) = `@exp/ui` (`packages/ui/src`; `@exp/ui/island` = shadow-root islands for styleguide + marketing); app compositions stay flat in `components/` (`agent-session.tsx` = the steer view). `lib/trpc/` = one file per router (`routes/api/trpc/$.ts` lists them). `lib/auth/membership.ts` = data lookups; `access.ts` = authorization. `lib/notification-email-policy.ts`/`-digest.ts`: push fires on create, email = a DIGEST of still-unread (DAILY at a user-chosen local hour, hourly legacy, atomic `emailed_at` claim; `server-bun.ts` schedules). EXP-801: MCP `exponential_notifications_send` = issue-less team-scoped `agent_message` row + push to members/self (one inbox row each ×4); prefs `allow_agent_messages=false` BLOCKS other members' agents (own always pass). EXP-980: `setBlocked` null→set sends `session_blocked` to EVERY run's OWNER (issue-less + synced `session_id`; row + push open the run ×4). Team routes under `t/$teamSlug/`: inbox `?tab=my-issues` = a TAB, not a route (`?tab=drafts` phone-only; `drafts` route + sidebar entry only while drafts exist); `drafts/$draftId` = New issue AS the detail in draft mode, NO create dialog ×4 (EXP-1170: openers mint the id, ONE coalesced `issueDrafts.upsert`, copy `issue-draft.json`); `reviews` = the cross-board open-PR queue, a row opens the issue's Changes face `?view=diff` (EXP-1154); `agent` = the composer (play buttons route here with one-shot `?issues=|action=|pr=|device=|text=|icon=`; `?from=` = Back target), `sessions/$sessionId` steers inside it (EXP-818). Entry: `router.tsx`, `start.tsx` (`defaultSsr: false`), `server{,-bun}.ts`.

## Database

`@exp/db-schema` = authoritative—never mirror it here.

### Conventions

Better Auth user IDs (so all user FKs) = `text`; app tables use UUID PKs and timezone `created_at`/`updated_at`; sort orders = `doublePrecision` (fractional indexing).

### Non-obvious fields

Issues DUAL-WRITE `status` (the builtin ANCHOR enum) and `statusId` (nullable FK `issue_statuses` SET NULL, the precise per-team row); `creatorId` NULLABLE (widget issues have none), `source` = `user`/`widget`; comments thread ONE level (`parent_id`, replies re-parent to the root) + `source` user|mcp|reporter (`mcp` stamped from `ctx.viaMcp`, "via MCP" ×4, EXP-741; `reporter` = a widget reporter's words, `author_id` NULL, "reporter" caption ×4) + `audience` team|reporter (SLOP-4: `reporter` shows on the reporter page and, from a member, is emailed to the reporter; `fixtures/reporter-reply.json` = the ×4 spec + copy); `duplicateOfId` pairs with status `duplicate` and dual-writes an `issue_relations` `duplicate` row (EXP-736: canonical-direction rows `blocks`/`parent`/`duplicate`/`related`, `source` user|reference, member-managed via `relations` + 2 MCP tools, events on both sides; EXP-980: `blocks`/`parent` refuse TRANSITIVE cycles, `lib/relation-cycles.ts`); the PR fields mean ONE PR per issue on `exp/<IDENTIFIER>` (batch issues share ONE `prUrl`). `coding_sessions` = issue XOR batch XOR action-scoped (action rows carry `action_id` [set null] + an `action_name` snapshot; batch rows `batch_issue_ids`, the covered set NAMING them `EXP-874 +2` ×4, `lib/batch-run.ts`). Teams carry a server-only `compTier` plus synced PR automation (`prOpened*`/`prMerged*` `StatusId` + `Automation`: nullable FKs SET NULL, NULL = builtin default target, `*Automation=false` = do nothing; member-gated `statuses.setPrAutomation`; UI web + desktop).

### Enum behavior

`issue_status`: `pr_open` flips linked issues to the team's PR-open target (default `in_review`), merge to the PR-merge target (default `done`). `coding_session_status` (running/in_review/ended): `in_review` = PR open; PR MERGE **ends** live sessions on EVERY path (EXP-498) unless the team's synced `endSessionsOnMerge` is false or MCP `pr_merge({endSessions})` overrides (EXP-711), never the session that merged its OWN PR (server-only `merged_own_pr`, EXP-637). Orphan PG labels: `merged`, `todo`; `ended` also via `killSession`/`codingSessions.end`. `ended_by` (agent|user|client|merge|system) records the path; `exponential_sessions_end` (report not stored, EXP-862) = REGISTERED for every run (EXP-1222), the LAST call of UNATTENDED ones (`started_reason` schedule|event|`agent` = a `sessions_start` child; synced `parent_session_id` nests it, `session-tree` ×4; EXP-679/818) and ENDING them (EXP-673); a person-started run calls it only when asked, has NO idle bound (EXP-674) except a queued daemon update (FEED-36: idle ≥2h; `update_now`/cap `update-now` ends now); `needs_input` and `blocked` (EXP-804 jsonb, device-written, a walled run stays `running`; set ONLY by a refusal, never `allowed_warning`; `window` from claude's `rateLimitType`) land on every live status; `agent_busy` (device-written per turn edge, EXP-848) = the ONLY list-spinner input ×4; an action's Runs lists ALL its runs, Running/Recent person-started; `resumed_from_id` links a resume to its predecessor; EXP-906: a resume INHERITS `parent_session_id`+`started_reason` (`codingSessions.start`; the predecessor's reason wins), re-stamps its children; child messages follow the parent's resume succession (`resolveLiveParentSessionId`). Every `pr_open` form stamps the CALLER's row (`pr_*`), so a merge ends any run server-side.

### Custom triggers

`db/out/custom/0001_triggers.sql` = 16 functions (identifiers, `updated_at`, scoping/board-hide/membership mirrors, trash/archive fan-outs, builtin statuses, `status_id` from the anchor, immutable `creem_subscription_id`).

## Patterns

### Electric shape proxies

One proxy per synced table in `routes/api/shapes/` (`createShapeRouteHandler`). **Every proxy pins a server-side `columns` allowlist clients cannot widen; new server-only columns go BEHIND it**. `users` pins exactly `id,name,email,image,created_at,updated_at`; `teams` its contract list (`comp_tier`, `agent_prompt` never); `issue-subscribers` drops reporter `email`, `actions` `body`, board-scoped shapes their scoping columns; a shape may FILTER on an excluded column. Hardening: `cache-control: private, no-store` + `vary: authorization, x-api-key, cookie`; bad token credentials → 401, never the anonymous clause; `buildWhereClause` SORTS id lists (= the shape identity); membership ids stay OUT of board-scoped clauses (`buildTeamScopedChildWhere`). A FIFO semaphore (`electric-proxy.ts`) bounds snapshot-class forwarding (REV-27), never live long-polls.

### Board trash (48h soft delete) + archive (EXP-500)

Owner-only `boards.delete` stamps `deleted_at`; **archive = same machinery minus the purge** (`archived_at` + a `board_archived_at` child mirror). Both vanish server-side: `boardVisible()` (`lib/board-visibility.ts`, every boards join) + the shapes' static `IS NULL` suffixes + trigger-fanned mirrors (REV2-103). Both keep the `(team_id, slug)` reservation; web-only cards restore them. The purge sweep (`lib/board-trash.ts`) keys on `deleted_at`.

### Custom issue statuses (EXP-314)

Per-TEAM rows, six fixed categories (backlog/unstarted/started/completed/cancelled/duplicate; contract: values + ONE `displayOrder` + `startedMax: 4`). 6 LOCKED builtins per team (`builtin_key` = legacy enum values, never renamed/recolored/deleted; trigger-seeded; `unstarted` EMPTY since EXP-685). Customs = name+color, member-managed (`statuses` router; delete reassigns). `issues.status` STAYS the dual-written **anchor** (`CATEGORY_ANCHOR` in `db-schema/src/domain.ts`; unstarted customs anchor to `backlog`). `statusId` = the precise row (`populate_issue_status_id` re-anchors enum-only writers). Resolution/colors/glyphs: `lib/team-statuses.ts` + `lib/status-icons.ts` (×4, lock-tested). Lists group by status ROW; duplicate: no customs, enum+`duplicateOfId` lockstep.

### Web plumbing

- **Collections** (`lib/collections.ts`): all use `columnMapper: snakeCamelMapper()`, else `useLiveQuery` `where` silently fails. `undefined` (not `false`) skips a query; `and()`/`or()` from `@tanstack/react-db`, never `&&`/`||`.
- **Auth guard**: `_authenticated.tsx` `beforeLoad` + `throw redirect()`.
- **MCP OAuth consent**: `lib/auth/mcp-authorize-guard.ts` pre-flights every `mcp/authorize` (forces `prompt=consent`) → `/auth/consent` team/board multi-select → `mcp_grants` BEFORE the code mints. `lib/mcp/scope.ts` confines OAuth tokens to the grant (none = nothing) and to `/api/mcp`; cookies + `expu_` keys = full access. Login resumes interrupted authorizes.
- **Issue UI**: issue detail = a route fed a live Electric `issue` (md+: board list beside it); title/description save on blur, other fields at once; `completedAt` auto-managed. **Activity (EXP-900)**: `issue_events` fold at read time ×4 (`lib/activity/fold.ts`, fixture-locked): one actor+field ≤10 min → NET change (a no-op vanishes), broken by other actors' events/comments; "Show all" = raw.
- **Issue lists (EXP-980)**: sub-issues nest under the parent, the ROOT decides group + position (`lib/issue-nesting.ts`); ONE blocks badge per row opens THE mini-graph (`lib/issue-graph.ts`; also the blocked-start dialog); ×4, fixture-locked. EXP-998/1057: web md+ + desktop swap it for the RAIL (`lib/issue-rail.ts`, `components/issue-rail.tsx`; desktop `issue_rail.rs`): dots only, hover = mini-graph, geometry `issue-graph-geometry.json` ×4; phones keep the badge; tree connectors = 3px corner, unbroken tee.
- **Issue search (EXP-892)**: ONE engine ×4, `lib/issue-search.ts` (fixture-locked), `useIssueSearchResults` in every picker; `lib/issue-search-sql.ts` behind `issues.search` AND MCP `issues_list.search`; desktop search = issues only; lists: top row preselected, ↑/↓, Enter/Tab pick.

## Environment Variables

**Root `.env.example` = CANONICAL**, `selfhost/.env.example` its subset, relays own `apps/*/.env.example`. Not obvious from those:

- `CLOUD_INSTANCE` = the opt-IN cloud marker (EXP-364): `'true'` = billing, plan limits, in-app widget, conversion tracking; unset = self-hosted, every FEATURE limit unlocked; `INITIAL_ADMIN_EMAILS` auto-promotes global admins.
- `AUTH_PASSWORD_ENABLED`/`AUTH_SIGNUP_ENABLED`: password login defaults true, public signup on in dev, OFF in production builds (`selfhost/docker-compose.yaml` re-defaults `true`). Auth posture = BUILD-derived (`lib/production-build.ts` `isProductionBuild`, REV-5), never runtime `NODE_ENV`. EXP-857: `AUTH_EMAIL_OTP_ENABLED` defaults on WITH a mail transport, `AUTH_PASSKEY_ENABLED` WITH an https base (rpID = host; Android origins from `ANDROID_APP_LINK_FINGERPRINTS`); login = ONE "Continue with …" list ×4; `mobile-oauth-start?provider=browser` = the native browser handoff.
- Mail: SES (`AWS_SES_REGION` + creds) OR `SMTP_*`, SES wins; neither = no mail.
- OIDC: `OIDC_PROVIDERS` (JSON array) = primary; `AUTH_OIDC_ENABLED`/`OIDC_*` = legacy, read only when unset.
- GitHub = ONE flow (SLOP-7): a Better Auth `github` provider on the App's OAuth client (`GITHUB_APP_CLIENT_ID/SECRET`; `GITHUB_LOGIN_ENABLED` adds the login button); members link it (`linkSocial`), installations + pushable repos list LIVE off their token (`integrations.github.*`), `repositories` store `installation_id`+`full_name`; `/integrations/github` = the one guided step (popup on web, browser from natives, the App's setup URL). Claim tables retired (SLOP-24). `GITHUB_POLLING=true` = outbound merge cron for NAT'd self-hosts.
- `STEER_RELAY_URL` unset = remote start/steer off; HS256 `STEER_RELAY_SECRET` must match the relay; BOTH relays need `TRUST_PROXY=true` behind a proxy.
- `CLIENT_MIN_VERSION_{ANDROID,IOS,DESKTOP,CLI}` gate with HTTP 426 + a blocking update screen (unset = off); MARKETING versions, never build numbers; `CLIENT_LATEST_VERSION_*` = informational.
- Widget rate limits (`WIDGET_RATE_LIMIT_*`, `WIDGET_CONFIG_RATE_LIMIT_*`, REV-25): the per-KEY ones = self-host-only; cloud = per-TEAM plan ceiling (`lib/widget/submit-limit.ts`).

## Coding sessions & Actions

### The launcher

A thin launcher (`coding::prepare`, desktop + CLI): the issue's repo (tRPC) → a session-gated JIT GitHub-App token → a worktree + `exp/<IDENTIFIER>` branch with ambient git auth from a repo-local credential helper (EXP-73) → the `/api/mcp` MCP config with the user's `expu_` apikey, NOT `.mcp.json` → the system-prompt append (`skill::system_append`, claude `--append-system-prompt`/codex `developer_instructions`, rebuilt on EVERY start/resume/shell): the playbook `crates/coding/src/skill.md` + the TEAM PROMPT (EXP-1025: server-only `teams.agent_prompt`, owner-edited in Settings → General web + IDE, cap `team.agentPromptMaxBytes`) → plan-first. The agent commits, pushes, opens its PR via MCP `open_pr` (the `X-Exp-Session-Id` MCP header names the run), unattended ones then `sessions_end`. Action and chat runs get their OWN worktree + branch (`exp/<slug>-<id8>` / `exp/chat-<id8>`); a repo-less chat runs in a scratch dir (EXP-739); agents never touch the trunk. Local deps: `git` + the claude CLI, never `gh`; Codex = a MANAGED download (EXP-1232 `coding::managed_codex`: one pinned upstream build, sha256 ×6, fetched from OpenAI's release to `{data_dir}/codex/<ver>/` on first sign-in or once a login is stored; the doctor never says Not installed; a non-default `codexPath` wins verbatim). EXP-1196 computer use = `crates/computer`: the cua driver LINKED (git pin, tools verbatim) behind ONE loopback MCP server, granted per run iff synced `launch_defaults.computerUse` (default OFF); runtime = the host binary as cua's private worker; unrestricted. Readiness = device-built synced `devices.doctor`, rendered ×5 off `fixtures/device-doctor.json`. Default branches resolve live (never `main`): `boards.default_branch` → team `default_branch_override` → GitHub.

### Agents & the engine

EXP-201/EXP-746; `coding/src/agent.rs`, ONE engine (`crates/engine`) runs every session as in-process ACP 2.x on the user's UNMODIFIED CLI (`adapters/` = claude stream-json + control protocol, codex app-server); ONE `SessionUpdate`→`ActivityEvent` mapper feeds relay + UI; never spoof `CLAUDE_CODE_ENTRYPOINT`; logins + shell tabs = the ONLY agent PTYs (EXP-773). Claude ALWAYS gets `--allow-dangerously-skip-permissions` + a `--settings` layer pinning auto mode OFF (EXP-1124); adapter auto-allows every `can_use_tool` but `ExitPlanMode`/`AskUserQuestion`; codex full-access; ultracode claude-only; plan mode claude+codex. `runs.json` records the ACP id + the agent's NATIVE id (re-read on `/clear`); a resume re-enters the RECORDED run; a repo-less run purges WHOLE at end; `.exp-agents` gates worktree reuse. Doctor gates the SELECTED agent (git always) + ACP + the launch's LOGIN (EXP-1138: runnable = ANY signed-in login; an unnamed launch takes the LAST USED one); pickers list only runnable agents. Nav (EXP-870/923, desktop+web ≥md): web: the rail never leaves; desktop (EXP-1192): rail never folds, starts on Agent (first entry), in-card second sidebars: Inbox always, a Reviews detail, Agent history (`second_sidebar_for`); issue + run = ONE top tab (Issue|Run); live own runs = the rail's Running section, NEVER a tab; session lists = the ACTIVE team (phones: ALL, team bands, EXP-1186), the team picker's dot = live runs in OTHER teams, amber = needs input (EXP-1075); nested lists ×4 draw `tree-guides` (EXP-965); bottom bar = terminals. Phones (EXP-893/1150/1152): ONE Work screen, Issue|Run|Changes|Results = PAGER tabs, Changes wears `+N −M` once files are known (`lib/work-faces.ts` ×3; Results = `coding_sessions.results` by topic = the PR body (`lib/pr-body-from-results.ts`, EXP-1154), MCP `sessions_results`; EXP-1172: MCP `sessions_show` = an early inline picture under its transcript call ×4, folded under Earlier on Results), Merge = the WHITE bottom capsule ×4 (EXP-1154: in the bar on Changes/Results, a bar circle on Issue/Run), bar `[context][capsule][start]`, Stop/Resume top-right on Run only; list rows carry no buttons ×4. Steering: plan/model/effort launch-time (EXP-790); `config_state.options` carries ONLY `model` (EXP-877, `/model <alias>`); `config_state`/`usage`/`rate_limit`/`queue`/`task_list`/`context_layout` = latest-wins STATE (EXP-1051: `context_layout` = per-run segments, `base` from the first prefix, the rest bytes÷4; `lib/context-layout` ×4 fixture-locked; the usage popover's top row + bar; switch refusals = tooltips/toasts); EXP-861/873: `queue` bar ×4 = UNREAD messages (mid-turn sent at once, `sent` until claude's replay lands the row; mid-compaction HELD, `unqueue` revokes held only; Stop returns all); `tool` carries `id`+contract `toolKind`, `tool_update` patches it, a collapsed run's caption = contract `toolGroupSummary` ×4 (EXP-785/786); contract `expToolDisplay` labels Exponential MCP calls (EXP-846); `turn` (started|ended) drives the spinner + `agent_busy`; a pending question/plan answers IN its card (EXP-820); `/` = contract `steerCommands` ∪ agent commands. Narration carries `messageId` (fragments merge) + on user rows `subagentId` (subagent view only). Transcripts live ONLY on the device (`{data_dir}/journal/<id>.jsonl` + `.diff.json`, pruned per `sessionRetentionDays`); a viewer join with no live room → `history_request` → `activity_synced`, else `device_offline`/`history_unavailable`. `subagent.title` = the spawning call's description; a read-only Plan chip mirrors `config_state.currentMode`; ending verb = Stop ×4. **Accounts (EXP-849):** per-account config dirs (`agent_profiles`: `CLAUDE_CONFIG_DIR`/`CODEX_HOME`); credentials NEVER copied, synced or brokered, transcripts may be. `devices.agent_accounts`/`agent_usage` jsonb ride the heartbeat READ-ONLY; `health` (ok|needs_relogin|signed_out|unknown) = usage probe's 401; keep-alive: codex `account/read` 6-hourly; claude = OUR refresh-token POST under the CLI's `.oauth_refresh.lock`, written back ONLY to its source store (EXP-852/909). Accounts = decision surface; Devices = repair (never `codex logout` on the ambient login). Remote start/resume carry `account` (EXP-906); a mid-run switch = claude-only `startSession({resumeSessionId, account})` on an idle turn. EXP-1005 auto-rotate (device toggle, default ON, claude only): every launch picks the profile with the most headroom off the usage cache (`coding::prepare`); a walled idle run is resumed on another one by its host after a FORCED probe (`account_rotation`: cooldown + cap, `setBlocked.handled` mutes the owner push); codex waits. Polls pin past a reset only when EVERY window is maxed, ≤6h (EXP-817/964). MCP servers = ONE system (EXP-792): server-only TEAM rows (never repo `.mcp.json`); each MEMBER connects once, the SERVER holds their secrets (`mcp_credentials`, AES-GCM off `BETTER_AUTH_SECRET`, OAuth server-side), `resolveForLaunch` (EXP-1140) hands the LAUNCHER (never the agent's `kind: agent` key) ONLY its live row's pick (`X-Exp-Session-Id` → server-only `mcp_server_ids`) as `${VAR}` env refs, as the CALLER = device OWNER (a hosted run spends its tokens like its `expu_` key); unpicked/unconnected skip.

### Runs (SLOP-3)

A run = ONE `coding_sessions` row (one device, agent, worktree, branch) with a subject: none (chat), 1 issue, 2+ issues (a **batch**) or an action. The Agent page composer = the ONE launcher ×4 (EXP-825; issue chips OR one action chip; free text = `prompt`; ONE repo per run); `codingSessions.start` takes exactly one of issueId/teamId. A batch = ONE session on `exp/batch-<id8>`, its own runbook (`batch_prompt.rs`), ONE combined PR via `pr_open{issueIds, head}` (same repo enforced); a PR resolves to ALL linked issues by exact `pr_url`. EXP-626: `pr_open{repositoryId, head}` / `pr_merge{repositoryId, prNumber}` = an issue-less PR (nothing linked/notified; Reviews → Agent runs). Every form stamps the caller's row, merges from the run go through `codingSessions.mergePr`. NO server-side orchestration: the agent orchestrates itself (subagents, workflows, ultracode) or `sessions_start`s runs (child gets `parent_session_id`; `ask_parent` takes `parent`|`user`). Follow-up runs = playbook only: one child run per filed follow-up, based on the parent's branch (base in `prompt`; the child's `pr_open{base}` writes `blocks` lower→upper when `base` is another issue's open PR branch); caps 3 children / depth 3 / 10 per tree; `no follow-up runs` turns it off. Trees merge ROOT FIRST: the root's merge retargets every open child to the default branch (EXP-324 `retargetChildrenOfMergedPr`, 422-tolerant), then each child merges; never a mid-tree PR alone. `mergePullRequestSmart` = squash; merge-async + poll only on GitHub's stacked-PR refusal. A blocked issue's start asks Cancel · Start anyway · Stacked PR ×4 (`blocked-start.json`: the dependency LINE starts bottom-up from the nearest open PR, each run starting the next as a follow-up; PROMPT TEXT only). A Merge on a stack member asks Merge stack · Merge this ×4 (`stack-merge-choice.json`); `mergePr`/`pr_merge` `mergeStack` = the open chain merged bottom-up THROUGH the issue, plain squashes with the EXP-324 retarget between. Synced `issues.pr_base_branch` (pr_open/retarget/webhook) feeds the related-work badge ×4 (`lib/pr-graph.ts`: blockers, batch, stack) and the merge guard (`stackedOnOpenPr`: a PR based on another issue's OPEN PR merges after it). Reviews = a FLAT list deduped by `pr_url`.

### Actions (EXP-253)

`actions` rows (per team, markdown `body` ≤64KB, optional `repository_id` SET NULL + curated `icon`, ≤10 typed PICK inputs `repo|board|pr|icon`; EXP-825: free text = `steer.startSession({prompt})` → an "Additional instructions" section (hint `actions.prompt_placeholder`), images via `/api/teams/$teamId/session-files` + `start({attachmentIds})`, cap `start-prompt` gates the Chat/Create builtins): tRPC CRUD (member list/get, owner writes) + 4 MCP tools + the body-less shape. **Triggers (SLOP-2) live ON the action**: synced `actions.triggers[]`, each = a runner (`id`, `deviceId`, `enabled`, optional pins) + `schedule` | `event` `{source, event, filters}`; LOCAL-ONLY (the bound device self-starts them; `automationId` = the trigger id); owner-only whole-array `actions.update({triggers})` (+MCP); enabled ⇒ every input optional; unsharing a device pauses its triggers; the action PAGE ×4 = Prompt · Triggers · Runs (tabs on phones). `automations` = a server-written MIRROR for old clients. Runs execute LOCALLY (per-run worktree, PR worktree or scratch dir), never with server-side secrets. Remote start = `steer.startSession({actionId, deviceId, …})`; `resumeSessionId` + cap `resume-run` relaunch an ended run. Clients ×4 edit actions in full.

Virtual builtins = client-CONSTRUCTED, never DB rows, **byte-identical** strings (`lib/builtin-actions.ts`, `api::actions::builtin_*`, `ActionsApi` ×2); MCP's list appends the three listed ones, tRPC's does not. They pin FIRST by the `builtin` flag, every builtin start carries `teamId`, `get/update/delete` reject the reserved ids. **"Create action"** (`builtin:create-action`, inputs `[repo?, icon?]`, request = `prompt`) = the creator run, scratch cwd; hidden Chat (`[repo?]`, `prompt` required) = the composer with no subject. **"Fix merge conflicts"** (`builtin:fix-conflicts`, required `pr` = an open PR's representative issue id, batch PRs deduped) runs in that PR branch's WORKTREE (rebase, resolve, force-push, `pr_merge`); Reviews offers it on a failed merge. **"Tidy up"** (`builtin:tidy-up`, `[board?, repo?]`) = non-destructive board cleanup (duplicates, existing labels, relations); builtins carry no triggers (a migrated team's REAL `Tidy up` row hides the builtin). Prompts = shipped constants (`body` empty). Suggestion seeds (`action-suggestions.ts`, ×4) prefill the creator run's `text`.

### Desktop IDE & mobile

Desktop IDE = master-only + autopull (no branch switch; changes land via PRs or Source Control's CONFIRMED commit-and-push; Discard-and-reset; `trunk_sync` badge + banner). First-run wizard ×4 (`lib/auth/onboarding.ts`, server-gated): team → board → invite → devices; a joiner owning NO device gets the devices step after the accept (EXP-1169). ONE device-setup block ×4 = that step, Add device, readiness `set_up_server`. Lists ×4 = a filled group band over FLAT hairline-divided rows (EXP-818/1076, settings too); `GlassGroup` = form fields. EXP-1175: the Run face opens as the THREAD ×4: a status row (`runRowCaption` + `lastToolLine`, `run-row.json`) over `sessionThread` (results in publish order, Summary = the reply, cards never hide); Show work = the transcript in place, a per-USER pref.

## Billing (per-seat, Creem—cloud only)

Subscriptions bind to a TEAM (`creem_subscriptions.team_id` + `seats`; `billing.createSeatCheckout`, Creem `units` = seats), not the purchaser (REV2-55, `lib/billing/billing-handover.ts`): `reference_id` nullable/set-null; account deletion NEVER blocked by billing, it only cancels a SOLO team's subscription it destroys; team deletes REFUSE a live subscription (`PRECONDITION_FAILED`; a period-end cancellation passes), natives point at web. ONE subscription per team: `createSeatCheckout` refuses duplicates; `billing.updateSeats`/`changePlan` mutate the EXISTING subscription with `update_behavior: proration-charge-immediately`. Free = 3 seats, 250MB, 1 widget; **Team** = the ONE paid tier, €15/seat/mo or €12 yearly: 10GB, unlimited widgets (`PlanTier` free|team|unlimited). Boards/repos/coding sessions, push + steer = never plan-gated; over-seat teams only block invites.

**Limits exist only when `CLOUD_INSTANCE=true`**. Enterprise Support: no published pricing (EXP-218), marketing routes to `/contact/`. Self-host's one limit: no mobile push (store apps embed Firebase).

## Feedback widget (SLOP-4: ONE path, a submission IS an issue)

`packages/widget` (Preact shadow root + snapdom) builds an async IIFE loader + lazy panel into `apps/web/public/widget/v1/`; API = `window.ExponentialWidget`.

Server-only `widget_configs` (public `expw_` key + domain allowlist, `board_id` NOT NULL) + `widget_submissions` (`issue_id` NOT NULL = the "Reported via widget" card ×4 + the reporter identity); public CORS routes `/api/widget/{config,submit}` (origin/rate-limit/honeypot in `lib/widget/`). ONE form: `message` (+ optional pictures, name, email per `form_config` toggles, `labelIds` ≤10, `customFields`, `theme`, `launcher`); ONE capture button (getDisplayMedia desktop, snapDOM mobile) + an Off/3s/5s hold segment. Submit = issue + attachments (null `uploader_id`) + submission row in ONE transaction, `source='widget'`, `creator_id` NULL; reporter text is UNTRUSTED and escaped ONCE server-side into literal GFM (`lib/reporter-text.ts`, fixture-locked; the resolvers never run on it). **The conversation = comments**: a member comment with `audience=reporter` is emailed (`email_delivery_id` audit) with the magic link `/support/<token>` (`lib/reporter/token.ts`, HMAC over the ISSUE id, minted only when a reporter email exists); the reporter page reads the issue's reporter-audience comments + their own pictures and replies as `source=reporter` comments (a reply on a completed issue reopens it → `backlog`); notify = issue-scoped `reporter_reply`; the "Reply to reporter" toggle = the composer's leading-row pill ×4, only with a reporter email. Rate limits = in-process token buckets; the ONLY cloud upsell = the Widget settings' usage bar (owner-only, web + IDE read-only). The in-app widget = headless behind the sidebar "Report bug" button (cloud-only; key in `lib/runtime-config.ts`, `FEEDBACK_WIDGET_KEY` overrides).

## Conversion tracking (EXP-362, cloud only)

`lib/conversion/` + `adminConversions` router, no-ops unless `CLOUD_INSTANCE=true`. COOKIELESS: visitors = daily-rotating salted HMAC of ip+ua; attribution = URL params (`ref`/`utm_*`). `events.ts` owns the vocabulary; idempotency = partial unique indexes + `onConflictDoNothing`.

## Style Conventions

- Template literals; functional components only
- shadcn/ui from `@exp/ui` ALWAYS over raw `<input>`/`<button>`/`<textarea>`/`<label>`; icons by CONCEPT

## Agent context budget (EXP-353/EXP-637)

Keep this file under 40k chars. MCP clients DEFER tool defs behind tool search (`_meta["anthropic/alwaysLoad"]` opts in): the always-loaded set == `lib/mcp/always-load.ts`, <10k serialized, whole surface <60k, per-tool <1.8k, `MCP_SERVER_INSTRUCTIONS` <2k with a self-contained first 512; gated by `lib/mcp/context-budget.test.ts`. `exponential_issues_list` defaults to OPEN work, limit 50/max 1000, descriptions cut at 200 chars. **Compress, never append**: a rule over its rationale, a citation over a list.
