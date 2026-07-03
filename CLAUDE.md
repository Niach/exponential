# Exponential

Real-time issue tracker.

## Tech Stack

- **Framework**: TanStack Start (React 19, TanStack Router, TanStack React DB)
- **Database**: PostgreSQL 17 via Drizzle ORM (`snake_case` casing)
- **Real-time**: ElectricSQL (shape proxy pattern via `@tanstack/electric-db-collection`)
- **Auth**: Better Auth (email/password + OIDC via `genericOAuth` plugin, session-based, `tanstackStartCookies` plugin)
- **API**: tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync)
- **UI**: shadcn/ui on Tailwind v4 (OKLCH zinc palette, dark theme forced via `html.dark`)
- **Date Picker**: `react-day-picker` + `date-fns` via shadcn `Calendar` component
- **Infrastructure**: Docker Compose — Postgres:54321, Electric:30000, Garage:3900 (S3-compatible), Caddy:3000 (HTTP/2 reverse proxy; `Caddyfile` is gitignored — copy from `Caddyfile.example`, which is configured for Electric long-poll timeouts), optional steer-relay:4002 (`--profile steer`)
- **Package Manager**: bun

## Monorepo Layout

```
exponential/
├── apps/
│   ├── web/        # TanStack Start app (the issue tracker)
│   ├── push-relay/ # Standalone push notification relay (Hono/Bun, separately deployed)
│   ├── steer-relay/# Standalone remote-start + live-terminal-steer WebSocket hub (Bun; device presence + session rooms in memory)
│   ├── marketing/  # Marketing site (Vite + React, deployed via Coolify)
│   ├── ios/        # Native SwiftUI iOS app (Tuist + GRDB) — self-contained (ExpCore/ExpUI become iOS-only frameworks; the legacy ExponentialMac target still exists until masterplan-v3 Phase 7 deletes it)
│   ├── android/    # Native Kotlin / Jetpack Compose app
│   └── desktop/    # Cross-platform desktop IDE (Rust: gpui + gpui-component + alacritty_terminal; embedded `claude` coding sessions)
├── packages/
│   ├── db-schema/          # Drizzle schema + shared zod/domain types
│   ├── design-tokens/      # OKLCH→sRGB theme tokens → Compose/SwiftUI/Rust
│   ├── domain-contract/    # contract.json — canonical enum values; emits per-language constants
│   ├── electric-protocol/  # Electric SQL shape protocol fixtures
│   ├── steer-ticket/       # Shared HS256 steer-ticket sign/verify (web mints, relay verifies)
│   ├── widget/             # Embeddable feedback widget (Preact + snapDOM); builds loader.js/widget.js into apps/web/public/widget/v1/
│   └── tsconfig/           # Shared TS configs
├── docker-compose.yaml
├── Caddyfile
├── Dockerfile              # Builds the web app image; build context = repo root
├── Dockerfile.push-relay   # Builds the push relay image; build context = repo root
├── Dockerfile.steer-relay  # Builds the steer relay image; build context = repo root
└── package.json    # bun workspaces, dispatcher scripts
```

Workspace package names: `@exp/web`, `@exp/push-relay`, `@exp/steer-relay`, `@exp/marketing`, `@exp/db-schema`, `@exp/design-tokens`, `@exp/domain-contract`, `@exp/electric-protocol`, `@exp/steer-ticket`, `@exp/widget`, `@exp/tsconfig`. (The desktop app `apps/desktop` is a Rust Cargo workspace, not a bun workspace.)

**Client parity:** all four clients (web, iOS, Android, desktop) sync the same **fourteen** Electric shapes (workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, **coding_sessions**) — proxy count == shape count == 14. `repositories`, `project_repositories`, `user_notification_prefs`, `email_deliveries`, and the widget tables are **server-only (tRPC), never synced**. All clients honor `isPublic` / `publicWritePolicy` field gating via a small `WorkspacePermissions` helper that mirrors `apps/web/src/hooks/use-workspace-permissions.ts`. **Billing (Creem) and the admin console are intentionally web-only** — native clients show no billing UI (store-policy safe). **The desktop app is the only client that runs coding sessions and publishes to the steer relay.** When changing enum values in `packages/db-schema/src/domain.ts`, also update `packages/domain-contract/contract.json` and run `bun run --filter @exp/domain-contract generate` to refresh the Swift / Kotlin / Rust constants.

