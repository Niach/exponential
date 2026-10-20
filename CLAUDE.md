# Exponential

Real-time issue tracker.

## Tech Stack

TanStack Start (React 19, TanStack Router/React DB) · PostgreSQL 17 via Drizzle (`snake_case` casing) · ElectricSQL (shape proxy pattern, `@tanstack/electric-db-collection`) · Better Auth (email/password, email OTP, passkeys, Google/Apple, OIDC `genericOAuth`, session-based, `tanstackStartCookies`) · tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync) · shadcn/ui on Tailwind v4 (OKLCH zinc, dark forced via `html.dark`) · bun. Dev infra (compose): Postgres:54321, Electric:30000, Garage:3900, Caddy:3000, steer-relay:4002.

## Monorepo Layout

```
apps/
├── web/ # TanStack Start app
├── push-relay/ # Hono/Bun
├── steer-relay/# steer WS hub (Bun)
├── marketing/ # Vite + React; Remotion hero
├── ios/ # SwiftUI (Tuist + GRDB; ExpCore/ExpUI)
├── android/ # Jetpack Compose
├── styleguide/ # Shot gallery + app-extension islands
├── ui-site/ # ui.exponential.at (SDK site)
└── desktop/ # gpui IDE; crates/cli = the `exponential` daemon (EXP-403); crates/exponential-ui* = the SDK
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
├── site-shell/ # shared site shell + prerender
├── exponential-ui{,-react,-swift,-compose}/ # the SDK (VAPP-84)
├── shots/ # capture pipeline
└── tsconfig/
docs/ # third-party-licences.md + licences/
shots/ # COMMITTED webp store, <view>/<platform>.webp
docker-compose.yaml # DEV backend stack
selfhost/ # Pull-an-image compose; INSTALL.md = the runbook
Dockerfile{,.push-relay,.steer-relay} # build context = repo root
```

Workspace names: `@exp/<dir>`; `apps/desktop` = Cargo.

**Dead, never reintroduce:** releases + footage/fixtures (EXP-106), `@exp/video`, workspace/project vocabulary + `/w/`/`/projects/` URLs (EXP-180), board types, `agent_runs` + agent-core + the companion daemon + `isAgent`, the `assigned-issues` shape, `run_configs`, `claude_task` (EXP-259), `isProtected` boards + dogfood cases (EXP-364), due-date time-of-day (REV2-49), `SELF_HOSTED` (now `CLOUD_INSTANCE`), `GOOGLE_CALENDAR_ENABLED`/`DOGFOOD_REPO`, the builtin `todo` status (EXP-685), skip-permissions (EXP-690), PTY coding + `start_in_terminal` (EXP-773), standalone automations, the pi agent + external ACP agents (EXP-849/862), issue filtering (EXP-862), SLOP-3: workflows (EXP-978), stacked runs + the stack plan + `pr_stack_number` (EXP-897); SLOP-4: the helpdesk (`support_*`, `helpdesk` router, `support_reply`, widget `modes`); EXP-1291: `coding_sessions`/`threads`/`thread_items`/`session_attachments`, the issue PR columns, batch/chat/action session types, builtin actions, action triggers, Reviews, the Agent page history, work faces + Show work, right sidebars, `steer.startSession`/`codingSessions.start`, `resumed_from_id`/`started_reason`/`ended_by`/`merged_own_pr`, merge-ends-session; EXP-1280: the CLIs' native plan modes (`ExitPlanMode` cards, EXP-954 plan-file recovery, codex collaboration).

## Product Invariants

**Vocabulary (EXP-180):** the product says **team** and **board** EVERYWHERE: copy, URLs (`/t/$teamSlug/boards/$boardSlug/issues/$id`), identifiers, DB, routers, shapes, MCP tools; copy says **session**, never "run" (code may). Boards have no types; `repository_id` NULLABLE; coding gates on repo PRESENCE.

**Client parity:** all four clients sync the same 26 Electric shapes (`routes/api/shapes/`). `pins` (EXP-778) + `issue_drafts` (EXP-878) + `session_reads` (EXP-1323) = per-user static, never trash-scoped: a row renders only when its target/board resolves; draft attachments (`draft_id`, issue_id NULL) never sync (`issueDrafts.listAttachments`; eager upload `/api/issue-drafts/{id}/files`), reparent on `issues.create({draftId})`, which stamps synced `issues.draft_id` (EXP-1231: an open page elsewhere lands on the issue, consumed id upsert = CONFLICT). The `actions` shape EXCLUDES the `body` (tRPC `actions.get`); `sessions` excludes `instructions`/`host_user_id`/`mcp_server_ids`. `devices` + `device_worktrees` (EXP-481) sync `user_id = me OR shared_team_ids && (member teams)` via trigger mirrors; devices rows = SERVER-AUTHORITATIVE (persisted `launch_defaults`; online = `last_seen_at` within `contract.device.onlineWindowSeconds`). `repositories`, `user_notification_prefs`, `email_deliveries`, `conversion_events`, `device_commands`, `passkeys` and the widget tables = **server-only (tRPC), never synced**.

