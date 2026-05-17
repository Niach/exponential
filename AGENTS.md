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
- **Infrastructure**: Docker Compose — Postgres:54321, Electric:30000, Garage:3900 (S3-compatible), Caddy:3000 (HTTP/2 reverse proxy)
- **Package Manager**: bun

## Monorepo Layout

```
exponential/
├── apps/
│   ├── web/        # TanStack Start app (the issue tracker)
│   ├── marketing/  # Marketing site (Vite + React, deployed via Coolify)
│   └── android/    # Native Kotlin / Jetpack Compose app (in progress)
├── packages/       # Shared packages (db-schema, api-contracts, …)
├── docker-compose.yaml
├── Caddyfile
├── Dockerfile      # Builds the web app image; build context = repo root
└── package.json    # bun workspaces, dispatcher scripts
```

Workspace package names: `@exp/web`, `@exp/marketing` (and future `@exp/db-schema`, etc.).

## Commands

All commands run from the repo root unless noted.

```bash
bun install                        # Install workspace deps (run from root)
bun dev                            # Start web dev server (apps/web, localhost:5173)
bun run dev:marketing              # Start marketing dev server (apps/marketing)
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
```

You can also run a script directly inside a workspace: `bun --filter @exp/web <script>` or `cd apps/web && bun run <script>`.

## Deploys

- **Web**: tag `vX.Y.Z` triggers `.gitea/workflows/build-release.yml` — builds the root `Dockerfile` (context `.`), pushes to the Gitea registry, then redeploys the Portainer stack.
- **Marketing**: Coolify watches the repo. After the monorepo move the Coolify app's source/base directory is `apps/marketing/` (was `marketing/`). The Coolify start command must not include `serve -s`.
- **Android**: tag `android-vX.Y.Z` triggers `.gitea/workflows/build-android.yml` — builds debug + release (unsigned) APKs and uploads them as artifacts. Signing for distribution still needs a keystore + signing config in `app/build.gradle.kts`.

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
│   ├── auth.ts                  # Better Auth server config (genericOAuth plugin for OIDC)
│   ├── auth-client.ts           # Better Auth client + authStateCollection (genericOAuthClient plugin)
│   ├── auth-config.ts           # Server function exposing auth settings to client
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
│   ├── _authenticated/w/$workspaceSlug/
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

`workspaces`, `projects`, `issues`, `labels`, `issue_labels`, `issue_relations`, `comments`, `attachments`, `views`, `push_subscriptions`, `notifications` + Better Auth tables (users, sessions, accounts, verifications)

### Key Issue Fields

`id`, `projectId`, `number`, `identifier`, `title`, `description` (jsonb), `status`, `priority`, `assigneeId`, `creatorId`, `dueDate`, `sortOrder`, `completedAt`, `archivedAt`, `createdAt`, `updatedAt`

### Enums

`issue_status` (backlog/todo/in_progress/done/cancelled), `issue_priority` (none/urgent/high/medium/low), `issue_relation_type`, `notification_type`

### Custom Triggers (0001_triggers.sql)

- `generate_issue_number()` — auto-increments `number` per project, sets `identifier` as `{prefix}-{number}`
- `update_updated_at()` — auto-updates `updated_at` on all tables

## Patterns

### Electric Shape Proxies

Each synced table gets a shape proxy in `apps/web/src/routes/api/shapes/`. The proxy authenticates the request, then forwards to Electric. Client collections in `apps/web/src/lib/collections.ts` point to these proxy URLs. Current proxies: workspaces, projects, issues, labels, issue-labels.

### Electric Collections

All collections in `apps/web/src/lib/collections.ts` use `columnMapper: snakeCamelMapper()` from `@electric-sql/client` to map Postgres `snake_case` columns to JS `camelCase`. Without this, `useLiveQuery` `where` filters silently fail. Use `undefined` (not `false`) to skip a query; use `and()`/`or()` from `@tanstack/react-db` instead of JS `&&`/`||`.

### Auth Guard

`_authenticated.tsx` uses `beforeLoad` with `throw redirect()` to gate routes. Session is cached in `authStateCollection` (local-only collection) to avoid re-fetching on every navigation.

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
DATABASE_URL                  # Postgres connection string
BETTER_AUTH_SECRET            # 32+ char secret for session signing
BETTER_AUTH_URL               # App base URL (http://localhost:5173)
BETTER_AUTH_TRUSTED_ORIGINS   # Comma-separated allowed origins
ELECTRIC_URL                  # Electric service URL (http://localhost:30000)
S3_ENDPOINT                   # S3-compatible storage URL (Garage default: http://localhost:3900)
S3_ACCESS_KEY                 # S3 access key (created by `bun run storage:init`)
S3_SECRET_KEY                 # S3 secret key (created by `bun run storage:init`)
S3_BUCKET                     # S3 bucket for attachments (default: exponential-attachments)
S3_REGION                     # S3 region label (default: garage)
AUTH_OIDC_ENABLED             # Enable OIDC login (default: false)
AUTH_PASSWORD_ENABLED         # Enable email/password login (default: true)
OIDC_CLIENT_ID                # OAuth2 client ID
OIDC_CLIENT_SECRET            # OAuth2 client secret
OIDC_DISCOVERY_URL            # OIDC discovery endpoint URL
OIDC_PROVIDER_ID              # Provider ID for Better Auth (default: authentik)
GOOGLE_CLIENT_ID              # Google OAuth client ID (required for login or Calendar)
GOOGLE_CLIENT_SECRET          # Google OAuth client secret
GOOGLE_LOGIN_ENABLED          # Show "Sign in with Google" on login/register (default: false)
GOOGLE_CALENDAR_ENABLED       # Enable Google Calendar integration (default: false)
FIREBASE_SERVICE_ACCOUNT_JSON # Firebase service account key (JSON string) for FCM push delivery
```

## Integrations

### Google Calendar

Per-user opt-in. User connects via `/account/integrations` (sidebar user dropdown → Integrations). Linking uses Better Auth's `authClient.linkSocial({ provider: 'google', scopes: ['https://www.googleapis.com/auth/calendar.events'] })`. Tokens are stored in the existing `accounts` table with `providerId='google'`; access tokens are auto-refreshed via `auth.api.getAccessToken`.

Sync logic is in `src/lib/google-calendar.ts` and is invoked via `fireAndForgetSync` / `fireAndForgetDelete` from `src/lib/trpc/issues.ts` after each create/update/delete commits. Failures are logged and persisted to `issues.googleCalendarLastSyncError` but never block the mutation. The sync is one-way (issue → calendar) and writes all-day events to the user's primary calendar:

- Issue has `dueDate` AND status not in `done`/`cancelled` AND not archived → event exists
- Otherwise → no event
- `issues.googleCalendarEventId` tracks the synced event ID

## Style Conventions

- Template literals for strings (backticks, not quotes)
- Functional components, no class components
- shadcn/ui components in `src/components/ui/` — always use these over raw HTML elements
- Icons from `lucide-react`
- Business logic components in `src/components/` (not in `ui/`)