**Description / comment markdown contract:** `issues.description` and `comments.body` are plain `text` columns holding **GFM** markdown — the single interchange contract across web (TipTap + tiptap-markdown), iOS (cmark-gfm), desktop (Rust — pulldown-cmark or comrak, GFM), and Android (from-scratch block editor in `ui/markdown/`, commonmark-java; byte-parity locked by its test suite). Supported, round-trippable features: bold, italic, strikethrough, inline code, headings (H1–H3 editable), bullet/ordered lists, **task lists** (`- [ ]`/`- [x]`), blockquote, code blocks, links, **block/full-width inline images**, and **@mentions**. **Underline is intentionally unsupported** (no GFM representation — it does not round-trip); tables, slash commands, and image resize are intentionally out of scope. **Mentions** are written as `@<email>` in the markdown source (the single interchange form — round-trips as plain GFM text); the server resolves `@email` to workspace members and fires `issue_mention` notifications + auto-subscribes them (`apps/web/src/lib/integrations/mentions.ts`). Clients render a known member's `@email` as a name pill; the web editor offers an @-autocomplete that inserts the `@email` form. Embedded images are always stored as the relative form `![alt](/api/attachments/{id})`; the server canonicalizes image URLs to relative on save (`canonicalizeMarkdownImageUrls` in `apps/web/src/lib/storage/issue-attachments.ts`), and clients resolve to absolute only at fetch time. The iOS editor is block-based: `IssueEditorModel` (an `@Observable`) owns `[ContentBlock]` as the single source of truth and derives markdown only at save; image upload is atomic (all-or-nothing) and concurrent. Attachments carry probed `width`/`height` so clients can pre-size and avoid layout shift.

## Commands

All commands run from the repo root unless noted.

```bash
bun install                        # Install workspace deps (run from root)
bun dev                            # Start web dev server (apps/web, localhost:5173)
bun run dev:marketing              # Start marketing dev server (apps/marketing)
bun run dev:push-relay             # Start push relay dev server (apps/push-relay, localhost:4001)
bun run start:push-relay           # Start push relay in production mode
bun run dev:steer-relay            # Start steer relay dev server (apps/steer-relay, localhost:4002)
bun run start:steer-relay          # Start steer relay in production mode
bun run test:steer-relay           # Steer relay + ticket unit/integration tests
bun run build                      # Build widget + web + marketing (widget MUST build before web)
bun run build:web                  # Build only the web app
bun run build:widget               # Build the embeddable widget into apps/web/public/widget/v1/
bun run dev:widget                 # Watch-build the widget (pairs with `bun dev`; test page at /widget/v1/demo.html)
bun run test:widget                # Run widget package unit tests
bun run typecheck                  # Typecheck the web app
bun run test                       # Run web vitest unit tests
bun run test:e2e                   # Run web playwright e2e tests
bun run migrate                    # Apply Drizzle migrations
bun run migrate:generate           # Generate migrations from schema changes
bun run psql                       # psql shell into local DB

bun run backend:up                 # docker compose up -d (Postgres + Electric + Garage + Caddy)
bun run backend:down               # docker compose down
bun run backend:clear              # docker compose down -v (wipe volumes)
bun run storage:init               # one-time Garage bootstrap (after backend:up); prints S3 access/secret key

bun run lint                       # ESLint fix across workspaces
bun run format                     # Prettier format

bun run android:build              # ./gradlew :app:assembleProductionDebug in apps/android (:staging variant exists)
bun run android:install            # ./gradlew :app:installProductionDebug (:staging variant exists)

bun run dev:desktop                # gpui IDE against LOCAL backend (EXP_INSTANCE_URL=http://localhost:3000)
bun run dev:desktop:staging        # gpui IDE, "Cloud" button → next.exponential.at (EXP_INSTANCE_URL override)
bun run dev:desktop:prod           # gpui IDE, "Cloud" button → app.exponential.at
bun run build:desktop              # cargo build --release -p app (production channel)
bun run build:desktop:staging      # cargo build --release -p app --features staging (staging channel: next.exponential.at + distinct app id)
bun run appimage:desktop           # build + package a Linux AppImage (production)
bun run appimage:desktop:staging   # build + package a Linux AppImage (staging)
bun run test:desktop               # cargo test (apps/desktop workspace)

bun run --filter @exp/domain-contract generate   # Regenerate iOS + Android + desktop enum constants
bun run --filter @exp/design-tokens generate     # Regenerate Android + iOS + desktop theme tokens
```

You can also run a script directly inside a workspace: `bun --filter @exp/web <script>` or `cd apps/web && bun run <script>`. The desktop commands shell out to `cargo` (like `android:build` shells out to gradle) — `apps/desktop` is a Rust Cargo workspace, so plain `cargo` from `apps/desktop/` works too. Its two generated Rust files (`crates/domain/src/contract.generated.rs`, `crates/theme/src/tokens.generated.rs`) are committed; re-run the generators above only when `contract.json` / `tokens.json` change.

## Deploys

