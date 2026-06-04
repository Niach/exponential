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
- **Infrastructure**: Docker Compose — Postgres:54321, Electric:30000, Garage:3900 (S3-compatible), Caddy:3000 (HTTP/2 reverse proxy; `Caddyfile` is gitignored — copy from `Caddyfile.example`, which is configured for Electric long-poll timeouts)
- **Package Manager**: bun

## Monorepo Layout

```
exponential/
├── apps/
│   ├── web/        # TanStack Start app (the issue tracker)
│   ├── push-relay/ # Standalone push notification relay (Hono/Bun, separately deployed)
│   ├── marketing/  # Marketing site (Vite + React, deployed via Coolify)
│   ├── ios/        # Native SwiftUI iOS app (Tuist + GRDB)
│   ├── android/    # Native Kotlin / Jetpack Compose app
│   └── linux/      # Native Zig + GTK4 desktop app (own sync engine; embeds libghostty + the Rust agent-core to run coding agents — replaces the old companion daemon)
├── crates/
│   └── agent-core/ # Rust cdylib (C ABI): the shared agent loop (dispatcher/pipeline/electric/mcp/git/github/pr-poll), driven by the desktop apps
├── packages/
│   ├── db-schema/          # Drizzle schema + shared zod/domain types
│   ├── domain-contract/    # contract.json — canonical enum values; emits per-language constants
│   ├── electric-protocol/  # Electric SQL shape protocol fixtures
│   └── tsconfig/           # Shared TS configs
├── docker-compose.yaml
├── Caddyfile
├── Dockerfile              # Builds the web app image; build context = repo root
├── Dockerfile.push-relay   # Builds the push relay image; build context = repo root
└── package.json    # bun workspaces, dispatcher scripts
```

Workspace package names: `@exp/web`, `@exp/push-relay`, `@exp/marketing`, `@exp/db-schema`, `@exp/domain-contract`, `@exp/electric-protocol`, `@exp/tsconfig`. (The Linux desktop app `apps/linux` is a Zig project and the Rust `crates/agent-core` is a Cargo crate — neither is a bun workspace.)

**Mobile parity:** iOS and Android sync the same thirteen Electric shapes the web client uses (workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, **comments**, **attachments**, **notifications**, **issue_events**, **issue_subscribers**). All three clients honor `isPublic` / `publicWritePolicy` field gating via a small `WorkspacePermissions` helper that mirrors `apps/web/src/hooks/use-workspace-permissions.ts`. When changing enum values in `packages/db-schema/src/domain.ts`, also update `packages/domain-contract/contract.json` and run `bun run --filter @exp/domain-contract generate` to refresh the Swift / Kotlin constants.

**Description / comment markdown contract:** `issues.description` and `comments.body` are jsonb `{ text: "<markdown>" }`. The markdown is **GFM** — the single interchange contract across web (TipTap + tiptap-markdown), iOS (cmark-gfm), and Android (compose-rich-editor). Supported, round-trippable features: bold, italic, strikethrough, inline code, headings (H1–H3 editable), bullet/ordered lists, **task lists** (`- [ ]`/`- [x]`), blockquote, code blocks, links, **block/full-width inline images**, and **@mentions**. **Underline is intentionally unsupported** (no GFM representation — it does not round-trip); tables, slash commands, and image resize are intentionally out of scope. **Mentions** are written as `@<email>` in the markdown source (the single interchange form — round-trips as plain GFM text); the server resolves `@email` to workspace members and fires `issue_mention` notifications + auto-subscribes them (`apps/web/src/lib/integrations/mentions.ts`). Clients render a known member's `@email` as a name pill; the web editor offers an @-autocomplete that inserts the `@email` form. Embedded images are always stored as the relative form `![alt](/api/attachments/{id})`; the server canonicalizes image URLs to relative on save (`canonicalizeMarkdownImageUrls` in `apps/web/src/lib/storage/issue-attachments.ts`), and clients resolve to absolute only at fetch time. The iOS editor is block-based: `IssueEditorModel` (an `@Observable`) owns `[ContentBlock]` as the single source of truth and derives markdown only at save; image upload is atomic (all-or-nothing) and concurrent. Attachments carry probed `width`/`height` so clients can pre-size and avoid layout shift.

## Commands

All commands run from the repo root unless noted.

```bash
bun install                        # Install workspace deps (run from root)
bun dev                            # Start web dev server (apps/web, localhost:5173)
bun run dev:marketing              # Start marketing dev server (apps/marketing)
bun run dev:push-relay             # Start push relay dev server (apps/push-relay, localhost:4001)
bun run start:push-relay           # Start push relay in production mode
bun run build                      # Build web + marketing
bun run build:web                  # Build only the web app
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

bun run android:build              # ./gradlew :app:assembleDebug in apps/android
bun run android:install            # ./gradlew :app:installDebug

bun run --filter @exp/domain-contract generate  # Regenerate iOS + Android enum constants
```

