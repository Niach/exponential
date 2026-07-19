# Exponential — Masterplan (v5, release)

> **Vocabulary note (EXP-180, 2026-07-19):** this document predates the great rename. Read
> "workspace" as **team** and "project" as **board** — the rename was a clean cut across copy,
> URLs, code identifiers, and DB tables (`teams`/`boards`/`team_id`/`board_id`; `/w/` and
> `/projects/` URL forms are dead). EXP-180 also removed public feedback boards and the
> dev/tasks/feedback board types entirely, moved the helpdesk to ONE team-level switch feeding
> a shared Support inbox (standalone tickets, escalatable to board issues), and gave the Free
> plan a feedback widget. Where this plan references those systems, the code is the spec.

*2026-07-05. The single plan of record, superseding `docs/archive/masterplan-v4.md`
(project=repo + desktop git IDE — shipped through R0–R3/R4; its §9 do-not-regress contract is
carried forward here). Where this document is silent about an inherited system, **the code is
the spec**. This plan takes the product from "v4 feature-complete" to **public release**: the
EXP-1…12 fix wave (staging feedback board), the per-seat monetization rework, MCP completeness,
the marketing site + distribution pipeline, and the release gate itself.*

**Inputs**: staging issues EXP-1…EXP-12 (EXP-6 was deleted; confirmed nothing lost), two
attached screenshots (EXP-8), a seven-agent code-grounding research pass (file:line references
below come from it), and decisions locked with Danny on 2026-07-05 (§2).

---

## Table of contents

- §1 Vision deltas
- §2 Locked decisions (new in v5)
- §3 Monetization rework (EXP-9)
- §4 Web fix wave (EXP-1 duplicates, EXP-5 web enhancements, EXP-7 public board)
- §5 Feedback widget v2 (EXP-2)
- §6 GitHub flow & integrations removal (EXP-3, parts of EXP-12)
- §7 Run configs unification (EXP-4)
- §8 Desktop IDE wave (EXP-8)
- §9 Mobile wave (EXP-12)
- §10 MCP completeness (EXP-10)
- §11 Marketing site & distribution (EXP-11)
- §12 Execution plan (phases P0–P5, packetized for the Opus subagent workflow)
- §13 Release checklist (manual steps)
- §14 Do-not-regress contract

---

## §1 Vision deltas

`docs/vision.md` is updated alongside this plan (same commit). What changes:

1. **Per-seat reversal.** The old moat line "billed per workspace, never per seat" is dead.
   New stance: **individuals ride free forever; teams pay per seat, at roughly half of
   Linear's price.** Rationale: flat-workspace pricing forced per-workspace limits, which
   forced the `ownedWorkspaces` anti-loophole cap — limits users hate and that punish the
   wrong thing. Seats scale with the value a team gets; solo users cost us ~nothing.
2. **The loop is the story.** Marketing and vision now lead with the full circle: **feedback
   widget → issue → Claude codes → PR ships → reporter gets the resolution email.** Tagline:
   **"Make your app exponential."**
3. **What we still never monetize**: notifications (in-app/push/email) and remote
   watch/steer. Both relays are computationally trivial and steer is the killer demo.
4. Principles, platform roles, the NOT-list, and the killer flow are unchanged.

---

## §2 Locked decisions (v5)

v2/v3/v4 locked decisions L1–L18 stay binding except where amended below.

| # | Decision | Notes |
|---|----------|-------|
| L19 | **Per-seat billing bound to the workspace.** A subscription belongs to one workspace (not to a user); seats = purchased quantity; members (excluding `isAgent` users) must fit in seats. The owner-fan-out plan resolution and the `ownedWorkspaces` marketing limit die. | reverses vision moat line |
| L20 | **Tiers**: Free — 1 seat, unlimited projects/repos/coding sessions, 250 MB storage/workspace, no widget. Pro — **$5/seat/mo billed yearly only** ($60/seat/yr), 5 GB/ws, 1 widget config. Business — **$10/seat/mo, monthly or yearly**, 50 GB/ws, unlimited widgets, priority support, SSO line-item. | |
| L21 | **Feedback widget (+ helpdesk resolution emails) is Pro+.** SSO/OIDC is a Business *pricing-page line item* this release ("coming soon") — per-workspace SSO does not exist yet (OIDC is instance-level env) and is NOT built now. | |
| L22 | **Push and remote start/steer stay free on every tier.** The FOUNDING coupon is deleted (Creem dashboard + both UI mentions). | |
| L23 | **One run-config system.** The `.exponential/config.json` preview-target system (WebTarget/AndroidTarget/IosTarget, `previewConfig.targets`, web "Run Targets" dialog section, desktop discovery promise) is deleted. `run_configs` (DB, per project, argv/cwd/env, Trust & Run) is the only model; **argv-direct spawn stays** (no shell — L8/L5 security posture unchanged); editors present the command as a single line via argv⇄line parsing. Run-config editing is **IDE-only** (web shows no run-config UI). | amends v3 preview-target scope |
| L24 | **L17 amendment**: the "Create run configs with Claude" task is a `claude_task` **with a scoped `.mcp.json`** (expu_ key, same as coding sessions) so Claude can call the new run-config MCP tools. Still no `coding_sessions` row, no worktree, always visible. All *other* Claude tasks (conflict fixing) remain MCP-less. | |
| L25 | **The Integrations menu dies everywhere** (web account page + nav links, desktop Account pane, iOS, Android). GitHub App install/manage lives solely in **workspace settings → Repositories** (UI already exists there). `github_installations` stays user-scoped in the DB — only the surface moves. | EXP-3/EXP-12 |
| L26 | ~~**Mobile is a pure companion**: no workspace creation, no project creation (including onboarding — replaced by a "set up on the web" screen), no integrations. Issue creation only inside a project.~~ **SUPERSEDED by L31 (2026-07-07)** — row kept for the record; do not execute its removals. | EXP-12 |
| L27 | **Duplicate = status interception.** Selecting status `duplicate` anywhere opens the duplicate-search picker; confirming sets `duplicateOfId` (server keeps status in lockstep); cancelling reverts the status control. The standalone "Mark as duplicate…" menu entries are removed. "Unmark duplicate" + the banner stay. The create-issue dialog drops `duplicate` from its status options (creating a new issue as a duplicate is nonsense). | EXP-1 |
| L28 | **Release = cloud + desktop + submitted stores.** Release day: production greenfield DB reset (v2 commitment), new pricing live, marketing site live, desktop downloads live (signed + notarized, stable latest URLs); iOS/Android are *submitted* to the stores but approval does not gate the release. Full desktop auto-update is post-release; an in-app "update available" banner (GitHub Releases API check) IS in scope. | |
| L29 | **Anonymous public reads reach issue detail.** Public workspaces expose the full-page issue detail (read-only) to signed-out visitors; the users shape stays members-only by design (placeholder avatars are the accepted degradation). | EXP-7 |
| L30 | **Default branch is resolved, never assumed.** No code path may assume `main`: connect-time resolution from GitHub, launch-time healing (exists), and the desktop consuming the healed value everywhere. | EXP-8 |
| L31 | **Mobile ships FULL ONBOARDING** (amends L26; Danny, 2026-07-07). iOS + Android get the same first-run experience as web/desktop: a server-gated wizard (`onboardingCompletedAt` via `lib/auth/onboarding.ts` — never inferred locally from synced data) with guided create-first-project (name → auto-derived prefix → color → **mandatory repo picker**) and inline GitHub App connect (browser / Custom Tab install hop with foreground re-detect), plus regular in-app project creation and repo management. The cross-platform audit confirmed both apps already implement this — it is blessed behavior, not a violation. Boundaries that stand: L25 (no account-level Integrations menu — GitHub connect lives in the repo-picker / workspace-settings flow), L23 (run configs stay IDE-only), and workspace creation stays server-side (`workspaces.ensureDefault` personal-workspace path; `workspaces.create` remains instance-admin-only). | amends L26 (EXP-12) |