Three production targets run on Coolify (`coolify.home.straehhuber.com`, Hetzner host `46.225.140.133`), one self-host target on the home Portainer stack.

- **Web cloud (`app.exponential.at`)** — Coolify app `exponential-web` (uuid `hzoe7vty1rzjypyymsaqw2w6`), a *dockerimage* app pulling `ghcr.io/niach/exponential-web:latest`. The image is built by `.github/workflows/build-issues-web.yml` on every push to `master` and on `v*.*.*` / `v*.*.*-dev` tags. **Coolify is home-LAN-only, so there is no auto-redeploy webhook** — after a green Actions run, redeploy manually from a LAN-connected machine: `coolify deploy uuid hzoe7vty1rzjypyymsaqw2w6` (or click "Deploy" in the Coolify UI). Backed by `exponential-postgres` (uuid `hqc1ofbam3x5kyxjexwj1oio`) and `exponential-electric` (uuid `s12y6uvto3utdsan5mrkhjjp`); attachments live in Hetzner Object Storage bucket `exponential` at `nbg1.your-objectstorage.com`.
- **Marketing (`exponential.at`)** — Coolify app `exponential-marketing` (uuid `bh4vnu32zwiu0bw6nf8d7yt8`). Public-source app cloning `https://github.com/Niach/exponential.git`, base directory `/`, build `cd apps/marketing && bun run build`, start `npx -y serve apps/marketing/dist -l 80`. **No auto-redeploy** — Coolify is home-LAN-only so the webhook doesn't reach it. Manual: `coolify deploy uuid bh4vnu32zwiu0bw6nf8d7yt8` from a LAN-connected machine.
- **Push relay (`push.exponential.at`)** — Coolify app `exponential-push-relay` (uuid `escnmp723si2642q1vcrmnqt`). Public-source app cloning `https://github.com/Niach/exponential.git` and building `Dockerfile.push-relay` (context `.`). Holds the `FIREBASE_SERVICE_ACCOUNT_JSON` env var. Same manual-deploy rule: `coolify deploy uuid escnmp723si2642q1vcrmnqt`.
- **Steer relay (`steer.exponential.at`) — (to create)** — planned Coolify app for `apps/steer-relay`, mirroring the push-relay setup: public-source app cloning the repo and building `Dockerfile.steer-relay` (context `.`). Needs `STEER_RELAY_SECRET` (must match the web app's env); once live, set `STEER_RELAY_URL=https://steer.exponential.at` on the web apps. Until created, remote start/steer stays off in cloud (unset `STEER_RELAY_URL` disables it gracefully).
- **Staging cloud (`next.exponential.at`)** — Coolify app `exponential-next-web` (uuid `i2h9ozcemp70yigkf8jylaq2`), same *dockerimage* from `ghcr.io/niach/exponential-web:latest`. Backed by `exponential-next-postgres` (uuid `mu6of6u8vul17sycib40zax8`) and `exponential-next-electric` (uuid `x80j1jdcf6zmviyh18d9b8iq`); attachments in Hetzner Object Storage bucket `exponentialnext`. Has Creem test-mode billing enabled. Deploy: `coolify deploy uuid i2h9ozcemp70yigkf8jylaq2`.
- **Self-host (on-prem)**: tag `vX.Y.Z` triggers `.gitea/workflows/build-release.yml` — builds the root `Dockerfile` (context `.`), pushes to the Gitea registry, then redeploys the Portainer stack.
- **Android**: tag `android-vX.Y.Z` triggers `.gitea/workflows/build-android.yml` — builds debug + release (unsigned) APKs and uploads them as artifacts. Signing for distribution still needs a keystore + signing config in `app/build.gradle.kts`.
- **Desktop**: tag `desktop-v*` (or manual `workflow_dispatch`) triggers `.github/workflows/build-desktop.yml` — a codegen-drift guard (regenerates the committed Rust files and fails on diff) then builds **two channels × two OSes**: `production` (→ `app.exponential.at`) and `staging` (→ `next.exponential.at`, built with `--features staging`, distinct app id `at.exponential.staging` so both can coexist). Channel is a compile-time cargo feature (`ui`'s `CLOUD_INSTANCE`, `app::channel`), the Rust analog of iOS `AppConstants.isStaging`. Linux artifacts are **AppImages** (`apps/desktop/scripts/build-appimage.sh` via linuxdeploy — bundles non-core libs incl. `libxkbcommon-x11`, leaves the GPU/GL/Wayland driver stack to the host; glibc floor = the runner's, currently ubuntu-22.04 → 2.35); macOS ships an unsigned raw binary. The app **self-registers the `exp://` deep-link handler on Linux at startup** (`app::desktop_integration`) pointing at `$APPIMAGE`/`current_exe()` — required because gpui's Linux backend never invokes `on_open_urls` (only macOS does); a `UnixDatagram` single-instance socket (`app::single_instance`, mirrors Zed's `open_listener`) forwards the browser callback into the running window instead of spawning a second one. macOS `.app` signing/notarization + Linux `.deb`/tarball are still manual (see checklist).