You can also run a script directly inside a workspace: `bun --filter @exp/web <script>` or `cd apps/web && bun run <script>`.

## Deploys

Three production targets run on Coolify (`coolify.home.straehhuber.com`, Hetzner host `46.225.140.133`), one self-host target on the home Portainer stack.

- **Web cloud (`app.exponential.at`)** — Coolify app `exponential-web` (uuid `hzoe7vty1rzjypyymsaqw2w6`), a *dockerimage* app pulling `ghcr.io/niach/exponential-web:latest`. The image is built by `.github/workflows/build-issues-web.yml` on every push to `master` and on `v*.*.*` / `v*.*.*-dev` tags. **Coolify is home-LAN-only, so there is no auto-redeploy webhook** — after a green Actions run, redeploy manually from a LAN-connected machine: `coolify deploy uuid hzoe7vty1rzjypyymsaqw2w6` (or click "Deploy" in the Coolify UI). Backed by `exponential-postgres` (uuid `hqc1ofbam3x5kyxjexwj1oio`) and `exponential-electric` (uuid `s12y6uvto3utdsan5mrkhjjp`); attachments live in Hetzner Object Storage bucket `exponential` at `nbg1.your-objectstorage.com`.
- **Marketing (`exponential.at`)** — Coolify app `exponential-marketing` (uuid `bh4vnu32zwiu0bw6nf8d7yt8`). Public-source app cloning `https://github.com/Niach/exponential.git`, base directory `/`, build `cd apps/marketing && bun run build`, start `npx -y serve apps/marketing/dist -l 80`. **No auto-redeploy** — Coolify is home-LAN-only so the webhook doesn't reach it. Manual: `coolify deploy uuid bh4vnu32zwiu0bw6nf8d7yt8` from a LAN-connected machine.
- **Push relay (`push.exponential.at`)** — Coolify app `exponential-push-relay` (uuid `escnmp723si2642q1vcrmnqt`). Public-source app cloning `https://github.com/Niach/exponential.git` and building `Dockerfile.push-relay` (context `.`). Holds the `FIREBASE_SERVICE_ACCOUNT_JSON` env var. Same manual-deploy rule: `coolify deploy uuid escnmp723si2642q1vcrmnqt`.
- **Staging cloud (`next.exponential.at`)** — Coolify app `exponential-next-web` (uuid `i2h9ozcemp70yigkf8jylaq2`), same *dockerimage* from `ghcr.io/niach/exponential-web:latest`. Backed by `exponential-next-postgres` (uuid `mu6of6u8vul17sycib40zax8`) and `exponential-next-electric` (uuid `x80j1jdcf6zmviyh18d9b8iq`); attachments in Hetzner Object Storage bucket `exponentialnext`. Has Creem test-mode billing enabled. Deploy: `coolify deploy uuid i2h9ozcemp70yigkf8jylaq2`.
- **Self-host (on-prem)**: tag `vX.Y.Z` triggers `.gitea/workflows/build-release.yml` — builds the root `Dockerfile` (context `.`), pushes to the Gitea registry, then redeploys the Portainer stack.
- **Android**: tag `android-vX.Y.Z` triggers `.gitea/workflows/build-android.yml` — builds debug + release (unsigned) APKs and uploads them as artifacts. Signing for distribution still needs a keystore + signing config in `app/build.gradle.kts`.

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