---

## §3 Monetization rework (EXP-9)

### 3.1 Current state (research summary)

- `PLAN_LIMITS` in `apps/web/src/lib/billing.ts:36-69`; six axes (members, projects,
  storageMb, repositories, concurrentCodingSessions, ownedWorkspaces).
- Subscriptions: `@creem_io/better-auth` plugin (`lib/auth/index.ts:329-349`),
  `creem_subscriptions.referenceId → users.id` (`packages/db-schema/src/auth-schema.ts:134-155`).
  `getWorkspacePlan()` fans out over workspace **owners** and takes the best tier
  (`billing.ts:93-146`) — one Pro user lights up all their owned workspaces.
- Seat quantity is never read from Creem. Prices exist only in UI tables
  (`components/workspace/plan-comparison.tsx:24-58`, `apps/marketing/src/lib/pricing.ts`).
- `getWorkspaceUsage()` member count includes the widget's `isAgent` user
  (`billing.ts:228-233`) — the EXP-7 "2 members" bug.
- Widget creation ungated (`lib/trpc/widgets.ts:88-131`). Push deliberately free
  (`billing.ts:18-20`) but the marketing table wrongly advertises it as paid
  (`pricing.ts:24`).

### 3.2 Target model

| | Free | Pro | Business |
|---|---|---|---|
| Price | $0 | $5/seat/mo, **yearly only** | $10/seat/mo, monthly or yearly |
| Seats | 1 | purchased quantity | purchased quantity |
| Projects / repos / coding sessions | **unlimited** | unlimited | unlimited |
| Storage per workspace | 250 MB | 5 GB | 50 GB |
| Feedback widget + helpdesk emails | — | 1 config/ws | unlimited configs |
| Push / email / steer | free | free | free |
| SSO/OIDC | — | — | pricing-page "coming soon" |

- **Seat counting** excludes `isAgent` users everywhere. Free = the owner alone; inviting the
  first teammate is the upgrade moment.
- **Deleted limits**: projects, repositories, concurrentCodingSessions, and the *marketed*
  ownedWorkspaces cap. An **invisible abuse guard** of 10 owned workspaces on Free remains
  (storage-farming guard; not shown in any pricing UI; error copy: "contact us").
- **Downgrade/lapse policy**: workspace over its seat count → invites blocked + upgrade nudge;
  existing members keep working (never lock people out of their data).
- **Self-hosted**: unchanged — `SELF_HOSTED=true` ⇒ unlimited everything. Marketing adds an
  **Enterprise** self-host tier: contact-sales button, extended support, honor-system ">10
  employees" language. No enforcement in code.

### 3.3 Implementation

1. **Spike first (P0.a gate)**: verify `@creem_io/better-auth` supports checkout `units`/
   quantity and workspace metadata. If not, bypass the plugin's checkout for subscription
   creation (direct Creem API: create checkout with `units: seats`, `metadata.workspaceId`)
   while keeping the plugin's webhook persistence. The packet must prove a seat-quantity
   checkout round-trip on Creem test mode before anything else lands.
2. **Schema**: add `workspaceId` (nullable text→uuid FK) + `seats` (integer) to
   `creem_subscriptions` (or a `workspace_subscriptions` mapping table if the plugin's table
   is hands-off). Bind on webhook/success via checkout metadata.
3. **`billing.ts` rewrite**: `PLAN_LIMITS` shrinks to `{ seats, storageMb, widgetConfigs }`
   (+ the invisible free `ownedWorkspaces` guard). `getWorkspacePlan(workspaceId)` = workspace-
   bound subscription lookup, no owner fan-out. Delete `assertWithinPlanLimits` call sites for
   dead axes: `lib/trpc/projects.ts:124,131`, `repositories.ts:219`,
   `coding-sessions.ts:29`, `steer.ts:162`. Keep invite-time gating
   (`workspace-invites.ts:28,56`) as the **seat check** (member count excl. agents < seats).
   Fix `getWorkspaceUsage` member count to exclude `isAgent` (`billing.ts:228-233`).