### Release-time checklist (not automated)

- **Android signing**: generate a keystore, add `signingConfigs` to `app/build.gradle.kts`, store the keystore + passwords as CI secrets; only then can store/distribution builds ship.
- **Desktop (Rust/gpui) distribution**: Linux AppImages are now built automatically (`build-appimage.sh` + linuxdeploy, both channels) — unsigned but installable. macOS `.app` still needs a Developer ID cert, real `codesign`, and `xcrun notarytool submit` (raw unsigned binary only from CI); Linux `.deb`/tarball not yet automated. AppImage URL-scheme registration is handled at runtime by the app's self-registration, not by a system installer.
- **iOS distribution**: no CI pipeline yet — archive via Xcode (`Exponential` scheme) and upload to TestFlight manually.
- **GitHub App webhook events**: the App must have the `Installation` event subscribed (App settings → Permissions & events) — the server syncs `github_installations` from installation created/unsuspend/suspend/deleted webhooks (HMAC-gated by `GITHUB_WEBHOOK_SECRET`).
- **Cloud launch env**: sign-up is **Google-only** — set `AUTH_PASSWORD_ENABLED=false`, `GOOGLE_LOGIN_ENABLED=true` + `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET`, and leave `AUTH_SIGNUP_ENABLED` unset/false (Google sign-in auto-creates accounts; `/auth/register` redirects to the Google-only login). `RESEND_API_KEY`/`EMAIL_FROM` stay configured for transactional mail, but the password reset/verification flows are inert without password auth. Staging (`next.exponential.at`) already runs this posture.
- `/api/health` gates the web Docker HEALTHCHECK (DB-backed; Electric reported but non-gating). The push relay's is `/healthz`.

DNS for `exponential.at` is on Cloudflare (zone-only, gray-cloud A records → Hetzner host) so Traefik's Let's Encrypt HTTP-01 challenge keeps working.

After schema changes, always: `bun run migrate:generate && bun run migrate`

Custom SQL triggers must be applied manually after migrations:

```bash
docker exec -i exponential-postgres-1 psql -U postgres -d exponential < apps/web/src/db/out/custom/0001_triggers.sql
```

## Pushing

Always use `git pushsync` instead of `git push` for this repo. It pushes
commits + tags to GitHub (`origin`) and then triggers the Gitea mirror
sync via the Gitea API so the home server starts building the new image
immediately. The alias lives in `.git/config` (not committed).

## Web App Structure (`apps/web/`)

High-level map (not exhaustive — trust the filesystem over this list):

