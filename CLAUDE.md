# Exponential

Real-time issue tracker.

## Tech Stack

TanStack Start (React 19, TanStack Router/React DB) · PostgreSQL 17 via Drizzle (`snake_case` casing) · ElectricSQL (shape proxy pattern, `@tanstack/electric-db-collection`) · Better Auth (email/password + OIDC `genericOAuth`, session-based, `tanstackStartCookies`) · tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync) · shadcn/ui on Tailwind v4 (OKLCH zinc, dark theme forced via `html.dark`; dates via `react-day-picker` + `date-fns`) · bun. Dev infra: Docker Compose — Postgres:54321, Electric:30000, Garage:3900 (S3), Caddy:3000 (HTTP/2 proxy; `Caddyfile` gitignored — copy `Caddyfile.example`), optional steer-relay:4002 (`--profile steer`).

## Monorepo Layout

```
apps/
├── web/        # TanStack Start app (the issue tracker)
├── push-relay/ # Push notification relay (Hono/Bun)
├── steer-relay/# Remote-start + live-steer WebSocket hub (Bun; in-memory presence/rooms)
├── marketing/  # Vite + React — owns the Remotion ClosedLoop hero movie (src/movie/)
├── ios/        # SwiftUI (Tuist + GRDB; ExpCore/ExpUI)
├── android/    # Kotlin / Jetpack Compose
└── desktop/    # Rust IDE (gpui + gpui-component + alacritty_terminal; embedded coding sessions)
packages/
├── db-schema/         # Drizzle schema + shared zod/domain types
├── design-tokens/     # OKLCH→sRGB tokens → Compose/SwiftUI/Rust
├── domain-contract/   # contract.json — canonical enums → per-language constants
├── icons/             # icons.json — the ONE icon registry → TS/Swift/Kotlin/Rust + SVGs
├── electric-protocol/ # Electric shape protocol fixtures
├── steer-ticket/      # HS256 steer-ticket sign/verify (web mints, relay verifies)
├── widget/            # Feedback widget (Preact + snapDOM) → apps/web/public/widget/v1/
└── tsconfig/
docker-compose.yaml    # DEV backend stack (not the self-host one)
selfhost/              # Pull-an-image self-host compose; INSTALL.md = agent-followable runbook
Dockerfile{,.push-relay,.steer-relay}   # build context = repo root
```

Workspace names: `@exp/<dir>`; `apps/desktop` is a Cargo workspace, not a bun workspace. **No `@exp/video` workspace** — the only movie is the ClosedLoop hero in `apps/marketing/src/movie/` (`movie-ref/` gitignored), embedded via `@remotion/player` in the lazy `LoopMoviePlayer` chunk; `LoopMovie.tsx` must stay remotion-free (`scripts/prerender.tsx` renders it under Bun). The releases feature is dead (EXP-106) — never reintroduce release footage/fixtures.

**Client parity:** all four clients (web, iOS, Android, desktop) sync the same **16** Electric shapes: teams, boards, issues, labels, issue_labels, users, team_members, team_invites, comments, attachments, notifications, issue_events, issue_subscribers, coding_sessions, actions, issue_statuses. The `actions` shape EXCLUDES the ≤64KB `body` (fetched via tRPC `actions.get` on demand) but carries `icon`. `repositories`, `user_notification_prefs`, `email_deliveries`, helpdesk and widget tables are **server-only (tRPC), never synced**.

**Vocabulary (EXP-180):** the product says **team** and **board** EVERYWHERE — copy, URLs (`/t/$teamSlug/boards/$boardSlug/issues/$id`), identifiers, DB, routers, shapes, MCP tools; workspace/project vocabulary and `/w/`+`/projects/` URLs are DEAD, no redirects — never reintroduce. Boards have no types; `repository_id` is NULLABLE (FK restrict); coding features gate purely on repo PRESENCE.

**Nothing is anonymously readable:** every shape is member-only (anonymous → impossible-match sentinel); no public tRPC; attachment reads require membership. Only anonymous endpoints: widget (`/api/widget/*`), helpdesk reporter magic-link (`/api/support/*` + `/support/$token`), invites, auth. Board-scoped shapes are TEAM-scoped with a STATIC trash predicate (REV2-5): `team_id IN (member teams) AND board_deleted_at IS NULL` — `board_deleted_at` is a trigger-maintained mirror of the board's `deleted_at` on all issue children. Trashed-board children leave sync as incremental move-out deltas, not a where-clause change — shape identities rotate ONLY on team-membership changes (embedded board-id lists once rotated 8 shape identities per board create/trash → full resyncs). Batch coding_sessions rows and issue-less notifications keep NULL `board_deleted_at` and always sync; the notifications shape is fully static per user (`user_id = me AND board_deleted_at IS NULL`; fan-out filters recipients at delivery; delivered rows outlive membership); issue-less `support_reply` rows carry a synced `team_id` for per-team Support routing.

The app is **noindex** everywhere: `__root.tsx` meta + ungated `X-Robots-Tag: noindex` from `server-bun.ts` (self-hosted too); `/robots.txt` ALLOWS crawling (`Disallow: /api/` only) so crawlers see the noindex; no sitemap. Marketing owns the indexed surface (its own robots.txt + `dist/sitemap.xml`/`llms.txt` from the `PAGES` manifest in `src/lib/seo.ts` via prerender).