4. **Widget gate**: `widgets.create` asserts plan ≥ Pro and config-count ≤ plan's
   `widgetConfigs` (insert after `resolveWorkspaceAccess`, `lib/trpc/widgets.ts:~102`).
   Bootstrap's dogfood path is exempt (internal insert).
5. **UI**: `plan-comparison.tsx` → three per-seat columns + seat-quantity picker at checkout;
   `billing-section.tsx` → seat usage bar (n of m seats), storage bar, widget count, Manage
   portal. Remove the FOUNDING callout (`plan-comparison.tsx:198-201`). Respect the
   Infinity→null tRPC convention (`use-billing.ts:76-77`).
6. **Marketing sync**: rewrite `apps/marketing/src/lib/pricing.ts` (fix the push-notifications
   lie at line 24, remove `FOUNDING_CODE` + `FoundingCallout`), per-seat cards, self-host
   Enterprise contact-sales card.
7. **Manual (release checklist §13)**: create Creem products (Pro yearly per-seat, Business
   monthly, Business yearly), new env `CREEM_BUSINESS_YEARLY_PRODUCT_ID`, delete the FOUNDING
   coupon in the Creem dashboard.

---

## §4 Web fix wave (EXP-1, EXP-5, EXP-7)

### 4.1 Duplicates (EXP-1, L27)

- Remove "Mark as duplicate…" from the row context menu
  (`components/issue-row-menu/context-menu.tsx:181-191`) and the detail overflow menu
  (`issue-detail-view.tsx:402-411`). Keep Unmark + `DuplicateOfBanner`.
- Intercept `status='duplicate'` at the three status sinks and open the existing
  `IssuePickerDialog`: (a) list-row dropdown `issue-properties/status-dropdown.tsx:44-49` +
  context-menu `submenus.tsx` StatusSubmenu, (b) detail properties panel
  (`issue-properties-panel.tsx:255-278` → `issue-detail-view.tsx:297-300`), (c) edit-dialog
  chips (`issue-editor/chips.tsx:74-91`). On pick → `issues.update({duplicateOfId})` (server
  sets status, `lib/trpc/issues.ts:287-326` already lockstep). On cancel → status reverts.
- Create dialog: remove `duplicate` from its status options.
- Desktop + mobile get the same interception (desktop duplicate picker already exists,
  `issue_detail.rs:901`; Android/iOS status pickers gain the same rule) — parity packets in
  §8/§9.

### 4.2 Web enhancements (EXP-5)

1. **Mobile web cleanup + the desktop empty bar (items 1+4, one root cause)**: the topbar
   `<header>` at `components/workspace/mobile-topbar.tsx:89` renders on ALL breakpoints (only
   its children are `md:hidden`) → 48px empty strip on desktop. Make the header itself
   mobile-only. De-duplicate mobile nav: **topbar keeps** hamburger + workspace/project
   context + New issue; **drawer (sidebar sheet) keeps** Search, My Issues, Inbox, projects,
   user menu. Remove the duplicated topbar search/My-Issues/inbox/user-menu cluster
   (`mobile-topbar.tsx:108-201`).
2. **Right-click**: delete the leftover `contextmenu` preventDefault in
   `components/issue-editor/markdown-editor.tsx:432-437`. Nothing else references it.
3. **Ctrl/Cmd+F opens search**: lift `IssueSearchSheet` open state into the workspace layout
   (today duplicated in `sidebar.tsx:80` and `mobile-topbar.tsx:75`), add a global keydown
   (pattern: the Cmd+B handler in `components/ui/sidebar.tsx:97-110`) that preventDefaults
   Cmd/Ctrl+F. Desktop app parity in §8.13 (new quick-search modal — it has no search today).
4. **Widget user hygiene**: filter `isAgent` at the source — `useWorkspaceUsers`
   (`hooks/use-workspace-data.ts:185-219`) — which fixes the assignee picker
   (`issue-properties/assignee-picker.tsx:106`), the row-menu AssigneeSubmenu, and every other
   consumer at once. The billing member count fix is §3.3(3). (EXP-7's "2 members" is this.)
5. **Newlines stripped on create (item 6)**: root cause is the TipTap↔markdown round-trip
   collapsing blank paragraphs (`markdown-editor.tsx:411-415` `tiptap-markdown` config), NOT
   the server trim (`domain.ts:159-166` only trims edges). Packet order: reproduce (serialize
   a doc with blank lines through the create path), fix at the failing layer (hardBreak /
   empty-paragraph serialization config), add a round-trip unit test. Must stay inside the
   GFM contract — cross-client byte-parity (Android tests) is the lock.

### 4.3 Public feedback board (EXP-7, L29)

- Remove the hard login redirect in
  `routes/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier.tsx:19-26`; mirror
  the parent route's public-aware logic (`route.tsx:42-64`). Shapes are already
  anonymous-capable (issues/projects/workspaces/comments via public where-clauses,
  `lib/auth/membership.ts:61-112`); verify labels + issue-labels shapes are public-scoped;
  confirm no child component subscribes a `requireAuth` shape when anonymous. Detail renders
  read-only via existing `readOnly={!permissions.canMutateIssue(issue)}`.
- Users shape stays members-only (by design): anonymous sees placeholder avatars.
- Member-count fix is §3.3(3)/§4.2(4).

---

## §5 Feedback widget v2 (EXP-2)

All in `packages/widget` (+ rebuild into `apps/web/public/widget/v1/`):

1. **Launcher**: tiny icon-only floating button, **default bottom-LEFT**, hover effect
   (scale/label reveal). `init({ position: 'bottom-left' | 'bottom-right' })` option, default
   bottom-left. The in-app `FeedbackWidgetProvider` adopts the same default.
2. **Usable above dialogs**: mount the widget container on `document.documentElement` (not
   `body` — Radix modal sets `pointer-events:none` on body) with max z-index, so the launcher
   and panel work while any app dialog is open.
3. **Crop**: the annotation editor gains a crop tool (rectangle select in image-pixel space,
   `packages/widget/src/annotate/`); crop applies at flatten time (`flattenAnnotations`
   re-encode ladder crops the canvas first); shapes stay editable pre-submit; recrop allowed.
4. Widget creation becomes Pro-gated server-side (§3.3(4)) — the widget bundle itself is
   unchanged by billing.

---

## §6 GitHub flow & integrations removal (EXP-3, L25)

1. **Stale repo list**: the 60s per-user server cache (`lib/trpc/integrations.ts:15-19,
   155-164`) swallows mid-flow installs. Add `refresh: boolean` input to
   `integrations.github.repos` that busts the cache; the picker passes it on focus-refetch
   and the manual button (`components/github-repo-picker.tsx:67-71, 118-126`); additionally
   invalidate the user's cache entry in the setup redirect handler
   (`routes/api/integrations/github/setup.ts`).
2. **Kill the Integrations surface**: delete `routes/_authenticated/account/integrations.tsx`
   (it holds only the GitHub App card), its nav links (`sidebar.tsx:326`,
   `mobile-topbar.tsx:172`), and the setup redirect's non-dialog fallback target (redirect to
   workspace settings instead of `/account/integrations`). GitHub App management remains in
   workspace settings → Repositories (`repositories-section.tsx:67-204` already has
   install/manage/status). Desktop/iOS/Android removals in §8.9/§9.