```
apps/web/src/
├── components/
│   ├── ui/                       # shadcn components — always use these over raw HTML
│   ├── workspace/                # Sidebar, mobile topbar, settings sections (general/members/labels/projects/billing), plan-comparison
│   ├── onboarding/               # First-run wizard (project → GitHub repo connect; workspace auto-created)
│   ├── inbox/                    # Notifications inbox view
│   ├── issue-editor/, issue-properties/, issue-row-menu/, comment-rows/
│   ├── issue-list.tsx, issue-detail-view.tsx, issue-timeline.tsx, issue-search-sheet.tsx
│   ├── issue-filter-bar.tsx, issue-filter-popover.tsx, active-filter-pills.tsx
│   ├── create-issue-dialog.tsx, create-project-dialog.tsx, create-workspace-dialog.tsx
│   ├── diff-view.tsx, steer-terminal.tsx (xterm.js live-steer viewer over the steer relay)
│   └── github-repo-picker.tsx, recurrence-editor.tsx, subscribe-toggle.tsx, …
├── db/                           # schema.ts (re-exports @exp/db-schema + auth-schema), connection.ts, out/ (migrations + custom/0001_triggers.sql)
├── hooks/                        # use-session, use-workspace-data, use-my-issues-data, use-project-board-data, use-workspace-permissions, …
├── lib/
│   ├── auth/                     # Better Auth: index.ts (server), client.ts (fetchSessionOnce), config.ts, membership.ts, policies.ts, shape-where.ts, app-user.ts
│   ├── collections.ts            # Electric collection definitions (all use snakeCamelMapper)
│   ├── shape-route.ts            # createShapeRouteHandler — shared auth-gated shape proxy builder
│   ├── filters.ts                # IssueFilters, tab presets, matchesFilters()
│   ├── trpc.ts / trpc-client.ts  # tRPC server setup / client hooks
│   ├── trpc/                     # Routers: issues, projects, workspaces, labels, issue-labels, comments, notifications, subscriptions, workspace-members, workspace-invites, users, push-tokens, integrations, billing, admin, onboarding, repositories, coding-sessions, widgets, steer
│   ├── steer.ts                  # Pure core of the steer router: ticket claims, perm mapping, relay HTTP calls
│   ├── email.ts / email-unsubscribe.ts  # Single outbound-mail sender (Resend or SMTP; no-op when neither) + signed unsubscribe tokens
│   ├── integrations/             # mentions, notifications, fcm, activity, github-app, github-pr, pr-sync, subscriptions
│   └── storage/                  # S3 attachments: issue-attachments, issue-image-upload, image-dimensions, cleanup
├── routes/
│   ├── _authenticated/           # account/integrations, account/notifications (email prefs), onboarding, feedback, admin/*, integrations/github/installed
│   ├── w/$workspaceSlug/         # route.tsx (layout), index, my-issues/, inbox/, settings/, projects/$projectSlug/ (index + issues/$issueIdentifier full-page detail)
│   ├── auth/login.tsx, auth/register.tsx, invite/$token.tsx
│   ├── api/shapes/               # 14 Electric shape proxies (see Patterns)
│   ├── api/trpc/$.ts             # appRouter
│   ├── api/auth/$.ts, api/auth-config.ts, api/mcp.ts, api/webhooks/github.ts
│   ├── api/attachments/$attachmentId.ts, api/issues/$issueId/images.ts
│   └── api/mobile-oauth-start.ts / -return.ts, api/integrations/github/setup.ts, api/email/unsubscribe.ts, api/health.ts
├── router.tsx, start.tsx (defaultSsr: false), server.ts / server-bun.ts
└── styles.css                    # Tailwind v4 + shadcn dark theme (zinc OKLCH)
```

## Database

### Key Conventions

- Better Auth user IDs are `text` (not UUID) — all FKs to users must be `text` type
- All app tables use UUID PKs via `gen_random_uuid()`
- All tables have `created_at` / `updated_at` timestamps (with timezone)
- Sort order fields use `doublePrecision` (float8) for fractional indexing
- Rich text fields (issue description, comment body) are plain `text` GFM markdown
- Due date uses `date` type (no time component)

### Tables

`workspaces`, `projects`, `issues`, `labels`, `issue_labels`, `comments`, `attachments`, `coding_sessions`, `repositories`, `project_repositories`, `github_installations`, `workspace_members`, `workspace_invites`, `fcm_tokens`, `push_subscriptions`, `notifications`, `issue_subscribers`, `issue_events`, `user_notification_prefs`, `email_deliveries`, `widget_configs`, `widget_submissions` + Better Auth tables (users, sessions, accounts, verifications, apikeys)

### Key Issue Fields

`id`, `projectId`, `number`, `identifier`, `title`, `description` (text, GFM), `status`, `priority`, `assigneeId`, `creatorId`, `dueDate`, `sortOrder`, `completedAt`, `archivedAt`, `createdAt`, `updatedAt`, recurrence fields `recurrenceInterval` + `recurrenceUnit` (recurring issues: on completion the server spawns the next occurrence; intervals come from `domain-contract/contract.json`), `duplicateOfId` (self-FK, pairs with status `duplicate`), plus PR fields `prUrl`/`prNumber`/`prState`/`branch`/`prMergedAt` (one issue = one PR = one `exp/<IDENTIFIER>` branch)

### Enums

`issue_status` (backlog/todo/in_progress/done/cancelled/duplicate), `issue_priority` (none/urgent/high/medium/low), `notification_type` (incl. pr_opened/pr_merged), `workspace_member_role` (owner/member), `public_write_policy`, `recurrence_unit`, `pr_state`, `coding_session_status` (running/ended), `issue_event_type`, `subscriber_source` (incl. widget_reporter) — canonical values live in `packages/domain-contract/contract.json`

### Custom Triggers (0001_triggers.sql)

- `generate_issue_number()` — auto-increments `number` per project, sets `identifier` as `{prefix}-{number}`
- `update_updated_at()` — auto-updates `updated_at` on all tables
- `populate_issue_label_workspace_id()` / `populate_issue_child_workspace_id()` — denormalize `workspace_id` onto issue_labels / issue_subscribers / issue_events / coding_sessions so Electric shape filters stay workspace-scoped

## Patterns

### Electric Shape Proxies