**Nothing is anonymously readable:** every shape = member-only (anonymous → impossible-match sentinel); no public tRPC; attachment reads need membership. ONLY anonymous endpoints: widget (`/api/widget/*`), the reporter magic-link page (`/api/support/{thread,reply,attachment}` + `/support/$token`, SLOP-4), invites, auth, `/about` + `/NOTICES.txt`, MCP OAuth CIMD + state-gated callback (`/api/mcp-oauth/{client.json,callback}`, EXP-792), `/api/{guide-uploads,attachment-uploads}/$token` (HMAC uploads, EXP-879/929), `/api/cli/install-token/redeem` (EXP-1111). Board-scoped shapes = TEAM-scoped with a STATIC trash predicate (REV2-5): `team_id IN (member teams) AND board_deleted_at IS NULL` (trigger-fanned mirror on every issue child) and **shape identities rotate ONLY on membership changes**. Issue-less notifications keep NULL `board_deleted_at` and always sync; the session tree uses the visibility rule instead (Threads v2); the notifications shape = static per user (`user_id = me AND board_deleted_at IS NULL`; fan-out filters recipients at delivery, delivered rows outlive membership); issue-less rows (`agent_message`, `session_blocked`) carry a synced `team_id`.

**Permissions are membership-only:** `lib/auth/access.ts` `resolveTeamAccess` = the single authority (capabilities `read`/`comment`/`create_issue`/`mutate_resources`), mirrored by `use-team-permissions.ts` + the natives; every member moderates and answers reporters, owners own the destructive/settings surface (owner-only controls HIDDEN from non-owners; destructive native actions confirm). Signups get **no** team: first-run = "Create a team" (`teams.create`; creator = owner; cap `FREE_OWNED_TEAMS_CAP`) or "Join a team" (`teamInvites.accept` stamps `onboardingCompletedAt`; invites may carry a synced `email`). `teams.getDefault` = the non-creating resolver. An owner may delete ANY team incl. the last. **Billing and the admin console are web-only. Coding sessions run only on the desktop app and the headless `exponential` CLI daemon** (EXP-403; device-code login at `/auth/device`), both publishing scrubbed activity to the steer relay; a session on a team-SHARED device is visible/answerable/stoppable by every member, any other only by its starter (Threads v2).

The app = **noindex** (`__root.tsx` meta + `X-Robots-Tag`); marketing owns the indexed surface (`seo.ts` `PAGES`).

## Shared Contracts

**Icons (EXP-273/317):** ONE icon set, Lucide, byte-identical ×4, generated from `packages/icons/icons.json` into COMMITTED per-platform outputs (web: `@exp/ui` `icons.generated.ts`). Change an icon in `icons.json` + regenerate; never hand-edit generated files nor add per-platform glyph maps. `pickable` (contract `boardIcon`) + `devicePickable` (`deviceIcon`) = **APPEND-ONLY**: reordering orphans rows. Multi-client surfaces name a CONCEPT (`conceptIcon(\`nav-search\`)`, `registry::NAV_SEARCH`, `ExpIcons.navSearch`), never a raw glyph; iOS renders via `AppIcon`, never `Image(systemName:)`; `icons.test.ts` gates it. Desktop assets hold hand-kept BRAND marks; MCP catalog marks = icons.json `brand` (rows = contract `mcpCatalog`).

**Enums:** canonical values live in `packages/domain-contract/contract.json`; a `db-schema/src/domain.ts` change = update `contract.json` + regenerate.

**Exponential UI (VAPP-84):** `packages/exponential-ui` = catalog + runtime themes + HOST API (`catalog/host.json` ×4: TS `ExponentialHost`, Rust `host`) + conformance (`conformance/`, every renderer in `exponential-ui.yml`) + `release/` (`ui-v*` tags); fixtures = the contract ×4; `exponential-ui-react` = React renderer + the shadcn primitives `@exp/ui` re-exports; `crates/exponential-ui{,-ffi,-gpui}` = Rust core + UniFFI facade + gpui painter; natives: `packages/exponential-ui-{swift,compose}` (+ primitives the apps wrap); Example apps = shots lane `package` (`fixtures/specimens.json`); `samples/exponential-ui` = 4 outside hosts. The SDK imports nothing from the app (`lib/exponential-ui-host.tsx` = a host like any other); extension + packages: `packages/ui/exponential-ui`.

**Markdown:** `issues.description` + `comments.body` = plain `text` GFM, one interchange: web TipTap, iOS cmark-gfm, desktop comrak + vendored WYSIWYG `crates/gpui-markdown-editor`, Android commonmark-java. The round-trippable feature set IS `CONTRACT_FIXTURES` (byte-locked ×4). No underline, no slash commands. **Tables** (EXP-726): canonical `| a | b |`/`| --- |` rows, `:---` alignment, one inline paragraph per cell, `\|` escape; top-level only, natives HOIST nested ones out (EXP-728). **Mentions** = plain `@<email>` (`lib/integrations/mentions.ts`; fires `issue_mention`, auto-subscribes); **issue mentions** = plain `#<IDENTIFIER>` (`lib/issue-refs.ts`, `#` autocomplete ×4; a pill renders only for a synced same-team issue; a ref auto-links both as `related`, source `reference`). Images stored relative `![alt](/api/attachments/{id})` (server canonicalizes; attachments carry probed `width`/`height`; `as_file` = a paperclip upload, never inlined, listed in Files). **Inline media (EXP-824):** `video/*`/`audio/*` rows embed as a PLAIN LINK alone in a paragraph, `[clip.mp4](/api/attachments/{id})`, upgraded to a player from the synced row (`duration_ms`, poster); normalisation = CLIENT-side (H.264+AAC MP4, mobile 720p); the server probes MP4/MOV headers only. **Emoji** (EXP-551) = unicode, never `:shortcode:`; picker + `:` typeahead data generated ONCE by `packages/emoji` (drift-gated).