3. Onboarding keeps the existing popup round-trip (`/integrations/github/installed`
   self-close + opener focus refetch) — it works once (1) lands.

---

## §7 Run configs unification (EXP-4, L23/L24)

1. **Delete the preview-target system**: in `packages/db-schema/src/domain.ts:201-375` remove
   `WebTarget`/`AndroidTarget`/`IosTarget`/`CommandTarget` + `platformValues` + the targets
   half of `ProjectPreviewConfig`/`ProjectPreviewMirror`; `previewConfig` shrinks to
   `{ feedbackProjectId }` (still used by widget dogfood wiring — keep the column and the
   `projects.updatePreviewConfig` proc, simplified). Web: remove the "Run Targets" section +
   `.exponential/config.json` copy from `project-preview-settings-dialog.tsx:149-181` (dialog
   becomes "Feedback project" only). Desktop: nothing to delete (discovery was never
   implemented); drop any lingering mirror parsing of `targets` if present. Delete the repo
   root `.exponential/config.json`. Contract note: `platformValues` lives in
   `packages/domain-contract/contract.json` → update + `bun run --filter @exp/domain-contract
   generate`.
2. **Run configs stay DB + argv** (L23): desktop `run_configs_editor` (owner-only CRUD,
   `crates/ui/src/run_bar.rs:735+`) presents the command as a single line (argv⇄line, mirror
   of `apps/web/src/lib/run-configs.ts` parse/format helpers in Rust). Web keeps NO run-config
   UI.
3. **"Create with Claude" button** in the desktop run-configs editor: spawns
   `coding::claude_task` at the trunk clone with a scoped `.mcp.json` (L24) and a prompt to
   inspect the repo (README/package.json/Cargo.toml/etc.), propose run configs, and create
   them via the new `run_configs` MCP tools (§10). Tab kind `ClaudeTask`, visible/steerable,
   no `coding_sessions` row. Run bar refetches configs when the tab exits.

---

## §8 Desktop IDE wave (EXP-8)

Grounded in `apps/desktop` research; items ordered by user report.

1. **Always-editable issue editor**: remove the Write/Preview toggle — delete the toolbar
   eye button (`crates/ui/src/markdown/toolbar.rs:315-325`), the `preview` field +
   `toggle_preview`/`is_preview` (`crates/ui/src/markdown/editor.rs:305,440-442,1032-1033`),
   collapse the render branch (`editor.rs:1219-1274`) to editable blocks always.
2. **Image removal persists**: structural edits currently only fire `on_change` (in-memory
   mirror) — `remove_image_at` (`editor.rs:871-900`) never triggers `on_save` (blur-only,
   `description_editor.rs:125-129`). Add a commit hook for structural edits (image
   insert/remove) that invokes the save path immediately.
3. **Detail padding**: unify horizontal padding across breadcrumb (`px_4`), tabs (`px_3`),
   title (`px_3`), description (`px_4`), fallback (`px_5`) in `issue_detail.rs` to one shared
   edge; recheck `max_w(768).mx_auto()` centering against the properties panel.
4. **Sidebar project color dots**: custom row in `sidebar.rs` `projects_menu` (L301-332)
   prepending a `size_3().rounded_full()` dot from `Project.color`
   (`parse_hex_color` precedent at `issue_detail.rs:430`). Web reference:
   `apps/web/.../sidebar.tsx:267-270`.
5. **Create-project dialog is BROKEN + needs repo picker**: server requires `repository`
   (`lib/trpc/projects.ts:102-116`) but `ProjectsCreateInput`
   (`crates/api/src/projects.rs:40-51`) doesn't send one. Add `repository: {repositoryId}` to
   the input, a registry repo picker fed by `repositories.list` (pattern:
   `crates/ui/src/settings/repositories.rs::fetch_repositories`), submit gated on selection,
   empty state links to web settings ("Connect a repository in the web app"). Inline
   GitHub-connect stays web-only.