Each synced table gets a shape proxy in `apps/web/src/routes/api/shapes/`, built with the shared `createShapeRouteHandler` (`lib/shape-route.ts`). The proxy authenticates the request, then forwards to Electric. A proxy may pin a server-side `columns` allowlist that clients cannot widen — `issue-subscribers` uses this to EXCLUDE the reporter `email` column from sync (widget-reporter PII stays server-only; the resolution-email path reads the DB directly). Client collections in `apps/web/src/lib/collections.ts` point to these proxy URLs. There is one proxy per synced table — workspaces, projects, issues, labels, issue-labels, users, workspace-members, workspace-invites, comments, attachments, notifications, issue-events, issue-subscribers, and coding-sessions (14 total, matching the 14 synced shapes).

Proxies are hardened three ways: (1) proxied responses always carry `cache-control: private, no-store` + `vary: authorization, x-api-key, cookie` — Electric's upstream `public, max-age=604800` must never reach auth-gated clients (it poisoned macOS URLCache with cross-auth snapshots); (2) a request presenting token credentials (`Authorization`/`x-api-key`) that fail to resolve gets an explicit 401 instead of degrading to the anonymous where clause (cookie-only requests still fall back anonymously — the web collection layer has no 401 recovery and the router guard re-auths on navigation); (3) `buildWhereClause` sorts id lists so the same id set always yields byte-identical SQL — the where clause is part of Electric's shape identity, and heap-order flips were rotating shape handles into native-client 409 loops.

### Electric Collections

All collections in `apps/web/src/lib/collections.ts` use `columnMapper: snakeCamelMapper()` from `@electric-sql/client` to map Postgres `snake_case` columns to JS `camelCase`. Without this, `useLiveQuery` `where` filters silently fail. Use `undefined` (not `false`) to skip a query; use `and()`/`or()` from `@tanstack/react-db` instead of JS `&&`/`||`.

### Auth Guard

`_authenticated.tsx` uses `beforeLoad` with `throw redirect()` to gate routes. The session is fetched once via `fetchSessionOnce()` (`lib/auth/client.ts`) and cached to avoid re-fetching on every navigation.

### tRPC + Electric Sync

Mutations go through tRPC. `generateTxId` captures the Postgres transaction ID so the client can wait for Electric to sync the write before updating the UI. Routers are modular in `apps/web/src/lib/trpc/` and combined in `api/trpc/$.ts` as `appRouter`.

### Issue List UX

- Issues are displayed in a CSS grid layout: `grid-cols-[24px_72px_24px_1fr_auto]` (priority, identifier, status, title, labels+due date)
- Rows are clickable to open the edit dialog; priority/status dropdowns use `stopPropagation` to prevent row click
- Empty status groups are hidden from the list
- Due dates display with a `CalendarDays` icon on the right side of rows

### Issue Filtering

- `apps/web/src/lib/filters.ts` defines `IssueFilters` (statuses, priorities, labelIds), tab presets (all/active/backlog), and `matchesFilters()`
- `IssueFilterBar` provides tab navigation and a filter popover button
- `IssueFilterPopover` offers multi-category drill-down filtering (status, priority, labels)
- `ActiveFilterPills` shows removable pills for active filters below the filter bar

### Issue Detail / Edit

- Issue rows navigate to the full-page detail route (`projects/$projectSlug/issues/$issueIdentifier`); the editor receives a live `issue` from Electric (stays fresh without refetching)
- Title and description use local state, save on blur if changed
- Status, priority, labels, and due date mutate immediately via tRPC
- Labels use `trpc.issueLabels.add` / `trpc.issueLabels.remove` for toggle behavior
- `completedAt` is auto-managed by the update mutation based on status changes

### Create Issue Dialog

- Supports title, description, status, priority, labels, and due date
- "Create more" checkbox keeps the dialog open and resets fields after creation
- Due date uses shadcn `Calendar` in a `Popover`

### UI Conventions

- All UI elements must use shadcn/ui components — no raw HTML `<input>`, `<button>`, `<textarea>`, `<label>` elements
- Borderless inputs inside dialogs: use `Input`/`Textarea` with `border-none shadow-none focus-visible:ring-0`
- Icon-only triggers in dropdowns: use `Button variant="ghost"` with `h-5 w-5 p-0`

## Environment Variables (.env)