Permissions are membership-only (`use-team-permissions.ts` + native mirrors); every member moderates and handles support. Signups get **no** team: first-run is create-or-join — "Create a team" (`teams.create`, open to every authed user; creator = owner; cloud free tier capped by `FREE_OWNED_TEAMS_CAP`) or "Join a team" (invite link; `teamInvites.accept` stamps `onboardingCompletedAt`). `teams.getDefault` is the NON-CREATING default-team resolver (oldest non-feedback membership or null; replaced `teams.ensureDefault` — bump `CLIENT_MIN_VERSION_*` past old native builds). An owner may delete ANY team incl. the last (cloud: only after its subscription is cancelled); invites optionally carry a synced `email` (server mails the link). The feedback team = `getFeedbackTeamId()` (slug `feedback`), never a flag; undeletable. **Billing (Creem) and the admin console are web-only.** **Desktop is the only client that runs coding sessions and publishes to the steer relay** (scrubbed activity channel only); since EXP-312 a LIVE session is visible/steerable ONLY by its owner — no view/steer perm split, no `perm`/`name` ticket claims (bump `CLIENT_MIN_VERSION_*` past pre-EXP-312 builds); teammates see just the synced status badge.

**Icons (EXP-273/317):** ONE icon set — Lucide — byte-identical on all four clients. `packages/icons/icons.json` is the source of truth: `pickable` (60 names backing board AND action icon pickers, byte-equal to contract `boardIcon.values`, APPEND-ONLY — reordering orphans stored rows), `semantic` (concept id → icon name), `custom` (hand-authored pie-clock status glyphs). `scripts/generate.ts` reads `lucide-react`'s `__iconNode` and emits COMMITTED outputs for all four platforms (iOS renders via the `AppIcon` view, NOT `Image(systemName:)`), drift-gated by `apps/web/src/lib/icons.test.ts`. Change an icon: edit `icons.json` + `bun run --filter @exp/icons generate` — never hand-edit generated files or add per-platform glyph maps. Cross-client surfaces name a CONCEPT (`conceptIcon(\`nav-search\`)`/`registry::NAV_SEARCH`/`AppIcons.navSearch`/`ExpIcons.navSearch`); `icons.test.ts`gates: settings navs resolve the same concept on web+desktop, desktop never uses gpui-component`IconName`outside`title_bar.rs`, web never imports deprecated lucide aliases. Multi-client surface → add a concept, don't import a glyph. Desktop assets also hold hand-maintained BRAND marks (`claude`/`codex`/`pi`/`logo\*`/`apple`/`google`) the generator must never own. Enum changes in `db-schema/src/domain.ts`→ update`domain-contract/contract.json`+`bun run --filter @exp/domain-contract generate`.