6. **Remove the "Personal API key" settings card**: provisioning is already fully automatic
   (`ensure_personal_key`, `crates/api/src/users.rs:139-156`; `.mcp.json` writing
   `crates/coding/src/mcp_json.rs:60-91`). Delete `render_key_card` + call
   (`crates/ui/src/settings/coding.rs:418-481,536`) and the now-dead KeyStatus machinery.
7. **Numeric identifier sort**: replace the lexicographic tie-break in
   `crates/sync/src/collections.rs::sort_issues` (L481-488) with a (prefix, number) natural
   comparator; apply the same in `issue_detail.rs:901` (duplicate picker),
   `markdown/autocomplete.rs:159`, `debug_board.rs:110`.
8. **Terminal panel**: (a) click on empty dock area starts a shell (`render_empty`
   `terminal_dock.rs:500-519` → `new_shell_tab`); (b) on `TabClosed` with empty manager →
   collapse the bottom dock (subscription at L193-213 currently only refocuses); (c) remove
   the zoom control — it comes from the `DockItem::tabs` wrapper (`workspace.rs:266`); build
   the bottom dock chrome-less like the center (`workspace.rs:249` pattern) or suppress
   TabPanel zoom.
9. **Account menu parity (with L25 applied)**: target menu on BOTH web+desktop:
   Admin (web, admin-only) · Settings · Notifications · New workspace/Join workspace
   (solo-gated) · Sign out. Desktop splits its "Account" entry accordingly and drops the
   Integrations pane (`crates/ui/src/settings/account.rs`); trigger becomes avatar+email for
   parity (`sidebar.rs:376-421` vs web `sidebar.tsx:286-348`).
10. **File browser**: design pass on the Files rail (`crates/ui/src/file_tree.rs`) + multi-tab
    file viewing — `FileViewerView` (`file_viewer.rs:73-146`) holds one path today; move to
    an open-paths + active-index tab strip in the viewer screen (larger structural item;
    keep read-only per L15).
11. **Navigation**: `Navigation.back_stack` + `go_back()`/`can_go_back()` already exist but
    are dead code (`crates/ui/src/navigation.rs:61-195`). Wire a back button into the top
    chrome + `cmd-[`/`Alt+Left` keybinding; add breadcrumbs to non-detail screens
    (Board/My Issues/Inbox/Settings/File viewer — only issue detail has one today).
12. **Default branch (the EXP-8 screenshot bug, L30)** — three layers:
    - Server: `connectRepositoryInTx` stops blind-seeding `main`
      (`lib/trpc/repositories.ts:88-93`) — resolve via `resolveRepoDefaultBranch`
      (`lib/integrations/github-app.ts:123-131`) when not supplied; `repositories.list`
      heals like `installationToken` already does (`repositories.ts:319-349`); one-off
      backfill script for rows whose stored branch disagrees with GitHub.
    - Desktop: the trunk sync worker uses the *stale scope value* — switch
      `git_bar.rs:585-621` pull/push to consume `token.default_branch` (already minted at
      :593); same for `issue_changes.rs:742-744,774` and `source_control.rs:601`; kill the
      `repo_resolver.rs:210-213` `"main"` fallback (use the healed API value).
13. **Cmd+F quick search** (EXP-5 parity): new lightweight search modal over synced issues
    (title substring, navigate on pick — reuse the autocomplete index), bound to cmd/ctrl+F.
14. **Duplicate interception parity** (L27): status dropdowns intercept `duplicate` → open
    the existing duplicate picker (`issue_detail.rs:901`); remove any standalone
    mark-as-duplicate entry.

---

## §9 Mobile wave (EXP-12)

### 9.1 iOS sync blackout (blocker)

"No shape activity + no log entries + Resync no-op" is only producible when **no HTTP
round-trip ever happens** — every poll logs to SyncDebug before status branching
(`ShapeClient.swift:117-119`). Prime suspect (H1): `db.pool(forAccountId:)` throws (GRDB
migrations `v2_offset_refetch_state` / `v3_project_repository_id`), which silently kills
`launchPipeline` AND makes `resync()` return early (`SyncManager.swift:88,122-131`) — errors
go only to `os.Logger`. Packet:

1. **Diagnose on a staging device** (log filter: SyncManager/ShapeClient/AppDependencies) —
   confirm/deny H1 before fixing; H2 fallback: token/baseUrl provider nil-spin
   (`ShapeClient.run()` guard L50).
2. **Fix the root cause** (likely migration hardening against existing `-v2.sqlite` DBs; add
   migration tests against fixture DBs at each historical schema version).
3. **Never silent again**: surface pool-open/migration failures + "pipeline launched" status
   into `SyncDebug` and the diagnostics screen; `resync()` failure paths report instead of
   no-op.

### 9.2 Both platforms

> **Amended 2026-07-07 (L31):** the L26 removals below are **rescinded — do not execute
> them**. Mobile keeps full onboarding: the server-gated first-run wizard (welcome →
> create-first-project with the mandatory repo picker + inline GitHub App connect) ships on
> both platforms (iOS `UI/Onboarding/OnboardingView.swift` + `CreateProjectForm.swift` +
> `UI/Settings/GithubRepoPicker.swift`; Android `ui/onboarding/OnboardingScreen.kt` +
> `CreateProjectForm.kt` + `GithubRepoPickerSheet.kt`), plus in-app project creation (iOS
> `WorkspaceProjectsSection.swift` / `IssuesHomeView.swift`; Android `CreateProjectSheet.kt`
> from `IssueListScreen.kt` / `WorkspaceSettingsScreen.kt`). Workspace creation stays
> off-device (`workspaces.ensureDefault` only). The Integrations-menu removal (L25) and the
> other items below still stand.

- ~~**Remove workspace creation** (L26)~~ / ~~**Remove project creation** (L26): onboarding
  becomes a "set up on the web" screen~~ — **rescinded by L31 (2026-07-07)**, see the
  amendment note above.