```
DATABASE_URL                  # Postgres connection string (db name: exponential)
BETTER_AUTH_SECRET            # 32+ char secret for session signing
BETTER_AUTH_URL               # App base URL (http://localhost:5173)
BETTER_AUTH_TRUSTED_ORIGINS   # Comma-separated allowed origins
ELECTRIC_URL                  # Electric service URL (http://localhost:30000)
ELECTRIC_SOURCE_ID            # Electric Cloud source id (optional; unset for local/self-hosted)
ELECTRIC_SECRET               # Electric Cloud source secret (optional)
S3_ENDPOINT                   # S3-compatible storage URL (Garage default: http://localhost:3900)
S3_ACCESS_KEY                 # S3 access key (created by `bun run storage:init`)
S3_SECRET_KEY                 # S3 secret key (created by `bun run storage:init`)
S3_BUCKET                     # S3 bucket for attachments (default: exponential-attachments)
S3_REGION                     # S3 region label (default: garage)
AUTH_PASSWORD_ENABLED         # Enable email/password login (default: true)
AUTH_SIGNUP_ENABLED           # Public password sign-up ('true'/'false'; default: on in dev, OFF in production)
RESEND_API_KEY                # Resend API key — enables password reset + email verification (unset = email flows off)
EMAIL_FROM                    # Verified sender, e.g. "Exponential <noreply@exponential.at>"
SMTP_HOST                     # SMTP alternative to Resend for ALL outgoing mail (self-host; Resend wins when both set)
SMTP_PORT                     # SMTP port (default 587)
SMTP_USER                     # SMTP auth user (optional)
SMTP_PASS                     # SMTP auth password (optional)
SMTP_SECURE                   # 'true' for implicit TLS (port 465)
OIDC_PROVIDERS                # JSON array of OIDC providers — the primary OIDC mechanism (see .env.example)
# Legacy single-provider OIDC (used only when OIDC_PROVIDERS is unset):
AUTH_OIDC_ENABLED             # Enable legacy single-provider OIDC (default: false)
OIDC_CLIENT_ID                # OAuth2 client ID
OIDC_CLIENT_SECRET            # OAuth2 client secret
OIDC_DISCOVERY_URL            # OIDC discovery endpoint URL
OIDC_PROVIDER_ID              # Provider ID for Better Auth (default: authentik)
GOOGLE_CLIENT_ID              # Google OAuth client ID (required for Google login)
GOOGLE_CLIENT_SECRET          # Google OAuth client secret
GITHUB_APP_ID                 # GitHub App numeric ID (install from /account/integrations or workspace settings → Repositories; server mints per-repo installation tokens; github_installations rows come from the setup redirect, installation webhooks, and an empty-table listAppInstallations() self-heal)
GITHUB_APP_SLUG               # GitHub App URL slug (builds the install link)
GITHUB_APP_PRIVATE_KEY        # GitHub App PEM private key, base64-encoded (base64 -w0 app.private-key.pem)
GITHUB_WEBHOOK_SECRET         # GitHub App webhook HMAC secret (cloud PR-merge detection; App webhook → ${BETTER_AUTH_URL}/api/webhooks/github)
GITHUB_POLLING                # 'true' to run the outbound merge cron (self-hosted behind NAT, unreachable by webhook); decoupled from SELF_HOSTED
GOOGLE_LOGIN_ENABLED          # Show "Sign in with Google" on login/register (default: false)
SELF_HOSTED                   # 'true' for self-hosted (disables billing, unlocks plan limits)
CREEM_API_KEY                 # Creem billing API key (cloud-only)
CREEM_WEBHOOK_SECRET          # Creem webhook signing secret (cloud-only)
CREEM_PRO_PRODUCT_ID          # Creem product ID for the Pro plan
CREEM_BUSINESS_PRODUCT_ID     # Creem product ID for the Business plan
PUSH_RELAY_URL                # URL of the push-relay service (e.g. https://push.yourapp.com)
PUSH_RELAY_SECRET             # Shared secret between web app and push relay
STEER_RELAY_URL               # URL of the steer relay (unset = remote start/steer off; LAN URLs fine — all connections dial OUT)
STEER_RELAY_SECRET            # Shared HS256 secret: web mints steer tickets, relay verifies (must match the relay process env)
SECURITY_HEADERS_ENABLED      # 'true' to emit CSP/HSTS etc. from the Bun server
INITIAL_ADMIN_EMAILS          # Comma-separated emails auto-promoted to global admin at startup
FEEDBACK_WIDGET_SCRIPT_URL    # Self-hosted only: cloud loader URL for the in-app feedback widget (cloud derives from DB)
FEEDBACK_WIDGET_KEY           # Self-hosted only: expw_ key of the cloud feedback widget config
WIDGET_RATE_LIMIT_PER_KEY_HOURLY # Widget submit limit per public key (default 60/h, burst 10 via WIDGET_RATE_LIMIT_KEY_BURST)
WIDGET_RATE_LIMIT_PER_IP_HOURLY  # Widget submit limit per client IP (default 60/h, burst 5 via WIDGET_RATE_LIMIT_IP_BURST)
```

(The push relay process itself reads `FIREBASE_SERVICE_ACCOUNT_JSON` — see `.env.example`.)