## Commands

```bash
bun install
bun run backend  # docker compose up -d + dev server (:3000 via Caddy)
bun run ios / ios:test  # tuist+Xcode / ExpCore+ExpUI suites
bun run android  # productionDebug install + launch
bun dev  # web dev server (:5173)
bun run {dev,build}:marketing / movie:{studio,render,poster,still}
bun run {dev,start,test}:push-relay / {dev,start,test}:steer-relay  # :4001 / :4002
bun run build  # widget FIRST, then web + marketing
bun run build:web / build:widget / test:widget / dev:widget (watch, /widget/v1/demo.html)
bun run typecheck / test / test:e2e  # web
bun run migrate / migrate:generate / psql / backend:{up,down,clear} / storage:init
bun run dev:desktop / {build,appimage,macapp,test}:desktop  # gpui IDE vs the local backend
bun run --filter @exp/{domain-contract,design-tokens,icons} generate
cd apps/web && bun run seed:screenshots  # demo data, then `bun run shots`
```

Workspace scripts: `bun --filter @exp/web <script>`; `cargo` in `apps/desktop`. Never `lint`/`format`.

## Deploys

Coolify (`coolify.home.straehhuber.com`, Hetzner, **home-LAN-only**): `coolify deploy uuid <uuid>` after a green Actions run. `build-web.yml` publishes `ghcr.io/niach/exponential-web` on master pushes + `v*` tags; ONE image for cloud, staging and self-host; licence rules: `docs/third-party-licences.md` (gated). Native releases = tags `{android,desktop,cli,ios}-v*` → `build-*.yml` (desktop: prod + staging × 3 OSes, `make_latest`, self-update `crates/updater`; cli: bare binaries, `EXP_INSTANCE`).

**The operations runbook lives OUTSIDE the repo.**

Every user-facing release PREPENDS a `ChangelogEntry` to `lib/changelog.ts` (gated; mirror `crates/ui/src/changelog.rs`).

After schema changes: `bun run migrate:generate && bun run migrate`; custom triggers auto-apply at boot (`applyCustomSql`).

## Web App Structure (`apps/web/src/`)