```
apps/web/src/
├── components/
│   ├── ui/                      # shadcn components (button, input, card, sidebar, dialog, calendar, etc.)
│   ├── create-issue-dialog.tsx   # Issue creation (title, description, status, priority, labels, due date)
│   ├── edit-issue-dialog.tsx     # Issue editing (live data via Electric, save-on-blur, inline mutations)
│   ├── create-project-dialog.tsx # Project creation (name, prefix, color)
│   ├── issue-list.tsx            # CSS grid issue list, grouped by status, row-click to edit
│   ├── issue-filter-bar.tsx      # Tab-based filtering (All Issues, Active, Backlog) + filter popover
│   ├── issue-filter-popover.tsx  # Multi-category filter popover (status, priority, labels)
│   ├── active-filter-pills.tsx   # Removable filter pills below filter bar
│   ├── label-picker.tsx          # Popover multi-select label picker with inline create
│   ├── status-dropdown.tsx       # Issue status dropdown (exports statuses, StatusIcon, getStatusConfig)
│   └── priority-dropdown.tsx     # Issue priority dropdown (exports priorities, PriorityIcon, getPriorityConfig)
├── db/
│   ├── schema.ts                # Full app schema (re-exports auth-schema)
│   ├── auth-schema.ts           # Better Auth managed tables (users, sessions, accounts, verifications)
│   ├── connection.ts            # Drizzle pg connection
│   └── out/
│       ├── custom/              # Custom SQL (triggers)
│       └── *.sql                # Generated Drizzle migrations
├── hooks/
│   └── use-mobile.ts            # useIsMobile() — detects mobile breakpoint (768px)
├── lib/
│   ├── auth/                    # Better Auth: index.ts (server config), client.ts (fetchSessionOnce session cache), config.ts (getAuthConfig), membership.ts, policies.ts, shape-where.ts
│   ├── collections.ts           # Electric collection definitions (all use snakeCamelMapper)
│   ├── electric-proxy.ts        # Shape proxy helpers (prepareElectricUrl, proxyElectricRequest)
│   ├── filters.ts               # Issue filter types, tab presets, matchesFilters(), activeFilterCount()
│   ├── trpc.ts                  # tRPC server setup (router, authedProcedure, generateTxId)
│   ├── trpc-client.ts           # tRPC client hooks
│   ├── trpc/                    # Modular tRPC routers
│   │   ├── issues.ts            #   create (with dueDate, labels), update (with dueDate, completedAt auto-management)
│   │   ├── projects.ts          #   create (auto-generates slug), update
│   │   ├── workspaces.ts        #   ensureDefault
│   │   ├── labels.ts            #   create
│   │   └── issue-labels.ts      #   add, remove
│   └── utils.ts                 # cn() utility
├── routes/
│   ├── __root.tsx               # Root layout (dark HTML, Inter font, TooltipProvider)
│   ├── _authenticated.tsx       # Auth guard (redirect to /auth/login if no session)
│   ├── w/$workspaceSlug/        # Top-level workspace route (own beforeLoad session gating)
│   │   ├── route.tsx            # Workspace layout (shadcn Sidebar, project nav, user dropdown)
│   │   └── projects/$projectSlug/index.tsx  # Project page (issue list, filtering, create/edit dialogs)
│   ├── auth/login.tsx           # Login page
│   ├── auth/register.tsx        # Register page
│   ├── api/auth/$.ts            # Better Auth handler
│   ├── api/trpc/$.ts            # tRPC handler (combines all routers into appRouter)
│   ├── api/shapes/              # Electric shape proxies (auth-gated)
│   │   ├── workspaces.ts
│   │   ├── projects.ts
│   │   ├── issues.ts
│   │   ├── labels.ts
│   │   └── issue-labels.ts
│   └── index.tsx                # Redirects to /w/default
├── router.tsx                   # TanStack Router config (defaultPreload: 'viewport', scrollRestoration)
├── start.tsx                    # TanStack Start instance (defaultSsr: false)
├── server.ts                    # Server entry point
└── styles.css                   # Tailwind v4 + shadcn dark theme (zinc OKLCH)
```

## Database

### Key Conventions

- Better Auth user IDs are `text` (not UUID) — all FKs to users must be `text` type
- All app tables use UUID PKs via `gen_random_uuid()`
- All tables have `created_at` / `updated_at` timestamps (with timezone)
- Sort order fields use `doublePrecision` (float8) for fractional indexing
- Rich text fields (issue description, comment body) use `jsonb`
- Due date uses `date` type (no time component)

### Tables

`workspaces`, `projects`, `issues`, `labels`, `issue_labels`, `issue_relations`, `comments`, `attachments`, `workspace_members`, `workspace_invites`, `workspace_agents`, `fcm_tokens`, `push_subscriptions`, `notifications`, `issue_subscribers`, `issue_events` + Better Auth tables (users, sessions, accounts, verifications, apikeys)

### Key Issue Fields

`id`, `projectId`, `number`, `identifier`, `title`, `description` (jsonb), `status`, `priority`, `assigneeId`, `creatorId`, `dueDate`, `sortOrder`, `completedAt`, `archivedAt`, `createdAt`, `updatedAt`, plus agent-plan fields `agentPlanState`, `agentPlanRevision`, `agentPlanApprovedAt`

### Enums

`issue_status` (backlog/todo/in_progress/done/cancelled), `issue_priority` (none/urgent/high/medium/low), `issue_relation_type`, `notification_type`

### Custom Triggers (0001_triggers.sql)

- `generate_issue_number()` — auto-increments `number` per project, sets `identifier` as `{prefix}-{number}`
- `update_updated_at()` — auto-updates `updated_at` on all tables

## Patterns

### Electric Shape Proxies

Each synced table gets a shape proxy in `apps/web/src/routes/api/shapes/`, built with the shared `createShapeRouteHandler` (`lib/shape-route.ts`). The proxy authenticates the request, then forwards to Electric. Client collections in `apps/web/src/lib/collections.ts` point to these proxy URLs. There is one proxy per synced table — currently workspaces, projects, issues, labels, issue-labels, users, workspace-members, workspace-invites, comments, attachments, notifications, issue-events, issue-subscribers, and assigned-issues.

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