## Integrations

### Coding flow (v2 — "Start coding" launcher)

The old Rust `agent-core` runtime, the companion daemon, the synthetic desktop-agent identity (`agent_registrations`, `role=agent`, `expk_` keys), the `agent_runs` plan/approval state machine, and the `assigned-issues` shape are **all deleted**. The coding flow is a thin **launcher inside the desktop IDE** (`apps/desktop`, Rust): resolve the issue's repo from the workspace `repositories` registry (tRPC) → mint a session-gated JIT GitHub-App installation token → create a git worktree + `exp/<IDENTIFIER>` branch with a token-embedded remote → write `.mcp.json` pointing at the web `/api/mcp` (authenticated with the user's personal `expu_` Better Auth apikey) → spawn `claude --dangerously-skip-permissions` in the embedded terminal (alacritty_terminal-backed), seeded with a plan-first prompt. Claude commits, pushes, and opens its own PR via the MCP `open_pr` tool (the server opens + links the PR through the GitHub App). Local deps are only `claude` + `git` — never `gh`. A slim synced `coding_sessions` row (`running`/`ended`) powers the cross-client "coding now" badge. The person coding is the **real signed-in user**. The server-side spec is `docs/masterplan.md` (§3 steer relay, §7 GitHub/repositories); the gpui desktop IDE that hosts the launcher is specced in `docs/masterplan-v3.md` (§6 terminal, §7 IDE features, §8 steer).

### Embeddable Feedback Widget

A marker.io-style widget third parties paste into their sites via a GA-style async `<script>` snippet. Source lives in `packages/widget` (Preact in a shadow root + `@zumer/snapdom` for client-side screenshots); the build emits two classic IIFE scripts into `apps/web/public/widget/v1/` — `loader.js` (snippet target: command queue, floating button, config prefetch) and `widget.js` (lazy-loaded panel + capture), plus `demo.html` for manual testing. The widget API is `window.ExponentialWidget`: `init({key})`, `identify({email,name,userId})`, `setCustomData({...})`, `open()`, `close()`. Screenshots are captured client-side (snapDOM, viewport-cropped, WebP→PNG→JPEG ladder, never blocks submission on failure); marker.io's server-side rendering approach was researched and deliberately not used (capture sits behind a `CaptureEngine` interface in `packages/widget/src/capture/` if that ever changes). Screenshots can be **annotated** in a full-screen editor (rectangle / free line / arrow, fixed red, undo/clear): shapes live in image-pixel space (`packages/widget/src/annotate/`), stay editable across editor reopens, and are flattened into the uploaded image on submit (`flattenAnnotations` re-runs the encode ladder) — the server only ever sees one plain image.

Server side: two server-only tables (`widget_configs` with the public `expw_` key + domain allowlist, `widget_submissions` with reporter email/page/env metadata — NOT Electric-synced, read via the `widgets` tRPC router), two CORS-handling public routes (`/api/widget/config`, `/api/widget/submit` — plain file routes; CORS/origin/rate-limit/honeypot helpers in `apps/web/src/lib/widget/`). Each config owns a synthetic `isAgent` user (role=agent member) as the issue creator — **never delete that user: `issues.creator_id` cascades**. Submissions create the issue + screenshot attachment + submission row in ONE transaction; the description gets a human-readable metadata block. Rate limiting is in-process token buckets (`WIDGET_RATE_LIMIT_*` env). Managed in workspace settings → "Feedback widget" (owner-only, copy-paste snippet). Dogfood: cloud bootstrap creates the `Exponential App` config on the public feedback workspace, which holds a SINGLE project — Exponential (slug `exponential`, prefix `EXP`); bootstrap heals pre-collapse DBs by creating that project if missing, repointing widget configs to it, and deleting the legacy `feedback` project only when it has zero issues. The sidebar FeedbackButton opens the embedded widget (`FeedbackWidgetProvider` in the workspace layout) and falls back to the legacy `/feedback` redirect (now → projects/exponential); self-hosted instances point at the cloud via `FEEDBACK_WIDGET_SCRIPT_URL` + `FEEDBACK_WIDGET_KEY` (the script origin is auto-added to the CSP).

Dev-mode gotcha: the nitro-alpha dev bridge renders any app 404-status response as a connect `Cannot GET/POST` HTML page and strips custom response headers — both dev-only; production (`server-bun.ts`/srvx) passes responses through untouched.

## Style Conventions

- Template literals for strings (backticks, not quotes)
- Functional components, no class components
- shadcn/ui components in `src/components/ui/` — always use these over raw HTML elements
- Icons from `lucide-react`
- Business logic components in `src/components/` (not in `ui/`)