- **Remove Integrations** (L25): iOS `SettingsView.swift:172-175` + `AppRoute.integrations` +
  `IntegrationsView.swift`; Android `SettingsScreen.kt:184-189` + `integrations` route +
  `IntegrationsScreen.kt`. (Keep the repo-picker/integrations API clients used elsewhere —
  the L31 onboarding wizard and repo pickers depend on them.)
- **Feedback routing**: open `${baseUrl}/feedback` (server redirect = single source of truth)
  instead of the hardcoded `/w/feedback/projects/feedback` (iOS `SettingsView.swift:182-200`,
  Android `SettingsScreen.kt:197-216`).
- **Duplicate interception parity** (L27) in both status pickers.

### 9.3 Android-specific

- **Swipe → long-press**: replace `SwipeableIssueRow` (Material3 SwipeToDismissBox mirroring
  iOS) with `combinedClickable(onClick, onLongClick)` opening a bottom-sheet action list
  (Mark done / Move to backlog / …; patterns: `IssuePickerSheet.kt`). Call sites:
  `IssueListScreen.kt:201-213`, `MyIssuesScreen.kt:98`. iOS keeps native `.swipeActions`.
- **Issue-detail crash**: reproduce with `adb logcat` FIRST (exact stack collapses the
  hypotheses). Prime suspect: the new issue-ref pill `addLink` path without range coercion /
  overlapping LinkAnnotations (`ui/markdown/MarkdownView.kt:233-248` vs the coerced mark loop
  at 206-208; new code from commit `6549c5c`). Apply the defensive `coerceIn` + overlap-skip
  regardless; then fix the actual cause per the stack.
- **One add-issue button, project-only**: remove the top-right create button
  (`IssueListScreen.kt:109-112`); FAB shows ONLY on the project route — change `showCompose`
  to require `currentRoute == "project/{projectId}"` and drop the `fallbackProjectId` compose
  fallback (`AppNavHost.kt:236-241,391-419`).

---

## §10 MCP completeness (EXP-10)

23 tools exist (`apps/web/src/lib/mcp/tools.ts`; auth via `resolve-bearer.ts` — cookie,
bearer session, `expu_` key, OAuth). Add (reusing the tRPC-caller pattern; workspace-access
gated like existing tools):

| New tool | Backs onto |
|---|---|
| `exponential_comments_update` / `_delete` | `comments.update/delete` |
| `exponential_issues_subscribe` / `_unsubscribe` | `subscriptions.*` |
| `exponential_notifications_list` / `_mark_read` (id or all) | `notifications.*` |
| `exponential_members_list` | `users.listByWorkspaceIds` (+role; excludes `isAgent` by default — needed to resolve `assigneeId`) |
| `exponential_repositories_list` / `_add` / `_branch_diff` | `repositories.*` |
| `exponential_run_configs_list` / `_create` / `_update` / `_delete` | `runConfigs.*` (powers §7.3) |
| `exponential_issues_pr_files` | `issues.prFiles` |
| `exponential_projects_delete` / `_set_repository` | `projects.*` |
| `exponential_workspaces_create` / `_update` | `workspaces.*` |
| `exponential_invites_create` / `_list` / `_revoke` | `workspace-invites.*` (owner-gated) |
| `exponential_attachments_upload` | new: base64 image → S3 + attachments row, returns the `![](/api/attachments/{id})` form (reuse `issue-attachments.ts` helpers, plan storage limit applies) |

Explicitly out: steer, admin, billing, push-tokens, api-key minting, onboarding.
`issues_update` already sets any status; `issues_update_status` stays the narrow coding-flow
tool. Every new tool gets a vitest exercising the happy path + a permission-denial.

---

## §11 Marketing site & distribution (EXP-11)

### 11.1 Site overhaul (`apps/marketing` — Vite/React, custom OKLCH CSS, keep the stack)

- **Hero**: **"Make your app exponential."** — the loop as the centerpiece: *feedback widget →
  issue → Claude codes → PR ships → reporter gets the resolution email*. Animated loop
  visualization (the existing `TerminalDemo` choreography style, extended to the full circle).
- **Live widget demo**: embed the REAL feedback widget on the marketing site (loader from
  `app.exponential.at/widget/v1/loader.js` + a dedicated demo `expw_` key/config; the
  submission lands on the public feedback board — visitors experience step 1 of the loop for
  real, zed.dev-style).
- **IDE showcase** section (screenshots/screen recordings of the desktop: Start coding,
  terminal, source control, Changes tab) + **mobile section** with the steer-from-phone story.
- **Downloads**: resurrect `DownloadSection.tsx` (currently dead code) with CORRECTED copy
  (Rust/gpui — the current text says Swift/ghostty + Zig/GTK4!), wired to real GitHub
  Releases latest URLs (`lib/links.ts` TODO(launch) placeholders).
- **Pricing page**: per-seat model (§3), self-host free card + **Enterprise contact-sales**
  card. Remove `FoundingCallout`.
- Fix the drifted claims: push notifications shown as paid (`pricing.ts:24`), stale stack
  descriptions.

### 11.2 Desktop distribution

- `build-desktop.yml`: on `desktop-v*` tags, **publish production-channel artifacts to GitHub
  Releases** (today: private Actions artifacts only) with stable `latest` URLs + SHA-256
  checksums. Staging channel stays artifacts-only.
- **macOS**: Developer ID Application cert + `codesign --options runtime` +
  `xcrun notarytool` in CI (secrets: cert p12, notary API key) + staple; package as `.dmg`
  (create-dmg) — Gatekeeper-clean downloads. (Apple Developer account: release checklist.)
- **Linux**: AppImage as-is + checksum.
- **Update banner** (in scope, L28): the app checks the GitHub Releases API on launch
  (debounced, e.g. daily) and shows a "new version available → download" banner. Full
  Sparkle/zsync auto-update is post-release.

### 11.3 Mobile distribution (fastlane)

