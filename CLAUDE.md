# Exponential

Real-time issue tracker.

## Tech Stack

- **Framework**: TanStack Start (React 19, TanStack Router, TanStack React DB)
- **Database**: PostgreSQL 17 via Drizzle ORM (`snake_case` casing)
- **Real-time**: ElectricSQL (shape proxy pattern via `@tanstack/electric-db-collection`)
- **Auth**: Better Auth (email/password, session-based, `tanstackStartCookies` plugin)
- **API**: tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync)
- **UI**: shadcn/ui on Tailwind v4 (OKLCH zinc palette, dark theme forced via `html.dark`)
- **Date Picker**: `react-day-picker` + `date-fns` via shadcn `Calendar` component
- **Infrastructure**: Docker Compose — Postgres:54321, Electric:30000, MinIO:9000/9001, Caddy:3000 (HTTP/2 reverse proxy)
- **Package Manager**: bun

## Commands

```bash
bun dev                    # Start dev server (localhost:5173)
docker compose up           # Start Postgres + Electric + MinIO
docker compose down         # Stop infrastructure
docker compose down -v      # Stop + wipe volumes
bun drizzle-kit generate   # Generate migrations from schema changes
bun drizzle-kit migrate    # Apply migrations
bun run build              # Production build
bun lint                   # ESLint fix
bun format                 # Prettier format
```

After schema changes, always: `bun drizzle-kit generate && bun drizzle-kit migrate`

Custom SQL triggers must be applied manually after migrations:

```bash
docker exec -i exponential-postgres-1 psql -U postgres -d exponential < src/db/out/custom/0001_triggers.sql
```

## Project Structure

```
src/
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
│   ├── auth.ts                  # Better Auth server config
│   ├── auth-client.ts           # Better Auth client + authStateCollection (local-only session cache)
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

Each synced table gets a shape proxy in `src/routes/api/shapes/`. The proxy authenticates the request, then forwards to Electric. Client collections in `src/lib/collections.ts` point to these proxy URLs. Current proxies: workspaces, projects, issues, labels, issue-labels.

### Electric Collections

All collections in `src/lib/collections.ts` use `columnMapper: snakeCamelMapper()` from `@electric-sql/client` to map Postgres `snake_case` columns to JS `camelCase`. Without this, `useLiveQuery` `where` filters silently fail. Use `undefined` (not `false`) to skip a query; use `and()`/`or()` from `@tanstack/react-db` instead of JS `&&`/`||`.

### Auth Guard

`_authenticated.tsx` uses `beforeLoad` with `throw redirect()` to gate routes. Session is cached in `authStateCollection` (local-only collection) to avoid re-fetching on every navigation.

### tRPC + Electric Sync

Mutations go through tRPC. `generateTxId` captures the Postgres transaction ID so the client can wait for Electric to sync the write before updating the UI. Routers are modular in `src/lib/trpc/` and combined in `api/trpc/$.ts` as `appRouter`.

### Issue List UX

- Issues are displayed in a CSS grid layout: `grid-cols-[24px_72px_24px_1fr_auto]` (priority, identifier, status, title, labels+due date)
- Rows are clickable to open the edit dialog; priority/status dropdowns use `stopPropagation` to prevent row click
- Empty status groups are hidden from the list
- Due dates display with a `CalendarDays` icon on the right side of rows

### Issue Filtering

- `src/lib/filters.ts` defines `IssueFilters` (statuses, priorities, labelIds), tab presets (all/active/backlog), and `matchesFilters()`
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
MINIO_ENDPOINT                # MinIO URL (http://localhost:9000)
MINIO_ACCESS_KEY              # MinIO access key
MINIO_SECRET_KEY              # MinIO secret key
```

## Style Conventions

- Template literals for strings (backticks, not quotes)
- Functional components, no class components
- shadcn/ui components in `src/components/ui/` — always use these over raw HTML elements
- Icons from `lucide-react`
- Business logic components in `src/components/` (not in `ui/`)