### Edit Issue Dialog

- Opens on row click, receives live `issue` prop from Electric (stays fresh without refetching)
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
OIDC_PROVIDERS                # JSON array of OIDC providers — the primary OIDC mechanism (see .env.example)
# Legacy single-provider OIDC (used only when OIDC_PROVIDERS is unset):
AUTH_OIDC_ENABLED             # Enable legacy single-provider OIDC (default: false)
OIDC_CLIENT_ID                # OAuth2 client ID
OIDC_CLIENT_SECRET            # OAuth2 client secret
OIDC_DISCOVERY_URL            # OIDC discovery endpoint URL
OIDC_PROVIDER_ID              # Provider ID for Better Auth (default: authentik)
MCP_API_TOKEN                 # Static Bearer token for the MCP server
MCP_USER_EMAIL                # MCP tool calls run as this user
GOOGLE_CLIENT_ID              # Google OAuth client ID (required for login or Calendar)
GOOGLE_CLIENT_SECRET          # Google OAuth client secret
GOOGLE_LOGIN_ENABLED          # Show "Sign in with Google" on login/register (default: false)
GOOGLE_CALENDAR_ENABLED       # Enable Google Calendar integration (default: false)
SELF_HOSTED                   # 'true' for self-hosted (disables billing, unlocks plan limits)
CREEM_API_KEY                 # Creem billing API key (cloud-only)
CREEM_WEBHOOK_SECRET          # Creem webhook signing secret (cloud-only)
CREEM_PRO_PRODUCT_ID          # Creem product ID for the Pro plan
CREEM_BUSINESS_PRODUCT_ID     # Creem product ID for the Business plan
PUSH_RELAY_URL                # URL of the push-relay service (e.g. https://push.yourapp.com)
PUSH_RELAY_SECRET             # Shared secret between web app and push relay
```

## Integrations

### Google Calendar

Per-user opt-in. User connects via `/account/integrations` (sidebar user dropdown → Integrations). Linking uses Better Auth's `authClient.linkSocial({ provider: 'google', scopes: ['https://www.googleapis.com/auth/calendar.events'] })`. Tokens are stored in the existing `accounts` table with `providerId='google'`; access tokens are auto-refreshed via `auth.api.getAccessToken`.

Sync logic is in `src/lib/google-calendar.ts` and is invoked via `fireAndForgetSync` / `fireAndForgetDelete` from `src/lib/trpc/issues.ts` after each create/update/delete commits. Failures are logged and persisted to `issues.googleCalendarLastSyncError` but never block the mutation. The sync is one-way (issue → calendar) and writes all-day events to the user's primary calendar:

- Issue has `dueDate` AND status not in `done`/`cancelled` AND not archived → event exists
- Otherwise → no event
- `issues.googleCalendarEventId` tracks the synced event ID

### Desktop Agent

The desktop apps register the machine as an "agent member" and run a coding agent (`claude` / `codex` CLI) against issues assigned to it. This replaces the old `apps/companion` daemon (removed): the agent loop is now the Rust `crates/agent-core` (cdylib, C ABI — dispatcher/pipeline/electric/mcp/git/github/pr-poll), driven by the native desktop app (Linux: `apps/linux`, Zig + GTK4), and the agent's CLI session runs in an embedded **libghostty** terminal so the user can watch and steer it.

The full roadmap (locked architecture, the shared agent-core C ABI + run-request protocol, libghostty notes, and the sequenced macOS plan) is in `docs/native-desktop-roadmap.md`.

An owner adds an agent member in workspace settings (persisted in `workspace_agents`, linked to an `apikeys` row and a `users` row, gated by `setupTokenHash`) and registers a machine via the app (`companion.create` → `claimSetup`; the server tRPC routes kept their `companion.*` names). The agent watches assigned issues over Electric (`expk_` key), runs in a git worktree, and opens a GitHub PR. Every event (plan ready, questions, PR opened, errors) is also written as an issue comment, so the FCM push pipeline notifies the owner. Plan approval flows through the `agentPlanState` / `agentPlanRevision` / `agentPlanApprovedAt` issue fields; server logic lives in `lib/trpc/agent-plan.ts` and `lib/trpc/companion/`. v1 runs the agent only while the desktop app is open (no headless/systemd mode).

## Style Conventions

- Template literals for strings (backticks, not quotes)
- Functional components, no class components
- shadcn/ui components in `src/components/ui/` — always use these over raw HTML elements
- Icons from `lucide-react`
- Business logic components in `src/components/` (not in `ui/`)