**fastlane is the deployment mechanism for both stores from day one** (decision revised
2026-07-05: was "manual-first"). Lanes live in-repo and are runnable locally on the Mac; CI
wiring is optional sugar on top, not a prerequisite.

- **Android** (`apps/android/fastlane/`): release keystore + `signingConfigs` in
  `app/build.gradle.kts` fed by env/gradle-properties (unsigned fallback keeps CI green
  pre-keystore). Lanes: `build` (signed `.aab` + APK), `internal` (upload to the Play
  internal track via `supply`, service-account JSON from env), `production` (promote).
  Store listing metadata (title/descriptions/changelogs) checked into
  `fastlane/metadata/android/` so listings are versioned.
- **iOS** (`apps/ios/fastlane/`): lanes on top of Tuist — `build` (tuist generate + `gym`
  archive of the `Exponential` scheme), `beta` (`pilot` → TestFlight), `release`
  (`deliver` → App Store submission). Signing via cert + provisioning profile managed with
  `match` (private repo or Developer-portal-managed profiles — decided at account setup);
  App Store Connect API key (env) for non-interactive upload. Runs on the local Mac; no CI
  Mac runner needed for launch.
- `docs/release-android.md` / `docs/release-ios.md` document the one-time setup (accounts,
  keys, keystore/match bootstrap) and the per-release two-liner (`bundle exec fastlane …`).
- Store listing assets (screenshots per device size, privacy policy URLs from the marketing
  site) ride the fastlane metadata dirs.

---

## §12 Execution plan — phases P0–P5

Orchestration: a **dynamic Workflow of subagents**. Rules (per the standing model split):
**default every packet agent to Opus**; use Fable ONLY for (a) the P0.a billing/Creem contract
review, (b) the end-of-phase adversarial verify packets, (c) the final release-readiness
review. Packets name their files (§3–§11 above carry the file:line grounding) and must leave
their gate green: web packets `bun run typecheck` + `bun run test`; desktop `cargo build` +
`cargo test -p <crate>`; Android `bun run android:build`; iOS `tuist generate` + simulator
`xcodebuild build`; widget `bun run test:widget && bun run build:widget`; marketing
`cd apps/marketing && bun run build`.

Dependency shape: P0 first (server contracts); P1–P4 run **in parallel** after their P0
dependencies; P5 last. Each phase ends with an independent **verify packet** (re-run gates,
grep for leftovers, exercise flows per `/verify`).

### P0 — Server foundation (everything client waves depend on)

| Packet | Scope | Key refs |
|---|---|---|
| P0.a | **Creem seat spike** + subscription→workspace binding schema (§3.3 1–2). Gate: seat-quantity checkout round-trip proven on Creem test mode. **Fable-reviewed.** | `auth-schema.ts:134`, `lib/auth/index.ts:329` |
| P0.b | `billing.ts` rewrite: per-seat limits, workspace-bound plan resolution, delete dead axes + call sites, agent-excluded counts, widget Pro gate, invisible free workspace guard (§3.3 3–4) | `billing.ts`, `trpc/{projects,repositories,coding-sessions,steer,workspace-invites,widgets}.ts` |
| P0.c | Billing UI: plan-comparison (3 per-seat columns + seat picker), billing-section (seat/storage/widget usage), FOUNDING removal, Infinity/null convention (§3.3 5) | `plan-comparison.tsx`, `billing-section.tsx`, `use-billing.ts` |
| P0.d | Default-branch server fixes + backfill (§8.12 server half) | `trpc/repositories.ts:88-93,163-201`, `github-app.ts:123-131` |
| P0.e | MCP expansion — all §10 tools + tests | `lib/mcp/tools.ts` |
| P0.f | Preview-target deletion: domain types, contract.json + regen, web dialog shrink, `updatePreviewConfig` simplification, delete root `.exponential/config.json` (§7.1) | `domain.ts:201-375`, `project-preview-settings-dialog.tsx` |
| P0.g | Anonymous issue detail (route guard + shape audit) (§4.3) | `issues/$issueIdentifier.tsx:19-26` |
| P0.h | Repos-cache refresh input + setup-handler invalidation (§6.1) | `trpc/integrations.ts:15-19`, `setup.ts` |
| P0.v | **Verify packet** (Fable): gates green, fresh-DB migrate, seat checkout e2e on staging Creem, grep for deleted-axis leftovers | |

### P1 — Web wave (after P0.b/f/g/h)

| Packet | Scope |
|---|---|
| P1.a | Duplicates interception + menu removal + create-dialog status trim (§4.1) |
| P1.b | Mobile-web cleanup + empty bar + topbar/drawer dedup (§4.2 1) |
| P1.c | Right-click fix; Ctrl+F global search (state lift + shortcut) (§4.2 2-3) |
| P1.d | `isAgent` filter at `useWorkspaceUsers` source (§4.2 4) |
| P1.e | Newline round-trip: reproduce → fix → parity test (§4.2 5) |
| P1.f | Integrations page + nav removal, setup-redirect retarget (§6.2) |
| P1.v | Verify packet: e2e pass (anonymous board view, duplicate flow, mobile viewport, create-with-blank-lines) |

### P2 — Desktop wave (after P0.d/f; parallel with P1/P3/P4)

| Packet | Scope |
|---|---|
| P2.a | Editor: always-editable + structural-edit save commit (§8.1-2) |
| P2.b | Detail padding + sidebar color dots (§8.3-4) |
| P2.c | **Create-project fix** with registry repo picker (§8.5) |
| P2.d | API-key card removal + account-menu parity (§8.6, 8.9) |
| P2.e | Numeric sort + terminal panel (click-to-shell, collapse, no zoom) (§8.7-8) |
| P2.f | Default-branch consumption (token value everywhere, kill `main` fallback) (§8.12 desktop half) |
| P2.g | Run-configs editor: single-line command UX + **Create with Claude** (scoped `.mcp.json`) (§7.2-3) |
| P2.h | File browser design pass + multi-tab viewer (§8.10) — largest item, sequenced last |
| P2.i | Navigation: back button + keybinding + breadcrumbs; Cmd+F quick search; duplicate interception (§8.11, 8.13-14) |
| P2.v | Verify packet: cargo tests, manual checklist (worktree launch on a `master`-default repo MUST pass — the EXP-8 screenshot repro) |

