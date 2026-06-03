# macOS UI Parity Plan (run this on a Mac)

> **Why this is a separate plan:** the macOS app (`apps/ios/ExponentialMac`, SwiftUI) and
> `ExpUI`/`ExpCore` SwiftUI code can only be compiled/run with Xcode + the macOS SDK, so this work
> must be done and verified on a Mac (Tuist project, `ExponentialMac` target). It was split out of
> the cross-platform UI parity effort whose Linux pass is already landed on `master`.

## Context & goal

Bring the macOS app as close to the **web app's UI/UX and feature set** as possible while staying
native (glass/material). The web app is the parity target (dark-only zinc OKLCH palette, indigo
accent, Inter; canonical screens: sidebar shell + workspace switcher, status-grouped issue list,
two-pane issue detail with a labelled right rail, dialogs). **Agent / coding-agent functionality is
out of scope** (Ghostty terminal, GitHub device-flow, plan-approval UI stay as-is).

The macOS app today is a deliberately-scoped subset: login (instance picker + password/Google/OIDC),
a project sidebar grouped by server/workspace, a status-grouped issue list with raw multi-select
filtering + per-project search, a single-column issue detail with inline status/priority/assignee/
due/labels + comments + read-only attachments, a workspace-settings sheet, and an accounts-only
Settings window.

### Reusable building blocks (reuse before rebuilding)
- **`ExpUI` is cross-platform today:** `GlassTheme` (Zinc palette, `glassCard/glassRow/glassSection/
  glassButton`, `StatusColor`/`PriorityColor`), `CrossPlatform.swift` (`Platform.open`,
  `inlineNavigationTitle()`), `WorkspaceAvatar`, `IssueColorExtensions`, `IssueEditorModel`,
  `MarkdownConversion`/`MarkdownAttributes`.
- **iOS views to MIRROR** (VM + ExpCore logic portable; only iOS `NavigationStack`/`.searchable`/
  sheet chrome needs a macOS `Form`/`List` equivalent): `AdminUsersView`, `AdminWorkspacesView`,
  `IntegrationsView`, `WorkspaceLabelsSection` (full 20-swatch grid + inline rename),
  `CommentThreadView`, `RecurrencePickerSheet`/`PickerSheet`, `TimeFieldButton`, `SyncDebugView`.
- **ExpCore portable logic:** `IssueFilters` + `FilterTab`/`deriveTab` (unused on Mac),
  `Recurrence`/`formatRecurrence` + `recurrenceIntervals`, `WorkspacesApi.ensureDefault`/`update`/
  `delete` (unused), `WorkspacePermissions`.

### ⚠️ Pre-existing bug to fix while wiring Admin (verified by reading source)
`ExpCore/Sources/API/AdminApi.swift` `listUsers`/`listWorkspaces` call `trpc.mutation` (HTTP **POST**)
and decode an `AdminUsersResult { users: [...] }` / `AdminWorkspacesResult { workspaces: [...] }`
wrapper. But the server (`apps/web/src/lib/trpc/admin.ts`) defines both as `adminProcedure.query`
(**GET**) and returns a **bare array** (`[AdminUser]` / `[AdminWorkspace]`), not a wrapper object. So
the iOS admin path is doubly wrong (POST to a query → 405; and the decode shape is off). `TrpcClient`
has no `query` (GET) method at all. **Fix when wiring macOS admin:**
1. Add a `query<O: Decodable>(accountId:path:)` to `ExpCore/.../TrpcClient.swift` that does a **GET**
   to `/api/trpc/{path}` (no body) and decodes the same `{result:{data}}` envelope. (Mirrors the
   Linux `trpc.query` GET helper added in `apps/linux/src/core/api/trpc.zig`.)
2. Change `AdminApi.listUsers`/`listWorkspaces` to use `query(...)` and decode `[AdminUser]` /
   `[AdminWorkspace]` directly (drop the `*Result` wrappers, or keep them only as conveniences).
   `setUserAdmin`/`deleteUser`/`deleteWorkspace` stay `mutationVoid` (POST) — those are mutations.
3. Same applies to **Integrations**: `integrations.google.status` is a `.query` (GET) →
   `IntegrationsApi` must use the new `query(...)`; `disconnect`/`backfill` stay POST mutations.
   (Verify `IntegrationsApi.swift` before assuming its method shapes.)

Server input/response shapes (authoritative, from `apps/web/src/lib/trpc/`):
- `admin.listUsers` (GET, no input) → `[{id,name,email,isAdmin,createdAt,workspaceCount,providers}]`
- `admin.setUserAdmin` (POST) `{userId,isAdmin}`; `admin.deleteUser` (POST) `{userId}` (server blocks
  deleting self / the last admin)
- `admin.listWorkspaces` (GET) → `[{id,name,slug,createdAt,plan,memberCount,projectCount,owners[]}]`
- `admin.deleteWorkspace` (POST) `{workspaceId}` (server blocks the public workspace)
- `integrations.google.status` (GET) → `{connected:false}` | `{connected:true,scope,connectedAt}`;
  `integrations.google.disconnect` (POST, no input); `integrations.google.backfill` (POST, no input)
  → `{ok,scheduled}`
- `workspaces.create` (POST) `{name}`; `workspaces.update` (POST) `{id,name?,isPublic?,publicWritePolicy?}`;
  `workspaces.delete` (POST) `{workspaceId}`; `workspaces.ensureDefault` (POST, no input)
- `projects.create` (POST) `{workspaceId,name,prefix,color}`; `projects.delete` (POST) `{projectId}`
- `labels.create` `{workspaceId,name,color}`; `labels.update` `{workspaceId,labelId,name?,color?}`;
  `labels.delete` `{workspaceId,labelId}`

---