The shadcn set, theme `styles.css`, `cn`, icon registry and shared primitives (`IssueChip`, `StatusGlyph`, `Pill`, `SessionRow` small|big, `PrRow`/`StackRail`, toasts—`Toaster` + `toast-stack.json` ×4; THE `Menu` = `menu.tsx` MenuEntry[] trigger|pointer|sheet, `menuProps(kind,id)` = context menus; `Picker` = the only picker) = `@exp/ui` (`packages/ui/src`; `@exp/ui/island` = shadow-root islands for styleguide + marketing); app compositions stay flat in `components/` (`agent-session.tsx` = the steer view). `lib/trpc/` = one file per router (`routes/api/trpc/$.ts` lists them). `lib/auth/membership.ts` = data lookups; `access.ts` = authorization. `lib/notification-email-policy.ts`/`-digest.ts`: push fires on create, email = a digest of still-unread (daily at the user's hour, hourly legacy; atomic `emailed_at` claim; `server-bun.ts` schedules). An agent's `@email` in a conversation message = an issue-less `agent_message` row + push (prefs `allow_agent_messages=false` BLOCKS other members' agents); a session's usage wall sends `session_blocked` to its starter (synced `session_id`; opens the session ×4). Team routes under `t/$teamSlug/`: inbox `?tab=my-issues` = a TAB, not a route (`?tab=drafts` phone-only; `drafts` route + sidebar entry only while drafts exist); `drafts/$draftId` = New issue AS the detail in draft mode, NO create dialog ×4 (EXP-1170: openers mint the id, ONE coalesced `issueDrafts.upsert`, copy `issue-draft.json`); HOME = the composer page + the dawn (#FEED-100); Threads = the list (Needs you · Running · Recent) with the composer on top; a thread = header tabs + Conversation + THE composer box; a session card opens the raw session view; an agent = the same Conversation as a channel. Entry: `router.tsx`, `start.tsx` (`defaultSsr: false`), `server{,-bun}.ts`.

## Database

`@exp/db-schema` = authoritative—never mirror it here.

### Conventions

Better Auth user IDs (so all user FKs) = `text`; app tables use UUID PKs and timezone `created_at`/`updated_at`; sort orders = `doublePrecision`.

### Non-obvious fields

Issues DUAL-WRITE `status` (the builtin ANCHOR enum) and `statusId` (nullable FK `issue_statuses` SET NULL, the precise per-team row); `creatorId` NULLABLE (widget issues have none), `source` = `user`/`widget`; comments thread ONE level (`parent_id`, replies re-parent to the root) + `source` user|mcp|reporter (`mcp` stamped from `ctx.viaMcp`, "via MCP" ×4, EXP-741; `reporter` = a widget reporter's words, `author_id` NULL, "reporter" caption ×4) + `audience` team|reporter (SLOP-4: `reporter` shows on the reporter page and, from a member, is emailed to the reporter; `fixtures/reporter-reply.json` = the ×4 spec + copy); `duplicateOfId` pairs with status `duplicate` and dual-writes an `issue_relations` `duplicate` row (EXP-736: canonical-direction rows `blocks`/`parent`/`duplicate`/`related`, `source` user|reference, member-managed via `relations` + 2 MCP tools, events on both sides; EXP-980: `blocks`/`parent` refuse TRANSITIVE cycles, `lib/relation-cycles.ts`); PRs live in `pull_requests` + `pull_request_issues` (a thread has n; an issue may have several), never on `issues`. The session tree: see Threads v2. Teams carry a server-only `compTier` plus synced PR automation (`prOpened*`/`prMerged*` `StatusId` + `Automation`: nullable FKs SET NULL, NULL = builtin default target, `*Automation=false` = do nothing; member-gated `statuses.setPrAutomation`; UI web + desktop).

### Enum behavior

`issue_status`: `pr_open` flips linked issues (`pull_request_issues`) to the team's PR-open target (default `in_review`), merge to the PR-merge target (default `done`); a merge never ends a session. `session_status` queued → running ⇄ idle → ended: `queued` = the device has not claimed it, `idle` = between turns (a message resumes the SAME row), `ended` = explicit End or the agent process gone (agents never end); `busy` (device-written per turn edge) = the ONLY list-spinner input ×4; `needs_input` (a question/plan card waits) feeds Needs you; `blocked` (EXP-804 jsonb, a walled session stays live). `exponential_sessions_end` = the LAST call of sessions nobody started by hand (its summary = the final message); a person-started one calls it only when asked.

### Custom triggers

`db/out/custom/0001_triggers.sql` = the trigger functions (identifiers, `updated_at`, scoping/trash/archive/membership mirrors incl. `session_issues`/`pull_request_issues`/`messages`, builtin statuses, `status_id`).

## Patterns

### Electric shape proxies

One proxy per synced table in `routes/api/shapes/`. **Every proxy pins a server-side `columns` allowlist clients cannot widen; new server-only columns go BEHIND it**. `users` pins `id,name,email,image,created_at,updated_at`; `teams` its contract list (`comp_tier`, `agent_prompt` never); `issue-subscribers` drops reporter `email`, `actions` `body`, board-scoped shapes their scoping columns; a shape may FILTER on an excluded column. Hardening: `cache-control: private, no-store` + `vary: authorization, x-api-key, cookie`; bad token credentials → 401, never the anonymous clause; `buildWhereClause` SORTS id lists (= the shape identity); membership ids stay OUT of board-scoped clauses (`buildTeamScopedChildWhere`). A FIFO semaphore (`electric-proxy.ts`) bounds snapshot-class forwarding (REV-27), never long-polls.

### Board trash (48h soft delete) + archive (EXP-500)

Owner-only `boards.delete` stamps `deleted_at`; **archive = same machinery minus the purge** (`archived_at` + a `board_archived_at` child mirror). Both vanish server-side: `boardVisible()` (`lib/board-visibility.ts`, every boards join) + the shapes' static `IS NULL` suffixes + trigger-fanned mirrors (REV2-103). Both keep the `(team_id, slug)` reservation; web-only cards restore them. The purge sweep (`lib/board-trash.ts`) keys on `deleted_at`.

### Custom issue statuses (EXP-314)

Per-TEAM rows, six fixed categories (backlog/unstarted/started/completed/cancelled/duplicate; contract: values + ONE `displayOrder` + `startedMax: 4`). 6 LOCKED builtins per team (`builtin_key` = legacy enum values, never renamed/recolored/deleted; trigger-seeded; `unstarted` EMPTY since EXP-685). Customs = name+color, member-managed (`statuses` router; delete reassigns). `issues.status` STAYS the dual-written **anchor** (`CATEGORY_ANCHOR` in `db-schema/src/domain.ts`; unstarted customs anchor to `backlog`). `statusId` = the precise row (`populate_issue_status_id` re-anchors enum-only writers). Resolution/colors/glyphs: `lib/team-statuses.ts` + `lib/status-icons.ts` (×4, lock-tested). Lists group by status ROW; duplicate: no customs, enum+`duplicateOfId` lockstep.

### Web plumbing

- **Collections** (`lib/collections.ts`): all use `columnMapper: snakeCamelMapper()`, else `useLiveQuery` `where` silently fails. `undefined` (not `false`) skips a query; `and()`/`or()` from `@tanstack/react-db`, never `&&`/`||`.
- **Auth guard**: `_authenticated.tsx` `beforeLoad` + `throw redirect()`.
- **MCP OAuth consent**: `lib/auth/mcp-authorize-guard.ts` pre-flights every `mcp/authorize` (forces `prompt=consent`) → `/auth/consent` team/board multi-select → `mcp_grants` BEFORE the code mints. `lib/mcp/scope.ts` confines OAuth tokens + scoped `expu_` keys (FEED-76) to the grant (none = nothing) and to `/api/mcp`; cookies + plain keys = full. Login resumes interrupted authorizes.
- **Issue UI**: issue detail = a route fed a live Electric `issue` (ONE Start button, no face toggle); title/description save on blur, other fields at once; `completedAt` auto-managed. **Activity (EXP-900)**: `issue_events` fold at read time ×4 (`lib/activity/fold.ts`, fixture-locked): one actor+field ≤10 min → NET change (a no-op vanishes), broken by other actors' events/comments; "Show all" = raw.
- **Issue lists (EXP-980)**: sub-issues nest under the parent, the ROOT decides group + position (`lib/issue-nesting.ts`); ONE blocks badge per row opens THE mini-graph (`lib/issue-graph.ts`; also the blocked-start dialog); ×4, fixture-locked. EXP-998/1057: web md+ + desktop swap it for the RAIL (`lib/issue-rail.ts`): dots only, hover = mini-graph, geometry `issue-graph-geometry.json` ×4; phones keep the badge; tree connectors = 3px corner tee.
- **Issue search (EXP-892)**: ONE engine ×4, `lib/issue-search.ts` (fixture-locked), `useIssueSearchResults` in every picker; `lib/issue-search-sql.ts` behind `issues.search` AND MCP `issues_list.search`; desktop search = issues only; lists: top row preselected, ↑/↓, Enter/Tab pick. `#` + pickers + the "+" submenus = THE search list ×4 (`search-list.json`, no cap).

## Environment Variables

**Root `.env.example` = CANONICAL**, `selfhost/.env.example` its subset, relays own `apps/*/.env.example`. Not obvious there:

- `CLOUD_INSTANCE` = the opt-IN cloud marker (EXP-364): `'true'` = billing, plan limits, in-app widget, conversion tracking; unset = self-hosted, every FEATURE limit unlocked; `INITIAL_ADMIN_EMAILS` auto-promotes global admins.
- `AUTH_PASSWORD_ENABLED`/`AUTH_SIGNUP_ENABLED`: password login defaults true, public signup on in dev, OFF in production builds (`selfhost/docker-compose.yaml` re-defaults `true`). Auth posture = BUILD-derived (`lib/production-build.ts` `isProductionBuild`, REV-5), never runtime `NODE_ENV`. EXP-857: `AUTH_EMAIL_OTP_ENABLED` defaults on WITH a mail transport, `AUTH_PASSKEY_ENABLED` WITH an https base (rpID = host; Android origins from `ANDROID_APP_LINK_FINGERPRINTS`); login = ONE "Continue with …" list ×4; `mobile-oauth-start?provider=browser` = the native browser handoff.
- Mail: SES (`AWS_SES_REGION` + creds) OR `SMTP_*`, SES wins; neither = no mail.
- OIDC: `OIDC_PROVIDERS` (JSON array) = primary; `AUTH_OIDC_ENABLED`/`OIDC_*` = legacy, read only when unset.
- GitHub = ONE flow (SLOP-7): a Better Auth `github` provider on the App's OAuth client (`GITHUB_APP_CLIENT_ID/SECRET`; `GITHUB_LOGIN_ENABLED` adds the login button); members link it (`linkSocial`), installations + pushable repos list LIVE off their token (`integrations.github.*`), `repositories` store `installation_id`+`full_name`; `/integrations/github` = the one guided step. `GITHUB_POLLING=true` = outbound merge cron for NAT'd self-hosts.
- `STEER_RELAY_URL` unset = remote start/steer off; HS256 `STEER_RELAY_SECRET` must match the relay; BOTH relays need `TRUST_PROXY=true` behind a proxy.
- `CLIENT_MIN_VERSION_{ANDROID,IOS,DESKTOP,CLI}` gate with HTTP 426 + a blocking update screen (unset = off); MARKETING versions, never build numbers.
- Widget rate limits (`WIDGET_RATE_LIMIT_*`, `WIDGET_CONFIG_RATE_LIMIT_*`, REV-25): the per-KEY ones = self-host-only; cloud = per-TEAM plan ceiling (`lib/widget/submit-limit.ts`).

## Coding sessions & Actions

### The launcher

A thin launcher (`coding::prepare`, desktop + CLI): the issue's repo (tRPC) → a session-gated JIT GitHub-App token → a worktree + `exp/<IDENTIFIER>` branch with repo-local credential-helper git auth (EXP-73) → the `/api/mcp` MCP config with the user's `expu_` apikey, NOT `.mcp.json` → the system-prompt append (rebuilt on EVERY start/resume/shell): ONE `crates/coding/src/runbook.md` + the TEAM PROMPT (EXP-1025: server-only `teams.agent_prompt`, owner-edited in Settings → General web + IDE, cap `team.agentPromptMaxBytes`) + an agent's server-only `instructions`; the first prompt = the server-composed brief (`sessions.brief`). The agent commits, pushes, opens its PR via MCP `pr_open` (the `X-Exp-Session-Id` header names the `sessions` row). Every session gets its OWN worktree + branch; a repo-less session runs in a scratch dir (EXP-739); agents never touch trunk. Local deps: `git` + the claude CLI, never `gh`; Codex = a MANAGED pinned, sha256-checked download (EXP-1232 `coding::managed_codex`; custom `codexPath` wins). EXP-1196/1273 computer use = `crates/computer`: OUR thin FOREGROUND backend (x11rb XTest, CGEvent, SendInput; pixels only, no accessibility tree, no background mode, Wayland refused) behind ONE loopback MCP server, granted per run (start `computerUse`, default = synced `launch_defaults.computerUse`, OFF); the person's input is LOCKED while a run drives (XInput2 slave grabs / CGEventTap / low-level hooks, no elevation), the IDE shows the banner (`computer-use-banner.json`), Esc aborts + cancels the turn; a run works on one display at a time, coordinates in the fitted (≤1280x800) screenshot; `computerUseModel` (default `haiku`) = the SUBAGENT model for screen work. EXP-1236 code mode = `crates/codemode`, ONE loopback MCP server in EVERY run: `exec` runs boa JS, `tools.<server>.<tool>` = the run's HTTP MCP servers (stdio direct-only), `Promise.all` = parallel, only output returns; `describe` = schemas. Readiness = synced `devices.doctor`, rendered ×5 off `fixtures/device-doctor.json`. Default branches resolve live (never `main`): `boards.default_branch` → team `default_branch_override` → GitHub.

### Agents & the engine

EXP-201/EXP-746; `coding/src/agent.rs`, ONE engine (`crates/engine`) runs every session as in-process ACP 2.x on the user's UNMODIFIED CLI (`adapters/` = claude stream-json + control protocol, codex app-server); ONE `SessionUpdate`→`ActivityEvent` mapper feeds relay + UI; never spoof `CLAUDE_CODE_ENTRYPOINT`; logins + shell tabs = the ONLY agent PTYs. Claude ALWAYS gets `--allow-dangerously-skip-permissions` + a `--settings` layer pinning auto mode OFF; adapter auto-allows every `can_use_tool` but `AskUserQuestion`; codex full-access; ultracode claude-only. **Plan mode (EXP-1280) is OURS, never the CLI's** (no `--permission-mode plan`, no codex collaboration): `sessions.plan_mode` = still planning (brief + runbook `## Plan`; `pr_open` refused); the agent posts THE rich card with MCP `plan_propose` (GFM + `upload:<label>` pictures + choice `questions` + an eUI core `surface`) and stops; `sessions.answer` stamps decision/answers/surfaceData IN the card (approval flips `plan_mode` false), the device delivers `plan-card.json` `answerText` as the next turn (`fresh` = a fresh context); `sessions_ask` = one rich question outside a plan; ×4 render `plan-card.json` (`ChoiceTiles` = the eUI primitive). `runs.json` records the ACP id + the agent's NATIVE id (re-read on `/clear`); a resume re-enters the RECORDED run; a repo-less run purges WHOLE at end; `.exp-agents` gates worktree reuse. Doctor gates the SELECTED agent (git always) + ACP + the launch's LOGIN (runnable = ANY signed-in login; an unnamed launch takes the LAST USED one); pickers list only runnable agents. Nav ×4: rail Inbox · Threads · Agents (above Boards) · Boards · Actions · Devices · Settings; no right sidebars, no Show work; phones band lists per team. Steering: plan/model/effort launch-time; `config_state.options` = ONLY `model` (`/model <alias>`); `config_state`/`usage`/`rate_limit`/`queue`/`task_list`/`context_layout` = latest-wins STATE (`context_layout` = per-run segments, `lib/context-layout` ×4 fixture-locked); `queue` bar ×4 = UNREAD messages (mid-turn sent at once, `sent` until claude's replay lands the row; mid-compaction HELD, `unqueue` revokes held only; Stop returns all); `tool` carries `id`+contract `toolKind`, `tool_update` patches it (+ code mode's `script` log), a collapsed run's caption = contract `toolGroupSummary` ×4; `expToolDisplay` labels Exponential MCP calls; `turn` (started|ended) drives the spinner + `agent_busy`; a pending question/plan answers IN its card; `/` = `steerCommands` ∪ agent commands. Narration carries `messageId` (fragments merge) + on user rows `subagentId`. Transcripts live ONLY on the device (`{data_dir}/journal/<id>.jsonl`, pruned per `sessionRetentionDays`); a viewer join with no live room → `history_request` → `activity_synced`. `subagent.title` = the spawning call's description; a read-only Plan chip mirrors `config_state.currentMode`; ending verb = Stop. **Accounts (EXP-849):** per-account config dirs (`agent_profiles`); credentials NEVER copied, synced or brokered. A login = ONE profile per EMAIL: runs NEVER use the ambient login; doctor `import` (cap `agent-import`) MOVES it in; sign-ins stage then commit by email (a known email refreshes THAT profile + `alreadyAdded` toast ×4). `devices.agent_accounts`/`agent_usage` jsonb ride the heartbeat READ-ONLY; `health` = the usage probe's 401; keep-alive: codex `account/read` 6-hourly, claude = OUR refresh-token POST under the CLI's lock, written back ONLY to its source. Accounts = decision surface; Devices = repair (never `codex logout` the ambient login). Starts carry `account`. EXP-1005 auto-rotate (device toggle, default ON, claude only): every launch picks the profile with the most headroom off the usage cache (`coding::prepare`); a walled idle session resumes on another after a FORCED probe (`account_rotation`: cooldown + cap, `setBlocked.handled` mutes the push); codex waits. Polls pin past a reset only when EVERY window is maxed, ≤6h. MCP servers = ONE system (EXP-792): server-only TEAM rows (never repo `.mcp.json`); each MEMBER connects once, the SERVER holds their secrets (`mcp_credentials`, AES-GCM off `BETTER_AUTH_SECRET`), `resolveForLaunch` hands the LAUNCHER (never the agent's `kind: agent` key) ONLY its live row's pick (`X-Exp-Session-Id` → server-only `sessions.mcp_server_ids`) as `${VAR}` env refs, as the CALLER = device OWNER (a hosted run spends its tokens); unpicked/unconnected skip.

### Threads v2 (EXP-1291): ONE session tree

Spec = Model v3 (#EXP-1290). A **thread IS a root session**: `sessions` (roots `kind` thread|agent, `parent_id`/`root_id`/`depth`; runner `user_id` (starter), `device_id`, `cli`/`account`/`model`/`effort`, `repository_id`, `branch`; state `status` queued → running ⇄ idle → ended + `busy`/`needs_input`/`blocked`; thread facets `title`, `guide` (THE Guide), `action_*`, `spawned_by_session_id`; agent facets `name`/`icon`/`color`/server-only `instructions`/`triggers`). Around it: `session_issues` (root ↔ issue, n ↔ n), `messages` (the Conversation: people, the agent's FINAL turn reply, cards; `consumed_at` NULL = queued), `pull_requests` + `pull_request_issues` (ALL PR state; `issues` carries none). **Visibility ×4**: `shared` stamped at creation = the device is shared with the team (children inherit); shared → every member reads/posts/answers/Stops, else the starter only; shapes = `team_id IN member teams AND (shared OR user_id|owner_id = me)` (child tables carry `shared`/`owner_id` mirrors); PRs are team-visible. Transcripts stay on the device; opening a session shows its raw work.

ONE entrypoint **`sessions.send`** (`lib/sessions/send.ts`; tRPC + MCP `sessions_send`): message `sessionId` (an idle session resumes the SAME row; an ended thread continues in a new child), start a child `parentId`, or a new root (issueIds / actionId+inputs / repositoryId, or `agent` facets). The DB is the bus: a new session = a `queued` row naming its device, a message = a `messages` row; the device picks both up off its shapes and reports via `sessions.claim/report/reply/consume`; the relay keeps only the live layer. Starts land in the ACTIVE team (MCP: the calling session's). First prompt = `sessions.brief` (`@exp/domain-contract/session-brief`, fixture-locked): Request · Action · Issues · Parent. **Dispatch (2026-10-15)**: every new root (thread or agent) = `dispatch`: runs on the device's `threadModel` (haiku), no branch (read-only default-branch checkout), answers shallow asks, hands work to children who inherit server-only `work_launch` (the composer's picks) + the root's Action; runbook `## Dispatch`. Models are always explicit (contract `codingModel`, no blank). **Names + kinds (EXP-1341)**: ONE title chain ×4 = `session-title.json` (name/the agent CLI's title · its issues: a child's = `session_issues.assigned_session_id`, set by `sessions_send` `issueIds` on a child or a message (= forward); the dispatcher groups and forwards · `Run <action>` · first prompt), never `Session N`; `name` names a new session. Behaviour branches on `session-kind.json` traits (agent = persistent + triggers, revived by its device), never on `kind`. Merging never ends a session; Stop = interrupt the turn, End = explicit (a root ends its tree). Team agents (kind agent) never end: they idle, wake on a message or their own triggers (run LOCALLY by the agent's device) and spawn a thread per piece of work; owner-managed like boards (Settings → Agents). Follow-ups = `sessions_send{parentId}` children based on the parent's branch; caps depth 3 / 10 per tree. PRs: `pr_open` infers `base` (nearest open PR below `head` = a NATIVE stack, `base_pull_request_id`), links its session SUBTREE's issues (EXP-1334: link → In progress; stacks/parent issues wait), body = the Guide (`prBodyFromGuide`); `pr_merge{stack}` merges THROUGH a member, a plain merge of a stack member is refused; trees merge ROOT FIRST (retargets children). Clients ×4 build on the fixtures `session-card` (THE card = the Work card header), `session-thread` (EXP-1334: + · Conversation · folded issue/session chips · Guide, never scrolls, `partitionThreadStrip`; the conversation IS the dispatcher, children's asks inline; THE session composer, options mid-session via `sessions.configure`), `threads-list` (Needs you · Running · Recent + Agents), `agent-settings`, `session-brief`.

### Actions (EXP-253 → EXP-1291)

`actions` = SAVED PROMPTS: name, curated `icon`, markdown `body` ≤64KB (server-only; tRPC `actions.get`), ≤10 typed PICK inputs `repo|board|pr|issue`, `prompt_placeholder`. Running one = `sessions.send({actionId, inputs, text})`; the brief carries it. Member list/get, owner writes, MCP `actions_list/get/upsert/delete`, the body-less shape. No triggers (they live on agents), no repository, no MCP picks, no builtins.

### Desktop IDE & mobile

Desktop IDE = master-only + autopull (no branch switch; changes land via PRs or Source Control's CONFIRMED commit-and-push; `trunk_sync` badge + banner). First-run wizard ×4 (`lib/auth/onboarding.ts`, server-gated): team → board → invite → devices; a joiner owning NO device gets the devices step after the accept (EXP-1169). ONE device-setup block ×4 = that step, Add device, readiness `set_up_server`. Lists ×4 = a filled group band (no count) over FLAT gapless rows, NO dividers; `GlassGroup` = form fields (keeps its hairlines). A session in a thread = ONE card; tapping it opens the session view (the raw work). Workflows/tasks/subagents = ONE Work card, code mode = the Script card, `comments_create` = the real comment ×4. Starts from elsewhere never take the IDE.

## Billing (per-seat, Creem—cloud only)

Subscriptions bind to a TEAM (`creem_subscriptions.team_id` + `seats`, Creem `units` = seats), not the purchaser (REV2-55, `lib/billing/billing-handover.ts`): account deletion is never billing-blocked (cancels a SOLO team's subscription); team deletes REFUSE a live subscription (a period-end cancellation passes). ONE subscription per team; `billing.updateSeats`/`changePlan` prorate immediately. Free = 3 seats, 250MB, 1 widget; **Team** = the ONE paid tier, €15/seat/mo or €12 yearly: 10GB, unlimited widgets (`PlanTier` free|team|unlimited). Boards/repos/runs, push + steer = never plan-gated; over-seat teams only block invites.

**Limits exist only when `CLOUD_INSTANCE=true`**. Enterprise: no published pricing, `/contact/`. Self-host's one limit: no mobile push.

## Feedback widget (SLOP-4: ONE path, a submission IS an issue)

`packages/widget` (Preact + snapdom) builds a loader + lazy panel into `apps/web/public/widget/v1/`; API = `window.ExponentialWidget`.

Server-only `widget_configs` (public `expw_` key + domain allowlist, `board_id` NOT NULL) + `widget_submissions` (`issue_id` NOT NULL = the "Reported via widget" card ×4 + the reporter identity); public CORS routes `/api/widget/{config,submit}` (origin/rate-limit/honeypot in `lib/widget/`). ONE form: `message` + optional pictures/name/email (`form_config`), `labelIds` ≤10, `customFields`, `theme`, `launcher`; ONE capture button. Submit = issue + attachments (null `uploader_id`) + submission row in ONE transaction, `source='widget'`, `creator_id` NULL; reporter text is UNTRUSTED and escaped ONCE server-side into literal GFM (`lib/reporter-text.ts`, fixture-locked). **The conversation = comments**: a member comment with `audience=reporter` is emailed (`email_delivery_id` audit) with the magic link `/support/<token>` (`lib/reporter/token.ts`, HMAC over the ISSUE id, minted only when a reporter email exists); the reporter page reads the issue's reporter-audience comments + their own pictures and replies as `source=reporter` comments (a reply on a completed issue reopens it → `backlog`); notify = issue-scoped `reporter_reply`; the "Reply to reporter" toggle = the composer's leading-row pill ×4, only with a reporter email. The ONLY cloud upsell = the Widget settings' usage bar (owner-only, web + IDE read-only). The in-app widget = behind the sidebar "Report bug" button (cloud-only; key in `lib/runtime-config.ts`, `FEEDBACK_WIDGET_KEY` overrides).

**Conversion tracking (EXP-362, cloud only):** `lib/conversion/` + `adminConversions`; visitors = daily-salted HMAC(ip+ua), attribution = `ref`/`utm_*`, `events.ts` = the vocabulary.

## Style Conventions

- Template literals; functional components only
- Feedback ×4 = `feedback-states.json`: a person's failed action = toast, background never (`Couldn't <verb> <thing>`), invalid input inline, skeletons, not "Loading…"; motion = tokens
- shadcn/ui from `@exp/ui` ALWAYS over raw `<input>`/`<button>`/`<textarea>`/`<label>`; icons by CONCEPT

## Agent context budget (EXP-353)

Keep this file under 40k chars. MCP clients DEFER tool defs behind tool search (`_meta["anthropic/alwaysLoad"]` opts in): the always-loaded set == `lib/mcp/always-load.ts`, <10k serialized, whole surface <60k, per-tool <1.8k, `MCP_SERVER_INSTRUCTIONS` <2k with a self-contained first 512; gated by `lib/mcp/context-budget.test.ts`. **Compress, never append**: a rule over its rationale, a citation over a list.