### P3 — Mobile wave (after P0; §9.1 first — it blocks all iOS verification)

| Packet | Scope |
|---|---|
| P3.a | **iOS sync**: device diagnosis → root-cause fix → diagnostics surfacing + migration fixture tests (§9.1) |
| P3.b | iOS: Integrations removal + feedback URL (§9.2). ~~Workspace/project-creation + onboarding removals~~ rescinded by L31 (2026-07-07) — full onboarding stays |
| P3.c | Android: same amended set + dead code (§9.2) |
| P3.d | Android: crash repro + fix + MarkdownView hardening (§9.3) |
| P3.e | Android: long-press rows; single FAB project-only (§9.3) |
| P3.f | Both: duplicate interception parity (§9.2) |
| P3.v | Verify packet: both apps build + on-device staging smoke (iOS sync live, Android issue-detail open) |

### P4 — Widget + marketing + distribution (widget after P0.b; site after §3 numbers final)

| Packet | Scope |
|---|---|
| P4.a | Widget v2: launcher (bottom-left, tiny, hover), documentElement mount/z-index, crop tool (§5) |
| P4.b | Marketing: hero + loop narrative + live widget embed (§11.1) |
| P4.c | Marketing: IDE/mobile showcase + downloads section (corrected copy) + pricing page + enterprise contact (§11.1) |
| P4.d | Desktop CI → GitHub Releases + checksums; macOS Developer ID sign + notarize + `.dmg`; update banner (§11.2) |
| P4.e | Android signing (`.aab`) + release docs groundwork (§11.3) |
| P4.f | fastlane lanes for both platforms: `apps/android/fastlane` (build/internal/production via supply) + `apps/ios/fastlane` (gym/pilot/deliver on Tuist), metadata dirs, updated release docs (§11.3) |
| P4.v | Verify packet: widget demo on marketing preview, a real download→install→launch on both OSes from a draft release |

### P5 — Release

1. CLAUDE.md + memory sync to v5 realities; prune stale docs.
2. Staging reset (documented Coolify procedure) + **full four-client smoke** against staging
   (the §13 script).
3. Manual steps of §13 (Creem, Apple, Google, DNS).
4. Production greenfield reset (v2 commitment) + deploy all Coolify apps + `git push` (commits + tags):
   `v1.0.0`, `desktop-v1.0.0`, `android-v1.0.0`.
5. Store submissions (iOS via Xcode, Android via Play Console) — approval NOT gating (L28).
6. Marketing site live with real download URLs; announce.

**Carried-over debts that ride along in P5 unless picked earlier** (from v4):
wss:// TLS in the desktop WS client + publisher auto-reconnect + relay-unreachable kill
honoring; live relay-presence device pickers. (Native `#`-autocomplete and the
digest-batching email cron both landed 2026-07-07 — the hourly push-first digest sweep in
`apps/web/src/lib/notification-email-digest.ts` and `#`-autocomplete on all four clients.)

---

## §13 Release checklist (manual, human-only steps)

- [ ] **Apple Developer Program** account ($99/yr) — needed for BOTH notarized desktop
      downloads and iOS App Store. Create Developer ID Application cert + notary API key →
      GitHub Actions secrets.
- [ ] **Google Play Console** account ($25 one-time). Generate the Android release keystore
      (store passwords in GitHub Actions secrets + offline backup — losing it is fatal). Create the
      Play **service-account JSON** for fastlane `supply` (Play Console → API access).
- [ ] **App Store Connect API key** (Users & Access → Integrations) for fastlane
      `pilot`/`deliver`; decide `match` storage (private git repo) vs portal-managed
      profiles and bootstrap signing once.
- [ ] **Creem dashboard**: create Pro-yearly (per-seat), Business-monthly, Business-yearly
      products; set env ids on cloud + staging; **delete the FOUNDING coupon**.
- [ ] Create the demo `expw_` widget config for the marketing-site embed.
- [ ] Store listings: screenshots (per-device sizes), descriptions, privacy policy URLs.
- [ ] Production greenfield reset window + Coolify redeploys (LAN-only, manual).
- [ ] DNS/download sanity: releases URLs live before the marketing deploy flips.

---

## §14 Do-not-regress contract

Everything in v4 §9 carries forward verbatim (shape lockstep 14/14, proxy hardening, JIT
tokens, argv git, one launch entry point, terminal invariants, run-config security, markdown
byte-parity, widget users, model explicitness, deploy realities) **plus**:

1. **Seat gating is server-side only** and always excludes `isAgent` users; never lock
   existing members out on downgrade (invites block, access stays).
2. **Push + steer stay free** — no plan checks may appear on notification delivery or
   steer-ticket minting beyond existing auth (L22).
3. **Anonymous public reads** (L29): route guards may gate on membership only for non-public
   workspaces; the users shape stays members-only.
4. **No `main` assumptions** (L30): new code paths must consume the healed default branch
   (token or healed row), never a literal.
5. **argv-direct spawn** for run configs survives the single-line editor UX — the line is
   parsed to argv, never handed to a shell (L23).
6. **Claude tasks stay visible**: the run-config task's scoped `.mcp.json` (L24) is the ONLY
   MCP-enabled task; conflict-fix tasks stay MCP-less; no task ever creates `coding_sessions`
   rows.
7. **Markdown newline fix must not break byte-parity** — Android's parity suite is the lock;
   any serialization change lands with cross-client fixtures.