**Batch coding runs:** the release entity is entirely gone (no table/shape/router/`release_id`/UI/events/MCP tool). Multi-issue coding = **batch runs, desktop-only, any agent**: the ONE Start-coding dialog (`crates/ui/src/start_coding_dialog.rs` — searchable multi-issue picker; agent tabs with per-agent Model/Effort; ultracode + plan-mode Claude-only; skip-permissions claude+codex, OFF = guarded auto; ONE repo per run) launches single-issue for 1 checked issue, BATCH for 2+: ONE session on ONE pushed branch `exp/batch-<id8>` (8 hex so `parseIssueIdentifierFromBranch` can't mis-link), a prompt listing all issues (no per-issue subagents/worktrees/PRs), ending in ONE combined PR via `exponential_pr_open` with `issueIds` + `head` — the server links EVERY issue (same repo enforced); webhook/poller resolve a PR to ALL linked issues by exact `pr_url`, so merging completes them all (unless the team disabled/retargeted the automation). `coding_sessions` rows are issue- XOR batch- XOR action-scoped (`codingSessions.start` takes exactly one of issueId/teamId); batch/action sessions steer exactly like issue sessions.

**Markdown contract:** `issues.description` + `comments.body` are plain `text` GFM — the single interchange across web (TipTap + tiptap-markdown), iOS (cmark-gfm), desktop (comrak + the vendored WYSIWYG editor `crates/gpui-markdown-editor` [Apache-2.0 Velotype, see NOTICE], byte-parity-locked by `wysiwyg_parity.rs` over CONTRACT_FIXTURES), Android (`ui/markdown/` block editor, commonmark-java, parity-locked). Round-trippable: bold, italic, strikethrough, inline code, H1–H3, lists, task lists, blockquote, code blocks, links, block/inline images, @mentions, #issue mentions. **Underline intentionally unsupported**; tables, slash commands, image resize out of scope. **Mentions** = plain `@<email>`; the server resolves members, fires `issue_mention` + auto-subscribes (`lib/integrations/mentions.ts`); clients render known members as pills, @-autocomplete inserts the plain form. **Issue mentions** = plain `#<IDENTIFIER>`; clients render a pill only when it resolves to a synced same-team issue (token contract `lib/issue-refs.ts`; each client has its own lookup); `#` autocomplete on all four; server `resolveIssueRefs` fires NO notifications yet (deliberate). Images are stored relative `![alt](/api/attachments/{id})` — server canonicalizes on save (`canonicalizeMarkdownImageUrls`), clients absolutize at fetch. iOS editor is block-based (`IssueEditorModel` owns `[ContentBlock]`, derives markdown at save; atomic concurrent upload). Attachments carry probed `width`/`height`.

## Commands

From repo root unless noted.

```bash
bun install
bun run backend                    # docker compose up -d + web dev server (app at localhost:3000 via Caddy)
bun run ios                        # tuist generate + Xcode (Mac-only)
bun run android                    # install productionDebug + launch
bun dev                            # web dev server (localhost:5173)
bun run dev:marketing
bun run movie:studio / movie:render / movie:poster / movie:still <out.png> --frame=N
bun run dev:push-relay / start:push-relay        # localhost:4001
bun run dev:steer-relay / start:steer-relay / test:steer-relay   # localhost:4002
bun run build                      # widget + web + marketing (widget MUST build before web)
bun run build:web / build:widget / test:widget
bun run dev:widget                 # watch-build (pairs with bun dev; /widget/v1/demo.html)
bun run typecheck / test / test:e2e   # web app
bun run migrate / migrate:generate / psql
bun run backend:up / backend:down / backend:clear   # clear wipes volumes
bun run storage:init               # one-time Garage bootstrap; prints S3 keys
bun run format
bun run android:build / android:install
bun run dev:desktop                # gpui IDE against local backend
bun run build:desktop / appimage:desktop / test:desktop
bun run clean:desktop              # on zed/gpui rev bumps — cargo never GCs stranded artifacts
bun run --filter @exp/{domain-contract,design-tokens,icons} generate
```

Workspace scripts: `bun --filter @exp/web <script>` or `cd apps/web && bun run <script>`; plain `cargo` from `apps/desktop/` works. The generated Rust files (`contract.generated.rs`, `tokens.generated.rs`) are committed; regenerate only when `contract.json`/`tokens.json` change. Do NOT run `bun run lint` — its --fix corrupts `typeof import()` sites (EXP-13).

## Deploys

All on Coolify (`coolify.home.straehhuber.com`, Hetzner `46.225.140.133`). **Coolify is home-LAN-only — no auto-redeploy webhooks**; deploy from a LAN machine: `coolify deploy uuid <uuid>`.

- **Web cloud `app.exponential.at`** — uuid `hzoe7vty1rzjypyymsaqw2w6`, dockerimage `ghcr.io/niach/exponential-web:latest`, built by `build-web.yml` on master pushes + `v*` tags, multi-arch. The SAME image is the self-host distribution (`selfhost/` + `INSTALL.md`); the ghcr package must stay PUBLIC; self-hosters pin semver tags. Postgres `hqc1ofbam3x5kyxjexwj1oio`, Electric `s12y6uvto3utdsan5mrkhjjp`; attachments in Hetzner bucket `exponential`.
- **Marketing `exponential.at`** — uuid `bh4vnu32zwiu0bw6nf8d7yt8`; public-source clone; build `cd apps/marketing && bun run build`, start `npx -y serve apps/marketing/dist -l 80`.
- **Push relay `push.exponential.at`** — uuid `escnmp723si2642q1vcrmnqt`; builds `Dockerfile.push-relay`; holds `FIREBASE_SERVICE_ACCOUNT_JSON`.
- **Steer relay `steer.exponential.at`** — uuid `wxb6j3l0m01ogonvj5bxodum`; dockerimage `ghcr.io/niach/exponential-steer-relay:latest` (same triggers). Holds `STEER_RELAY_SECRET` (must match web env) **and `TRUST_PROXY=true`** (mandatory behind Traefik — per-IP rate limits key on `X-Forwarded-For`).
- **Staging `next.exponential.at`** — uuid `i2h9ozcemp70yigkf8jylaq2`, same web image; Postgres `mu6of6u8vul17sycib40zax8`, Electric `x80j1jdcf6zmviyh18d9b8iq`, bucket `exponentialnext`; Creem test mode. **Staging steer relay `steer-next.exponential.at`** — uuid `3reo2ipnx9m6srogc4h1xcow`, mirrors prod.
- **Android**: tag `android-vX.Y.Z` → one production APK + Play bundle to a GitHub Release (`make_latest: false`); signed when the keystore secrets are set. Play upload runs locally via fastlane.
- **Desktop**: tag `desktop-v*` (or dispatch) → `build-desktop.yml`: codegen-drift guard (regenerates the committed Rust files, fails on diff), then two channels × three OSes (macOS arm64, Linux x86*64, Windows x86_64 `continue-on-error`): `production` and `staging` (`--features staging`, app id `at.exponential.staging`, → next.exponential.at; channel = compile-time cargo feature). Linux: AppImage (glibc floor ubuntu-22.04/2.35). macOS: `.app` bundle (required for `exponential://`) — notarized `.dmg` when the `MACOS_CERT_P12`/`NOTARY*\*`secrets are set (they ARE, since 2026-07-11; copies in`~/keystores/`), else ad-hoc `.zip`. Windows: raw `.exe`only (download link AND updater asset; HKCU self-registration). Both desktops self-register the`exponential://` handler at startup (`app::desktop_integration`+ single-instance socket on Linux;`LSSetDefaultHandlerForURLScheme`on macOS). Production artifacts → GitHub Release (SHA256SUMS,`make_latest: true`; Android uses false so `releases/latest` stays desktop-owned). Self-update (EXP-22): check at launch + every 4h (`crates/ui/src/update.rs`; staging never) → `crates/updater`: download → SHA256SUMS verify → per-OS swap → gpui restart; a missing asset (incl. unsigned macOS), dev/non-AppImage runs, or unwritable installs degrade to a browser-link banner. Still manual: Linux `.deb`/tarball, signed Windows MSI.

### Release-time checklist (not automated)

- **Changelog (EXP-164)**: every user-facing release PREPENDS a `ChangelogEntry` to `apps/web/src/lib/changelog.ts` (fresh id, ISO date, title, one-line summary, short GFM body) — the head id re-surfaces the sidebar "What's new" card; `changelog.test.ts` enforces conventions.
- **Android**: Play upload manual via fastlane (docs/release-android.md). **iOS**: no CI — fastlane local (docs/release-ios.md).
- **GitHub App**: webhook Active (`${BETTER_AUTH_URL}/api/webhooks/github`, secret `GITHUB_WEBHOOK_SECRET`); `installation*` events arrive automatically; subscribe **Pull request**. Claim flow: callback `${BETTER_AUTH_URL}/api/integrations/github/callback`, client secret set, "Redirect on update" ticked, "Request user authorization" UNCHECKED. Permissions: `workflows` write + **Org → Members read-only** (EXP-363: claims require CONTROL — login match (user) / active membership (org), fail-closed `orgperm` until accepted; collaborators → `notowner` + install link); cached tokens keep mint-time perms ~1h. Staging + prod need separate Apps.
- **Deep links (EXP-92)**: web serves `/.well-known/apple-app-site-association` (static — team `V6W7BVCSM8`) + `assetlinks.json` (from `ANDROID_APP_LINK_FINGERPRINTS`; 404 unset — use the **Play App Signing key** SHA-256, NOT the upload keystore); iOS needs Associated Domains on both App IDs.
- **Cloud env**: sign-up is Google-only — `AUTH_PASSWORD_ENABLED=false`, `GOOGLE_LOGIN_ENABLED=true`, `AUTH_SIGNUP_ENABLED` unset (signup+login are ONE merged `/auth/login`; `/auth/register` redirects). SES stays set for transactional mail.
- `/api/health` gates the web HEALTHCHECK (DB-backed); push relay `/healthz`. DNS on Cloudflare, zone-only gray-cloud (keeps Traefik HTTP-01 working).

After schema changes, always: `bun run migrate:generate && bun run migrate`. Custom SQL triggers (`apps/web/src/db/out/custom/0001_triggers.sql`) auto-apply at every app boot (`bootstrap-cloud.ts` `applyCustomSql`, idempotent, self-hosted too); only never-booting contexts need manual psql (CI's schema job does).

## Web App Structure (`apps/web/src/`)

Trust the filesystem. Shadcn in `components/ui/`; feature components flat in `components/` (`agent-session.tsx` is the custom steer/activity view — no xterm). `lib/trpc/` routers: issues, boards, teams, labels, statuses, issue-labels, comments, notifications, subscriptions, team-members, team-invites, users, push-tokens, integrations, billing, admin, onboarding, repositories, coding-sessions, widgets, helpdesk, steer, mcp-grants. `lib/auth/` (server index.ts, client.ts `fetchSessionOnce`, membership.ts, policies.ts, shape-where.ts), `lib/collections.ts`, `lib/shape-route.ts`, `lib/filters.ts`, `lib/notification-email-policy.ts`/`-digest.ts` (push fires on create; email is a DIGEST of still-unread — DAILY default, hourly legacy opt-in, atomic `emailed_at` claim, scheduled from server-bun.ts), `lib/integrations/`, `lib/storage/`. `routes/`: `_authenticated/`, `t/$teamSlug/` (index, my-issues, inbox, support, settings, boards/$boardSlug + issues/$issueIdentifier), `auth/*` (incl. `consent.tsx` MCP OAuth scope picker), `invite/$token`, `api/shapes/` (16 proxies), `api/trpc/$.ts`, `api/mcp.ts`, `api/webhooks/github.ts`. Entry: `router.tsx`, `start.tsx` (defaultSsr: false), `server.ts`/`server-bun.ts`.

## Database

### Conventions

Better Auth user IDs are `text` — all user FKs must be `text`; app tables use UUID PKs (`gen_random_uuid()`). All tables have timezone `created_at`/`updated_at`; sort orders are `doublePrecision` (fractional indexing); rich text is `text` GFM. Due date is `date` only — time-of-day is DEAD (REV2-49), do not reintroduce.

### Key fields

Tables: see `@exp/db-schema`. Notable: `issue_statuses` (per-team; 7 locked builtins via `builtin_key`), `coding_sessions` (issue XOR batch XOR action; action rows carry `action_id` [set null] + `action_name` snapshot), `github_installation_links` (team ↔ installation claims).

Issues: `status` (builtin ANCHOR enum) + `statusId` (nullable FK `issue_statuses` SET NULL — the precise per-team status), `creatorId` (NULLABLE set-null — widget issues have none), `source` (`user`/`widget`), `duplicateOfId` (pairs with status `duplicate`), PR fields `prUrl`/`prNumber`/`prState`/`branch`/`prMergedAt` (one PR per issue, branch `exp/<IDENTIFIER>`; batch issues share ONE `prUrl`). Boards: `name`/`slug`/`prefix`/`color`/`icon`, nullable `repositoryId`, `deletedAt` + `isProtected`. Teams: `name`/`slug`/`iconUrl`, server-only `compTier`, synced `helpdeskEnabled` + PR-automation `prOpenedStatusId`/`prOpenedAutomation`/`prMergedStatusId`/`prMergedAutomation` (nullable FKs SET NULL — NULL = builtin default target, `*Automation=false` = "do nothing"; member-gated `statuses.setPrAutomation`; UI web + desktop).

### Enums

Canonical values live in `packages/domain-contract/contract.json` (support-thread `status`/`direction`/`visibility` are documented varchars in `domain.ts` only). Behavioral notes: `issue_status` — `pr_open` flips linked issues to the team's PR-open target (default `in_review`), merge to the PR-merge target (default `done`); `coding_session_status` (running/in_review/merged/ended) — `in_review` = PR open; PR MERGE flips live sessions to `merged` (still alive/steerable, EXP-358); `ended` ONLY from explicit ends (killSession, `codingSessions.end`, `mergePr({closeSessions:true})` = the "Merge and close" buttons), and the desktop reads its own row's →ended edge as the kill switch.

### Custom triggers (0001_triggers.sql)

`generate_issue_number()` (per-board number + `{prefix}-{number}`); `update_updated_at()`; `populate_issue_label_team_id()`/`populate_issue_child_team_id()` (denormalize `team_id` onto children; no-op when `issue_id` NULL — batch rows carry explicit team_id); `populate_issue_child_board_id()` (`board_id` + `board_deleted_at` onto children + notifications; fires on INSERT and `UPDATE OF board_id`); `populate_issue_board_context()` (same onto issues); `propagate_board_deleted_at()` (fans trash/restore to child mirrors; the `update_updated_at` triggers have a WHEN guard so fan-out never bumps `updated_at`); `seed_builtin_issue_statuses()` (AFTER INSERT ON teams; mirrors contract `issueStatusDefaults`, parity-locked by test); `populate_issue_status_id()` (derives `status_id` from the anchor for enum-only writers; explicit dual-writes never overridden).

## Patterns

### Electric shape proxies

One proxy per synced table in `routes/api/shapes/` (16), built with `createShapeRouteHandler`; all member-only. Every proxy pins a server-side `columns` allowlist clients cannot widen: `issue-subscribers` excludes reporter `email` (PII); `notifications` excludes `emailed_at`/`board_id`/`board_deleted_at` (a shape may FILTER on a column its allowlist excludes — Electric evaluates `where` server-side); the 8 board-scoped shapes exclude the scoping columns; `users` is pinned to exactly `id,name,email,image,created_at,updated_at` (server-only columns must NEVER sync — unknown-column updates once bricked native sync loops); `teams` pins its contract list (`comp_tier` never); `boards` pins its full list (incl. `deleted_at`/`is_protected`); `actions` excludes `body`. New server-only column on a synced table → add it BEHIND the allowlist. Hardening: (1) responses always carry `cache-control: private, no-store` + `vary: authorization, x-api-key, cookie` (Electric's upstream `public, max-age=604800` poisoned macOS URLCache with cross-auth snapshots); (2) failing token credentials → explicit 401, never the anonymous where clause (cookie-only still falls back anonymously); (3) `buildWhereClause` sorts id lists — the where clause is part of Electric's shape identity; heap-order flips caused native 409 loops — and membership id lists stay OUT of board-scoped where clauses (`buildTeamScopedChildWhere`). A FIFO semaphore bounds concurrent initial-snapshot (`offset=-1`) forwarding (`electric-proxy.ts`; bodies buffered for Traefik-safe framing; live long-polls never gated).

### Board trash (48h soft delete)

`boards.delete` stamps `deleted_at` (owner-only; refuses `is_protected`). Trashed boards vanish via the trash-aware helpers in `lib/auth/membership.ts`, the boards shape's static `"deleted_at" IS NULL`, and the trigger-fanned child mirror, but keep their `(team_id, slug)` reservation. `boards.restore`/`listDeleted` (owner-only) power the web-only "Pending deletion" settings card. The purge sweep (`lib/board-trash.ts`) hard-deletes after 48h and reclaims S3 blobs — as do team/admin deletes (collect `storage_key`s in-tx, delete after commit). `is_protected` is bootstrap-stamped on the dogfood board (server-guarded; clients disable the affordance). Widgets router mutations + the helpdesk toggle are owner-only (enabling helpdesk is paid-gated). `use-team-permissions.ts` exposes `isOwner` + capability booleans — the contract natives mirror (owner-only controls HIDDEN for non-owners; destructive native actions confirm first).

### Custom issue statuses (EXP-314)

Per-TEAM rows in six fixed categories (backlog/unstarted/started/completed/cancelled/duplicate; contract carries values + display/settings order + `startedMax: 4`). 7 LOCKED builtins per team (`builtin_key` = legacy enum values — never renamed/recolored/deleted, movable within category; seeded by trigger + migration; defaults live ONCE in contract `issueStatusDefaults`). Customs are name+color, member-managed (`statuses` router; started ≤4; delete requires reassign; management UI web+desktop, mobile renders/picks). `issues.status` STAYS the dual-written **anchor** (`CATEGORY_ANCHOR`: backlog→backlog, unstarted→todo, started→in_progress, completed→done, cancelled→cancelled) so enum-keyed subsystems (completedAt derivation, pr-sync from-sets, MCP enum tools, old clients) keep working; PR open/merge TARGETS are per-team configurable (any non-duplicate status or "do nothing"; completedAt rides the anchor derivation). `statusId` carries the precise row; `populate_issue_status_id` re-anchors enum-only writers. Rendering: statusId → row, else builtin by anchor, else constructed default (never fails); builtins render LEGACY token colors, customs their hex; started statuses render pie-clock icons by position (`startedClockIcon`, hand-mirrored ×4, lock-tested; glyphs `progress-*`). Lists group by status ROW; filters are group-key sets (web accepts legacy enum URL tokens); `status_changed` payloads carry from/to ids + names. Duplicate: no customs, excluded from pickers, enum+`duplicateOfId` lockstep.

### Web plumbing

- **Collections** (`lib/collections.ts`): all use `columnMapper: snakeCamelMapper()` — without it `useLiveQuery` `where` silently fails. `undefined` (not `false`) skips a query; use `and()`/`or()` from `@tanstack/react-db`, not `&&`/`||`.
- **Auth guard**: `_authenticated.tsx` `beforeLoad` + `throw redirect()`; session fetched once via `fetchSessionOnce()` and cached. Mutations via tRPC; `generateTxId` lets the client await Electric sync.
- **MCP OAuth consent**: `lib/auth/mcp-authorize-guard.ts` pre-flights every `mcp/authorize` (actionable HTML errors; forces `prompt=consent`) → `/auth/consent` team/board multi-select persisted to `mcp_grants` BEFORE the code mints. `lib/mcp/scope.ts` confines OAuth tokens to the grant (no grant row = nothing); OAuth tokens are accepted ONLY at `/api/mcp` (shapes/tRPC reject); cookies + personal `expu_` keys keep full access. Login resumes interrupted authorizes and drops the `oidc_login_prompt` cookie (`lib/auth/oauth-resume.ts`).

### Issue UI

Issue detail is a full-page route fed a live `issue` from Electric; title/description save on blur, other fields mutate immediately; `completedAt` is auto-managed by the update mutation. Filters live in `lib/filters.ts`. All UI uses shadcn/ui — no raw `<input>`/`<button>`/`<textarea>`/`<label>`; borderless dialog inputs `border-none shadow-none focus-visible:ring-0`; icon-only dropdown triggers `Button variant="ghost"` `h-5 w-5 p-0`.

## Environment Variables (.env)

```
DATABASE_URL / BETTER_AUTH_SECRET / BETTER_AUTH_URL / BETTER_AUTH_TRUSTED_ORIGINS
ELECTRIC_URL / ELECTRIC_SOURCE_ID / ELECTRIC_SECRET   # localhost:30000; source id+secret only for Electric Cloud
S3_ENDPOINT / S3_ACCESS_KEY / S3_SECRET_KEY / S3_BUCKET / S3_REGION   # attachments (Garage defaults; keys from storage:init)
AUTH_PASSWORD_ENABLED / AUTH_SIGNUP_ENABLED   # password login default true; public signup on in dev, OFF in production
AWS_SES_REGION / AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY / EMAIL_FROM / EMAIL_REPLY_TO   # SES (unset region = email off)
SMTP_HOST / SMTP_PORT / SMTP_USER / SMTP_PASS / SMTP_SECURE  # SMTP alternative for ALL mail (SES wins if both)
OIDC_PROVIDERS                # JSON array — the primary OIDC mechanism (see .env.example)
AUTH_OIDC_ENABLED / OIDC_CLIENT_ID / OIDC_CLIENT_SECRET / OIDC_DISCOVERY_URL / OIDC_PROVIDER_ID  # legacy single-provider (only when OIDC_PROVIDERS unset)
GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET / GOOGLE_LOGIN_ENABLED
APPLE_CLIENT_ID               # SIWA *Services ID* (required alongside Google on iOS — guideline 4.8)
APPLE_PRIVATE_KEY / APPLE_KEY_ID / APPLE_TEAM_ID / APPLE_CLIENT_SECRET   # .p8 base64, server mints the JWT at boot; static secret wins
APPLE_LOGIN_ENABLED / APPLE_APP_BUNDLE_IDENTIFIER  # native idToken exchange (at.exponential)
GITHUB_APP_ID / GITHUB_APP_SLUG / GITHUB_APP_PRIVATE_KEY   # id/slug/PEM-base64; installations claimed PER TEAM (github_installation_links)
GITHUB_APP_CLIENT_ID / GITHUB_APP_CLIENT_SECRET    # team claim flow (secret unset ⇒ install-page round-trip fallback)
GITHUB_WEBHOOK_SECRET         # App webhook HMAC (PR-merge detection)
GITHUB_POLLING                # 'true' = outbound merge cron (self-host behind NAT; decoupled from SELF_HOSTED)
SES_WEBHOOK_SECRET            # SES bounce/complaint SNS webhook (unset = off)
SELF_HOSTED                   # 'true' = billing off, plan limits unlocked
CREEM_API_KEY / CREEM_WEBHOOK_SECRET / CREEM_TEAM_PRODUCT_ID / CREEM_TEAM_YEARLY_PRODUCT_ID
PUSH_RELAY_URL / PUSH_RELAY_SECRET
STEER_RELAY_URL / STEER_RELAY_SECRET   # unset URL = remote start/steer off (LAN fine, dials OUT); HS256 secret must match relay
ANDROID_APP_LINK_FINGERPRINTS # SHA-256 fingerprints for assetlinks.json (unset = 404)
SECURITY_HEADERS_ENABLED      # 'true' = CSP/HSTS from the Bun server
BUN_CONFIG_MAX_HTTP_REQUESTS  # Bun outbound-fetch cap; baked 65336 into the image — 16 long-polls/client saturate the 256 default (REV2-6)
INITIAL_ADMIN_EMAILS          # auto-promoted global admins
CLIENT_MIN_VERSION_{ANDROID,IOS,DESKTOP}    # below ⇒ HTTP 426 blocking update screen; unset = off. Always MARKETING versions, NEVER versionCode/build number
CLIENT_LATEST_VERSION_{ANDROID,IOS,DESKTOP} # informational (GET /api/version + 426 body)
WIDGET_RATE_LIMIT_PER_{KEY,IP,EMAIL}_HOURLY # widget submit limits (60/60/6; *_BURST variants)
CONTACT_EMAIL_TO              # POST /api/contact recipient (503 without mail transport)
```

Relays have their own env (`apps/*/.env.example`): push relay reads `FIREBASE_SERVICE_ACCOUNT_JSON`; BOTH relays read `TRUST_PROXY=true` — mandatory behind any reverse proxy. Dead env vars (read by no code, do not re-document): `GOOGLE_CALENDAR_ENABLED`, `DOGFOOD_REPO`.

## Integrations

### Coding flow (v2 — "Start coding" launcher)

The old agent-core runtime, companion daemon, synthetic agent identity, `agent_runs` state machine, and `assigned-issues` shape are **deleted**. The flow is a thin launcher in the desktop IDE: resolve the issue's repo from the team registry (tRPC) → mint a session-gated JIT GitHub-App installation token → create a worktree + `exp/<IDENTIFIER>` branch with ambient git auth via a repo-local credential helper (EXP-73: `origin` stays the bare URL; token in `<clone>/.git/exp-git-credentials` with a no-downgrade guard, refreshed on GitHub's REAL `expires_at`; every writer goes through `coding::git_credentials::ensure`) → write `.exp-mcp.json` pointing at `/api/mcp` (user's personal `expu_` apikey; deliberately NOT `.mcp.json` — claude's project-approval scan of a cwd `.mcp.json` is unconditional, so it rides `--mcp-config .exp-mcp.json`) → spawn the agent CLI in the embedded terminal with a plan-first prompt.

**Three agents** (EXP-201; `coding/src/agent.rs`, contract `codingAgent`; savable default + per-agent path/model/effort in settings): **claude** (default — `--permission-mode auto` guarded, `--dangerously-skip-permissions` via checkbox, `--permission-mode plan`), **codex** (explicit workspace-write Auto preset or `--dangerously-bypass-approvals-and-sandbox`; MCP via `-c mcp_servers.*` argv, key rides spawn env `EXP_MCP_TOKEN` only), **pi** (no permission system/native MCP: the launcher writes a `.exp-pi-mcp.ts` extension loaded via `-e` bridging MCP over HTTP; `EXP_MCP_URL`/`EXP_MCP_TOKEN`). Ultracode + plan mode Claude-only; the doctor probes all three but gates only the SELECTED agent (git always required); the steer `online` frame advertises installed agents so remote Start-coding pickers offer only what the device runs (`steer.startSession` validates server-side; absent agent = claude). The agent commits, pushes, opens its own PR via the MCP `open_pr` tool. Local deps: `git` + agent CLIs — never `gh`. A slim synced `coding_sessions` row powers the cross-client "coding now" badge; the person coding is the real signed-in user. `docs/masterplan.md` (+ archive; cited `masterplan §…` in comments) is gitignored/local-only.

### Billing (per-seat, Creem)

Subscriptions bind to a TEAM (`creem_subscriptions.team_id` + `seats`; checkout `billing.createSeatCheckout` with Creem `units` = the authoritative seat count) and belong to the TEAM, not the purchaser (REV2-55, `lib/billing/billing-handover.ts`): `reference_id` nullable/set-null — account deletion is NEVER blocked by billing (store policy) and only cancels a SOLO team's subscription the deletion destroys; conversely team deletes REFUSE a live subscription (`PRECONDITION_FAILED` — cancel in settings → Billing first; a period-end cancellation passes), natives point at web. ONE subscription per team: `createSeatCheckout` refuses duplicates; `billing.updateSeats`/`changePlan` mutate the EXISTING subscription with `update_behavior: proration-charge-immediately` (Creem proration was broken on increases early July 2026, re-verified fixed 2026-07-21; evidence in `lib/billing/creem-subscriptions.ts`). Free = 3 seats, 250MB storage, 1 widget; **Team** = the ONE paid tier, €15/seat/mo or €12 yearly — 10GB, unlimited widgets, helpdesk (`PlanTier = free|team|unlimited`; comp tiers `team|unlimited`); Enterprise is a contact LINE, not a tier. Unlimited boards/repos/coding sessions every tier; push + steer never plan-gated; over-seat teams only block new invites. `SELF_HOSTED=true` unlocks every FEATURE limit — a product switch, not a licence: the repo is Apache-2.0 (EXP-352), self-hosting free for everyone, no licence gate in code; paid Enterprise Support (€590/yr ≤25 users, €1,900/yr ≤100, custom) is a support contract, never a code gate. Self-host's one limitation: no MOBILE push (store apps compile against first-party Firebase). Repo default branches are resolved live at connect and healed on `repositories.list`/`installationToken` — never assume `main`.

### Actions (EXP-253)

`actions` rows (per team, markdown `body` ≤64KB, optional `repository_id` SET NULL, optional curated `icon`) — tRPC CRUD (member list/get, owner writes) + 4 MCP tools + the body-less shape. Runs are `coding_sessions` rows (`action_id` + `action_name` snapshot) executed LOCALLY by the desktop as interactive agent sessions (any agent, per-agent model/effort) on the repo's trunk clone, a PR branch's worktree (fix-conflicts), or a scratch dir — never server-side secrets (runs use the user's own device auth); NO per-device trust prompt. Remote start rides the steer rails: devices advertise `caps: ["actions"]`, `steer.startSession({actionId, deviceId, agent?, model?, effort?})` gates on it. All four clients have an Actions surface (web `t/$teamSlug/agents` + desktop rail: list/run/create/edit; mobile: view + run). TWO virtual builtins are NOT DB rows — each client CONSTRUCTS them locally (`lib/builtin-actions.ts` web, `api::actions::builtin_*` desktop; `actions.list` still appends both for older builds), pinned FIRST by `builtin` flag; every builtin start carries `teamId`; `get/update/delete` reject the reserved ids: **"Create action"** (`builtin:create-action`, describe-with-Claude creator, scratch cwd) and **"Fix merge conflicts"** (`builtin:fix-conflicts` — required `pr` input: the representative ISSUE id of an issue-linked open PR, deduped by prUrl for batch PRs; runs in the PR branch's WORKTREE: rebase onto origin/<default>, resolve, force-push, merge via `exponential_pr_merge`; desktop Reviews offers it on a failed merge; remote start also gates on the `fix-conflicts` cap). Both get normal per-agent MCP wiring and prompts from shipped constants (`body` empty). The one-shot claude_task primitive is deleted (EXP-259); the trunk conflict banner keeps Open terminal / Abort / Discard & reset. The 3 default actions ship as TEMPLATES, never seeded rows.

The desktop IDE is master-only + autopull: no TopBar, no branch switch/commit/push (view-only editor — changes arrive via PRs; escape hatch = Discard-and-reset); the git bar became the headless `trunk_sync` engine with a rail status badge. Mobile ships full onboarding: server-gated first-run wizard (`onboardingCompletedAt`, `lib/auth/onboarding.ts`) — create-or-join team, then guided first board with optional repo picker + inline GitHub App connect; no account-level Integrations menu; action EDITING is web/desktop-only.

### Embeddable feedback widget

marker.io-style widget via a GA-style async `<script>` snippet. Source `packages/widget` (Preact shadow root + `@zumer/snapdom`); build emits IIFE `loader.js` (command queue, floating button, config prefetch) + `widget.js` (lazy panel + capture) + `demo.html` into `apps/web/public/widget/v1/`. API `window.ExponentialWidget`: `init({key})`, `identify`, `setCustomData`, `open`, `close`. Screenshots: client-side snapDOM, viewport-cropped, WebP→PNG→JPEG ladder, never block submission (behind a `CaptureEngine` interface); annotatable (rect/line/arrow, fixed red — shapes stay editable in image-pixel space, flattened on submit).

Server: server-only `widget_configs` (public `expw_` key + domain allowlist) + `widget_submissions`; public CORS routes `/api/widget/config` + `/submit` (CORS/origin/rate-limit/honeypot in `lib/widget/`). **Modes** `feedback`/`support`/both (`form_config.modes`; absent = feedback-only). Feedback files an ordinary issue onto `widget_configs.board_id` (NULLABLE — required iff feedback mode; set-null so trashing the board degrades feedback, not the config); the submission creates issue + screenshot attachment (null `uploader_id`) + submission row in ONE transaction, with `creator_id NULL` + `source='widget'` (the synthetic `isAgent` user pattern is GONE). Support files a STANDALONE ticket — `support_threads` + opening `support_messages` row, NO issue — gated on `teams.helpdesk_enabled` + paid plan (`assertCanUseHelpdesk`), re-checked per submit; reporter credential = emailed magic link (deterministic HMAC over thread id, `lib/helpdesk/token.ts`); members use the `helpdesk` router (`listThreads`, close/reopen, `escalate` → linked issue); notify via issue-less `support_reply` fan-out. Rate limiting = in-process token buckets (`WIDGET_RATE_LIMIT_*`, `SUPPORT_RATE_LIMIT_*`). Managed in team settings → "Feedback widget" (owner-only; the Helpdesk toggle card lives there too). Dogfood bootstrap: `Exponential App` config on the feedback team with ONE protected repo-backed board (slug `exponential`, prefix `EXP`) — helpdesk on, legacy `support` board un-protected, modes healed once, team comped (only while `comp_tier` NULL). `FeedbackWidgetProvider` pins the launcher `bottom-right`; the sidebar FeedbackButton opens the same widget and renders NOTHING when no config exists (dogfood key domain-allowlisted to exponential.at/app.exponential.at).

Dev-mode gotcha: the nitro-alpha dev bridge renders 404-status responses as connect `Cannot GET/POST` HTML and strips custom headers — dev-only; production passes responses through.

## Style Conventions

- Template literals for strings (backticks, not quotes); functional components only
- shadcn/ui in `src/components/ui/` — always over raw HTML; icons from `lucide-react`
- Business logic components in `src/components/` (not `ui/`)

## Agent context budget (EXP-353)

Keep this file under 40k chars and the MCP tool defs under ~24k serialized chars (Claude Code warns at 40k/25k); gated by `apps/web/src/lib/mcp/context-budget.test.ts`. Compress instead of appending.