## Phase A — quick reuse wins (do first; mostly S–M)

1. **Theme tokens** — `ExpUI/Sources/GlassTheme.swift`:
   - `StatusColor`: set **`todo` to near-white** (`Zinc._50` / `.primary`) — web's todo is
     `text-foreground`, not gray; currently backlog AND todo are both gray.
   - Add an **indigo accent** (`Color(hex: "#6366f1")` / `#4f46e5`) and apply it to primary buttons +
     count badges (replace the default system-blue `.borderedProminent`/`Color.blue.opacity`).
   - Hoist a shared **`ColorSwatchGrid`** (the 20-color `LABEL_COLORS` from
     `apps/web/src/lib/label-colors.ts`) into `ExpUI`.
   - Apply the (currently unused) `glassCard/glassRow/glassSection` modifiers to the sidebar,
     settings sheets, and detail cards so they read as native macOS material.
2. **Wire APIs** — `apps/ios/ExponentialMac/MacAppDependencies.swift` currently wires 9 APIs
   (`authApi`, `issuesApi`, `labelsApi`, `commentsApi`, `workspacesApi`, `workspaceMembersApi`,
   `workspaceInvitesApi`, `issueImagesApi`, `agentPlanApi`). **Add `adminApi` + `integrationsApi`**
   (constructed from the shared `trpc`/`httpClient` like the others).
3. **Sidebar-footer user-identity menu** — `MacShell.swift`: add an avatar + email menu hosting
   **Admin** (only when `deps.auth.isAdmin`), **Integrations**, **Send feedback** (`Platform.open` to
   the instance `/feedback`), **Sign out**. This is the entry point for the next two items.
4. **Mirror iOS Admin + Integrations** into `Mac*` views (after the TrpcClient `query` fix above):
   `AdminUsersView`/`AdminWorkspacesView` (Form/List + `.searchable`, admin toggle, delete) and
   `IntegrationsView` (Google Calendar status + backfill + disconnect; "Connect" stays web-only — open
   `/account/integrations`, matching iOS).
5. **Workspace settings** — `MacWorkspaceSettingsView.swift`: add a **Name** field
   (`workspacesApi.update(name:)`) + owner-only **Danger Zone** delete (`workspacesApi.delete`, gate on
   `isOwnerOrAdmin && !isPublic`); replace the raw `#hex` label field with the shared
   **`ColorSwatchGrid`** (mirror iOS `WorkspaceLabelsSection`) + inline rename.

## Phase B — feature parity (M–L)

6. **Workspace switcher** — `MacShell.swift`: a toolbar `Menu`/header button listing
   `MultiAccountProjectLoader.groups` with `WorkspaceAvatar` + a ✓ on the active workspace, plus
   "New workspace" (needs the Phase C API) and "Workspace Settings…". Today workspaces are only
   non-selectable sidebar section headers reachable via right-click.
7. **Filter tab presets + active pills** — `MacIssueListView.swift`: All/Active/Backlog tabs using
   ExpCore `FilterTab`/`deriveTab` (exists, unused); a `+` per status group; a Repeat indicator before
   recurring titles.
8. **Issue detail → right rail + recurrence** — `MacIssueDetailView.swift` currently renders
   properties as a horizontal row in one scrolling column. Add a **recurrence** control (mirror iOS
   `RecurrencePickerSheet`, reuse `Recurrence`/`formatRecurrence`; recurrence forces status=todo +
   dueDate=first occurrence), **due-date start/end time** (mirror iOS `TimeFieldButton`), a true
   **right rail** (`HSplitView`/`aside`) on wide windows in the canonical order STATUS / PRIORITY /
   ASSIGNEE / LABELS / DUE DATE / PROJECT, and upgrade comments to mirror iOS `CommentThreadView`
   (relative time, "· edited", edit/delete).
9. **Create Issue** — `MacCreateIssueView.swift`: add labels, recurrence, due-date time range, and a
   "Create more" toggle (mirror iOS `CreateIssueSheet`).

## Phase C — needs new ExpCore API (L)

10. Add `projects.create` (new `ProjectsApi` or extend `WorkspacesApi`) — **no `ProjectsApi.swift`
    exists today** — and `workspaces.create`, mirroring the web tRPC inputs above. Then build a Mac
    **create-project** sheet (from a new sidebar `+`) and **create-workspace** (from the switcher),
    invoke `WorkspacesApi.ensureDefault` post-login, and add a meaningful **empty state** (today only
    `ContentUnavailableView("Select a project")`). A full onboarding wizard is optional on desktop.

## Styling deltas to reconcile (from the review)
- todo icon: web near-white vs macOS gray (fix in Phase A #1).
- accent: web indigo vs macOS system-blue (Phase A #1).
- label palette: web 20 swatches vs macOS 7-color/`#hex` (Phase A #1 / #5).
- glass: `GlassTheme` modifiers exist but are almost entirely unused on Mac (Phase A #1).

## Verification (on the Mac)
- Build the **`ExponentialMac`** target (Tuist generate + Xcode, or `tuist build`). Resolve any Swift
  compile errors first — none of this was compiled on the Linux dev box.
- `bun run backend:up` (or point at a real instance), sign in, and confirm: workspace switcher (✓
  active, New workspace, settings), footer user menu (Admin if admin / Integrations / Send feedback /
  Sign out), Admin lists actually load (the TrpcClient GET fix), Integrations status loads,
  issue-detail right rail + recurrence + comment timeline, create-issue with labels/recurrence/time,
  create-project. Indigo accent + todo=near-white render correctly.
- Sync a change from the web app and confirm it reflects live (Electric).
- Cross-platform: edit a description with bold/list/task/code/link/image and confirm the markdown
  round-trips identically with web + Linux (shared GFM `{text}` contract).
```
