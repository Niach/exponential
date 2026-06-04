# Agent-System Redesign — Session Handoff (→ macOS)

> Written 2026-06-04 at the end of a long Linux session, to hand off to a fresh
> session on a Mac (where Xcode/Tuist exist). This documents the **whole goal**,
> **everything built**, and **what's left** — primarily Phase 8 (the macOS
> client) plus build-verifying the iOS/macOS Swift written blind on Linux.
>
> Companion docs: `docs/native-desktop-roadmap.md` (architecture), and the full
> approved plan is in the Linux box's `~/.claude/plans/please-lets-refine-the-steady-dove.md`
> (reproduced in §3 below so it's available on the Mac).

---

## 0. TL;DR — where we are

- **Goal:** turn the agent from a brittle headless daemon into a **first-class,
  human-owned actor** with OAuth registration, an **interactive mode** (watch/steer
  `claude` live in an embedded terminal), **synced PR state**, a **diff view**, a
  **subscription model**, a **unified inbox**, and a **Linear-style activity log** —
  across web, iOS, Android, Linux desktop (Zig/GTK4), macOS (SwiftUI), the Rust
  `agent-core`, and the shared domain-contract.
- **Done + verified:** Phases 0–7 (web, server, agent-core, Android, Linux desktop)
  AND a **GitHub web-OAuth refactor** (server + agent-core + Linux). See §4.
- **Written but UNVERIFIED (needs a Mac build):** all iOS Swift from Phase 6, and
  the macOS Swift touched so far. There is **no Xcode/Tuist on the Linux box**, so
  every `apps/ios/**` change is "to-pattern" only.
- **Remaining work (do on the Mac):** Phase 8 (full macOS client parity) + the
  macOS GitHub host change + `tuist generate` & build-fix the iOS/macOS Swift +
  a handful of follow-ups (§6).
- **Git:** everything through Phase 7 is committed at `5b5142f` (tag
  `v0.16.23-dev`, deployed to staging). The Linux UI fixes + the GitHub refactor
  are in the **same commit you're reading this from** (committed alongside this
  doc). `git pull` on the Mac gets it all.

---

## 1. The vision (what "done" looks like)

An owner installs the desktop app, logs in once (OAuth), clicks **"Register this
machine"** — no token paste. From then on:

1. **Assign an issue to the agent** (web / mobile / desktop) → the agent runs a
   **plan-only** pass in the background and posts the plan as a comment +
   `plan_ready` activity event + inbox entry.
2. **Approve** the plan non-interactively (web/mobile → runs the code stage in the
   background) **or** on the desktop click **"Approve & continue here"** → the
   agent continues **interactively** in an embedded ghostty terminal you can watch
   and steer.
3. Separately, a desktop **"AI" button** starts an interactive `claude` session at
   any issue directly (plan mode, MCP preconfigured, plan delivered via MCP tool).
4. The agent pushes a branch; the **server opens the PR** (one issue = one PR =
   one branch/worktree); `pr_*` columns sync everywhere; a **diff view** renders
   on web + desktop.
5. **Subscribers** (creator/assignee/commenter/mention, unsubscribable) get inbox
   entries + (plan-gated) push. The **activity timeline** shows status/assignee/
   label/PR/plan/error events merged with comments on every platform.

GitHub auth: the owner connects GitHub **once in the web app** (Better Auth
`linkSocial`); the **server** does PR creation + diff with that token; the
**desktop agent** fetches it just-in-time for clone/push.

---

## 2. Architecture decisions (locked)

| # | Decision |
|---|---|
| D1 | **Run model:** desktop-app-open only. Assigning enqueues; the owner's running desktop picks it up via the `assigned-issues` Electric feed + dispatcher. No headless daemon. |
| D2 | **Agent identity:** a distinct actor (own name/avatar) **owned by** a human. Hidden synthetic `users` row for FK integrity + `users.is_agent` (replaces the old `email LIKE 'agent-%'` hack) + `workspace_agents.owner_user_id`. The gate is `isActingAsAgent(userId, workspaceId)`. |
| D3 | **Auth:** no setup token, no public `claimSetup`, no never-expiring `expk_`. Registration is **human-session-authorized** via `companion.register`, minting a **refreshable OAuth credential** (per-agent `oauth_applications` + `oauth_access_tokens`). The single `getSession()` chokepoint widened (`lib/auth/resolve-bearer.ts`) to also resolve the opaque MCP token. **Hard cutover.** `companion.*` route names kept. |
| D4 | **Two modes:** interactive (AI button → ghostty `claude` **no `--print`**, WITH `--dangerously-skip-permissions --permission-mode plan`, `.mcp.json` preconfigured; the plan is submitted **out-of-band** via the `exponential_agent_plan_submit` MCP tool — no stdout parsing) vs non-interactive (background plan-only → approve). |
| D5 | **One issue = one PR = one branch/worktree.** Synced `issues.pr_url/pr_number/pr_state/branch/pr_merged_at` columns replace the comment-regex state machine. |
| D6 | **Diff view** on web + Linux + macOS via **one** server endpoint `issues.prFiles`. Not on mobile. |
| D7 | **Subscriptions:** `issue_subscribers` table; auto-subscribe on create/assign/comment/mention; a manual unsubscribe **suppresses** future auto-resubscribe. Drives inbox + push. The notification **row-write is decoupled from `canUsePush`** (inbox works on free plans; only push is plan-gated). |
| D8 | **Inbox:** unified, grouped by issue, "For me" / "Needs your review". Per-user `notifications` Electric shape. |
| D9 | **Activity log:** `issue_events` table → Linear-style timeline on **all** platforms. |
| D10 | A subscriber comment on an agent-assigned issue is picked up as next-run steering (existing `decide_stage` + `agentLastCommentSeenAt`). Composer hint "incorporated on next run". |
| D11 | **Terminal docking:** collapsible IDE-style bottom dock (Linux `GtkPaned`; macOS `NSSplitView` bottom pane), NOT a throwaway window. **Ghostty inits its surface lazily only at nonzero size** — only mount while expanded at a real height. |
| D12 | **Cancel/rerun:** cancel via `companion.pollControl` → `agent_core_cancel_run`; rerun = existing `agentPlan.retry`. |
| D13 | **`apps/push-relay` is unchanged.** Only the web fan-out + `data.type` vocabulary changed. |

**GitHub web-OAuth (decided this session, supersedes the desktop device flow):**
single credential via web `linkSocial`; server does PR create + diff + (future)
merge detection; desktop does clone/worktree/commit/**push** with a token fetched
from the server. Each instance registers its own GitHub OAuth App
(`GITHUB_CLIENT_ID`/`SECRET`). GitHub **App** (short-lived repo-scoped installation
tokens) is the hardening upgrade. See `~/.claude/.../memory/project_github_web_oauth.md`
on the Linux box, summarised in §4.G.

**Shape count: 10 → 13** (`notifications`, `issue_events`, `issue_subscribers`).
Must stay in lockstep across: `CLAUDE.md`, web `collections.ts`, Zig
`sync_manager.zig` `specs[]` (+ its `expectEqual(13, specs.len)` test), and the
iOS/Android entity/DAO lists.

---

## 3. The phased plan (status per phase)

> Strict order: 0 blocks all; 1 (auth) blocks 5 (PR/MCP) + 7 (desktop); 2 (native
> sync) precedes GUI phases.

- **Phase 0 — schema/contract/trigger floor.** ✅ DONE + verified. Enums + columns
  + 3 tables + triggers; migrations `0005`/`0006`; codegen regenerated.
- **Phase 1 — OAuth registration + agent-as-subuser (server).** ✅ DONE + validated
  live. The token-minting spike resolved (see §5 gotchas).
- **Phase 2 — native sync foundations (Linux + mobile data layer).** ✅ DONE.
  Linux + Android build-verified; iOS to-pattern only.
- **Phase 3 — subscriptions + fan-out + activity + 3 web shapes (server/web data).**
  ✅ DONE + validated e2e. Mentions in-scope (CLAUDE.md was wrong).
- **Phase 4 — web UI (inbox, timeline, agent panel, diff, assign, subscribe).**
  ✅ DONE; typecheck + 52 tests green.
- **Phase 5 — agent-core interactive + PR write-back.** ✅ DONE; cargo 62 tests.
- **Phase 6 — native refresh + mobile UI.** ✅ Android build-verified; **iOS
  written but UNBUILT** (needs Mac). Registration cutover validated e2e.
- **Phase 7 — Linux desktop (dock, AI button, approve-interactively, timeline,
  diff).** ✅ 10/11 steps; all `zig build` + `zig build test` green.
- **Phase 8 — macOS client + cleanup.** ⛔ **NOT STARTED — this is the main
  remaining work, to be done on the Mac.** See §6.

---

## 4. What was built this session (detail)

Everything below is **committed** (this commit) unless marked. Verification noted.

### 4.0 Phase 0 — schema/contract
- `packages/db-schema/src/domain.ts`: enums `prStateValues`, `runModeValues`,
  `subscriberSourceValues`, `issueEventTypeValues`, extended `agentPlanStateValues`
  superset, `notificationTypeValues` + zod schemas/types.
- `packages/domain-contract/contract.json` + `scripts/generate.ts` emit blocks
  (Swift/Kotlin/Rust/Zig) + `src/index.ts`; drift test in
  `apps/web/src/lib/domain-contract.test.ts`. Regenerate:
  `bun run --filter @exp/domain-contract generate`.
- `packages/db-schema/src/schema.ts`: `issues` pr_*/agent cols; `workspace_agents`
  `owner_user_id`/`oauth_client_id` + setup_token_* nullable; new `issue_subscribers`
  + `issue_events` tables.
- Migrations `apps/web/src/db/out/0005_*.sql`, `0006_*.sql`; triggers appended to
  `0001_triggers.sql` (`populate_issue_subscriber_workspace_id`,
  `populate_issue_event_workspace_id` via issue→project; `update_updated_at`).
  **Custom triggers are NOT auto-applied by deploy** — `docker exec … psql <
  0001_triggers.sql`.

### 4.1 Phase 1 — auth (server)
- `apps/web/src/lib/auth/resolve-bearer.ts` (NEW) — `resolveSession` (getSession →
  getMcpSession → synthetic session), `resolveSessionUserId`, `resolveMcpUserId`.
  Wired into `lib/shape-route.ts`, tRPC ctx (`routes/api/trpc/$.ts`), `routes/api/mcp.ts`.
- `apps/web/src/lib/auth/agent-credential.ts` (NEW) — `mintAgentCredential` (direct
  insert per-agent public `oauth_applications` + `oauth_access_tokens`). Token
  lifetimes via `mcp({ oidcConfig: { accessTokenExpiresIn, refreshTokenExpiresIn }})`.
- `lib/trpc/companion/setup.ts` — `register` replaces create/regenerateSetup/claimSetup;
  kept `list` + `revoke` (revoke deletes oauth client → cascades tokens).
- `components/workspace/agents-section.tsx` gutted to "register from the desktop
  apps" + list + revoke. Deleted `public/install/*.sh`.

### 4.2 Phase 2 — native sync (the 3 new shapes + 8 issue cols)
- **Linux** `apps/linux/src/core/db/migrations.zig` (schema_sql + idempotent
  self-heal ALTERs) + `core/electric/sync_manager.zig` `specs[]` + `tests.zig`.
  Persistence is **generic** (column-wise from the JSON shape msg) — see §5 the
  `tableColumnSet` filter fix.
- **Android** `data/db/Entities.kt`/`Daos.kt`/`ExponentialDatabase.kt` (Room v bump,
  destructive-fallback resync) + `SyncManager.kt`.
- **iOS** `ExpCore/Sources/DB/Entities.swift` (3 entities) + `DatabaseManager.swift`
  additive `registerMigration("v3_agent_system")` (do NOT bump the `-v2` file
  suffix) + `SyncManager.swift`. **UNBUILT.**

### 4.3 Phase 3 — subscriptions / fan-out / activity / web shapes
- `routes/api/shapes/{notifications,issue-events,issue-subscribers}.ts` + collections
  in `lib/collections.ts`.
- `lib/integrations/subscriptions.ts` (`ensureSubscribed`), `activity.ts`
  (`recordIssueEvent`), rewritten `notifications.ts` (subscriber fan-out, row always
  written, push gated by `canUsePush`), `mentions.ts` (`@<email>` → members).
- `lib/trpc/{subscriptions,notifications}.ts` routers. Producers wired in
  `issues.ts`, `comments.ts`, `issue-labels.ts`, `agent-plan.ts`.

### 4.4 Phase 4 — web UI
- `routes/w/$workspaceSlug/inbox/index.tsx` + `components/inbox/inbox-view.tsx`;
  `components/subscribe-toggle.tsx`, `agent-panel.tsx`, `diff-view.tsx`,
  `mention-textarea.tsx`, `comment-rows/event.tsx`; assignee People/Agents
  segmentation; `issue-timeline.tsx` merges events (comment-regex later removed in P5).

### 4.5 Phase 5 — agent-core interactive + PR write-back (Rust)
- PR write-back: `agentPlan.reportPr`/`reportError` + MCP tools; agent-core
  `mcp::report_pr`/`report_error`. Comment-regex removed from `issue-timeline.tsx`.
- Interactive: `agent_run.rs` `RunRequest{interactive, continue_session_id}` +
  `RunResult{session_id}` + `build_claude_interactive_run`; `pipeline.rs`
  `INTERACTIVE_PLAN_SYSTEM_PROMPT`; `run_pipeline.rs` `run_interactive_plan/continue`;
  `git::worktree_reuse`; `state.rs` `claude_session_id`/`interactive_owned`;
  dispatcher suppresses reentry when `interactive_owned`; `ffi.rs`
  `agent_core_request_interactive`/`approve_interactive` + `submit_run_result`
  gained `session_id` (C ABI; **5-arg now** — Zig + Swift call sites updated).
  `crates/agent-core/include/agent_core.h` updated.

### 4.6 Phase 6 — mobile UI
- **Android (build-verified):** inbox (`ui/inbox/*`, `NotificationsApi`,
  `SubscriptionsApi`, `IssueDao.observeAll`), subscribe toggle, assign-to-agent
  segmentation (`users.is_agent` on `UserEntity`, Room v4 bump), activity timeline
  (CommentThread merges issue_events). Push routing already correct (FcmService
  keys on `issueId`).
- **iOS (UNBUILT, needs Mac):** `ExpCore/Sources/API/{Notifications,Subscriptions}Api.swift`
  (registered in `AppDependencies`); subscribe toggle in `IssueDetailViewModel`/`View`;
  inbox `Exponential/UI/Inbox/{InboxView,InboxViewModel}.swift` + `AppRoute.inbox` +
  tray toolbar on `HomeView`; timeline in `CommentThreadView` (sealed `TimelineItem`
  + `eventRow`); assign-segmentation (`is_agent` on `UserEntity` w/ permissive custom
  `Decodable` + `v4_user_is_agent` migration; `assigneeOptions` People→Agents).
  **`tuist generate` is needed** (3 new Swift files) then fix build errors.

### 4.7 Phase 7 — Linux desktop (Zig/GTK4)
- Interactive run handler in `agent_manager.zig` (`buildInteractiveScript`, no
  tee/PIPESTATUS, empty submit on exit). **AI button** + **"Approve & continue
  here"** in `app.zig`. **Activity timeline** (`database.zig` `listIssueEvents` +
  two-pointer merge in `showIssueDetail`). **Terminal dock** `ui/terminal_dock.zig`
  (GtkPaned, libghostty zero-height guard). **"Changes"** button opens the PR.

### 4.G GitHub web-OAuth refactor (this session, spans platforms)
- **Server (typechecks):** `lib/auth/index.ts` github `socialProvider` +
  `trustedProvider` (gated `GITHUB_CLIENT_ID`/`SECRET`); `lib/integrations/github-pr.ts`
  `resolveOwnerGithubToken` / `resolveWorkspaceAgentOwnerToken` / `resolveRepoToken`
  (actor→owner→legacy) + `createPullRequest` + `fetchPullState`; `issues.prFiles`
  uses `resolveRepoToken(actorUserId)`; `agentPlan.openPr` mutation +
  `exponential_agent_open_pr` MCP tool; `integrations.github.status`/`.token`;
  `lib/auth/config.ts` `githubEnabled`; "Connect GitHub" card on
  `routes/_authenticated/account/integrations.tsx`; `.env.example`.
- **agent-core (62 tests):** `mcp::open_pr` replaces `github::create_pull_request`
  in `code_stage` (push branch → server opens PR).
- **Linux (builds):** `reconcileAgent` `fetchGithubToken` (`integrations.github.token`)
  → `CoreConfig.githubToken`; `settings.zig` device-flow UI → "Open Integrations
  in browser" (`onOpenGithubIntegrations`). The old device-flow code
  (`onConnectGithub`/`startGhPoll`/`GhJob`/`onDisconnectGithub` + `github_auth.zig`)
  is now **dead — clean it up**.
- **Mobile = N/A** (connect is web-only).
- **Merge detection:** kept agent-core `pr_poll`→`reportPr` for v1; `fetchPullState`
  is ready for a server cron/webhook (cloud follow-up).
- **macOS = TODO (folds into Phase 8):** mirror the Linux change — in
  `ExponentialMac/MacAgentService.swift` `startAgent`, replace
  `MacAgentStore.githubToken()` (line ~192) with a token fetched from
  `integrations.github.token` (human session) via the `trpc` helper; remove the
  device-flow UI in `MacAgentService`/`MacIntegrationsView`/`MacSettingsView`
  (the `// MARK: - GitHub device flow` block) → point at the web app.

---

## 5. Gotchas & non-obvious things (READ before coding)

- **Token mint spike (the load-bearing unknown):** Better Auth `mcp` plugin IS
  the oidcProvider; `getMcpSession` is a plain DB lookup on
  `oauth_access_tokens.access_token` **with no expiry check** → the agent's access
  token works ~forever, so **no refresh loop is required for v1**. Refresh grant at
  `/api/auth/mcp/token` accepts a `type:"public"` client (client_id + refresh_token,
  no secret). Token lifetimes are read from `oidcConfig`, NOT the top-level mcp opts.
- **OAuth redirect ≠ inbound.** GitHub/Google OAuth redirects the **browser**; the
  token exchange is an **outbound** server call. So self-hosted LAN instances work.
  Don't "fix" self-hosted GitHub thinking it needs a public ingress.
- **Linux sync drops rows on an unknown column.** The Zig client builds
  `INSERT INTO <table> (<payload cols>)` from the Electric payload — if the server
  adds a column the local table lacks (e.g. `users.is_agent`, or auth fields), the
  statement fails to prepare and the **whole row is dropped**. Fixed via
  `database.zig::tableColumnSet` (skip columns not present locally) + the
  `migrations.zig` self-heal ALTER. **Mirror this defensiveness if you touch the
  iOS/Android generic upsert.**
- **ghostty zero-height trap (D11):** the GL surface inits lazily only at nonzero
  size. On macOS, host `MacGhosttyTerminalView` in a collapsible `NSSplitView`
  bottom pane and only mount while expanded at a real backing size (Linux dock does
  this: min height 200 + expand-before-mount).
- **"Approve & continue here" is a HUMAN action.** The agent credential can't
  self-approve. The desktop host approves with the **human** session
  (`agentPlan.approvePlan`) THEN calls `agent_core_approve_interactive` (resume only).
- **tRPC error code is `PRECONDITION_FAILED`** (not `FAILED_PRECONDITION`).
- **`EmptyInput` name clash (Android):** the `data.api` package already has a
  `private object EmptyInput` (WorkspacesApi) — use a prefixed name.
- **Custom SQL triggers aren't auto-applied** by the Docker image (`CMD` only runs
  `drizzle-kit migrate`). Apply `0001_triggers.sql` manually after a fresh DB.
- **`agent_core_submit_run_result` is now 5-arg** (trailing `session_id`). Any new
  host call site must pass it (NULL for headless).
- **GtkPaned/`allocPrintSentinel` format strings must be comptime** in Zig — split
  runtime-selected strings into branches.

---

## 6. Remaining work (do this on the Mac)

### 6.A macOS GitHub change (small — closes the refactor on all platforms)
In `apps/ios/ExponentialMac/MacAgentService.swift`: `startAgent` builds the
agent-core config JSON with `"githubToken": MacAgentStore.githubToken()`. Replace
with a token fetched from `integrations.github.token` (use the **human session**
bearer of the owning account, via the existing `trpc` helper — note it may be
POST-only today; tRPC queries are GET, so you may need a GET variant). Remove the
device-flow block (`// MARK: - GitHub device flow`) and the "Connect GitHub" UI in
`MacIntegrationsView`/`MacSettingsView` → point at `/account/integrations` in the
browser. Drop `MacAgentStore.saveGithubToken/githubToken` once unused.

### 6.B Phase 8 — full macOS client parity
The macOS app (`apps/ios/ExponentialMac/*`) is a complete ExpCore-backed SwiftUI
app. Mirror Phase 4 (web) + Phase 7 (Linux) on it:
1. **Inbox** in `MacRootView` nav (mirror iOS `InboxView`/`InboxViewModel`).
2. **Activity timeline** — render `IssueEventEntity` merged with comments in
   `MacIssueDetailView`/`CommentThread`.
3. **Assign-to-agent** picker segmentation (`MacIssueControls`); `is_agent` is
   already on the iOS `UserEntity` (Phase 6).
4. **Subscribe toggle** + **agent panel** + **diff view** (via `issues.prFiles`).
5. **AI button** + **"Approve & continue here"** on the SwiftUI issue detail (the
   FFI entry points `agent_core_request_interactive`/`approve_interactive` exist).
6. **Docked terminal:** host `MacGhosttyTerminalView` in a collapsible
   `NSSplitView` bottom pane (not a throwaway `NSWindow`); same nonzero-backing-size
   guarantee; branch interactive vs headless on the run_request `interactive` flag;
   on exit `submit_run_result(..., nil)` for headless / the session id for interactive.
7. Registration cutover already done in Phase 6 (`companion.register`); just verify.

### 6.C Build-verify the blind Swift (iOS + macOS)
- `cd apps/ios && tuist generate` then build both schemes. Phase-6 iOS added **3 new
  files** (`InboxView`, `InboxViewModel`, and the two API clients) — Tuist globbing
  should pick them up; fix any build errors. Watch the GRDB `ValueObservation`
  closures, the `@Observable` VMs, `AppRoute.inbox` wiring, and the `UserEntity`
  custom `Decodable`.

### 6.D Follow-ups (not blocking)
- **Linux full in-app diff** (`diff_view.zig`): needs a URL-encode helper +
  `trpc.zig::queryInput` (the current `query` is no-input) to hit `issues.prFiles`,
  then a side-by-side patch renderer. Today the "Changes" button opens the PR.
- **Server merge poll / webhook** (move `pr_poll` off the desktop): `fetchPullState`
  is ready; add a cron over `issues` with `pr_state='open'` (works self-hosted) and
  a GitHub webhook for instant cloud updates.
- **Explicit cancel button** (D12): `companion.pollControl` → `agent_core_cancel_run`
  (needs run_id tracking). Closing the terminal already stops an interactive run.
- **Delete the dead Linux device-flow code** (§4.G) + `github_auth.zig`.
- **GitHub App** (hardening): short-lived repo-scoped installation tokens so the
  desktop never holds a long-lived token.
- **Phase-1 cutover migration:** delete legacy `expk_` agents; tighten
  `workspace_agents.owner_user_id` to NOT NULL after the window.
- **`bump_issue_updated_at_from_comment` trigger** fails during a workspace
  cascade-delete that has comments (pre-existing latent bug in admin.deleteWorkspace)
  — delete comments first, or guard the trigger.

---

## 7. Verification commands

```bash
# Web (server + UI)
bun run typecheck          # apps/web
bun run test               # 52 vitest tests
bun dev                    # localhost:5173 (needs backend:up)

# Backend
bun run backend:up         # Postgres:54321 + Electric:30000 + Garage + Caddy
bun run migrate            # drizzle migrations
docker exec -i exponential-postgres-1 psql -U postgres -d exponential < apps/web/src/db/out/custom/0001_triggers.sql

# agent-core (Rust)
cargo build -p agent-core
cargo test -p agent-core   # 62 tests

# Linux desktop (Zig) — content-hashed; a deliberate-error probe is the reliable
# way to confirm a file compiled. `pkill -x exponential` to stop a running GUI
# (NEVER `pkill -f zig-out/bin/exponential` — it matches your own shell command).
cd apps/linux && zig build && zig build test

# Android
bun run android:build      # ./gradlew :app:assembleDebug

# iOS / macOS (MAC ONLY)
cd apps/ios && tuist generate && <build in Xcode>

# Contract codegen (after any enum change)
bun run --filter @exp/domain-contract generate
```

---

## 8. Environment & deploy

New env this session (per instance — register your own GitHub OAuth App):
```
GITHUB_CLIENT_ID=            # GitHub OAuth App (Settings → Developers)
GITHUB_CLIENT_SECRET=        # callback: ${BETTER_AUTH_URL}/api/auth/callback/github
```
(`EXPONENTIAL_GITHUB_OAUTH_CLIENT_ID`, the old device-flow client id, is now legacy
and can be dropped once the device flow is deleted.)

**Set `GITHUB_CLIENT_ID`/`SECRET` on staging (`exponential-next-web`,
`i2h9ozcemp70yigkf8jylaq2`) and prod** before testing the agent PR flow.

Staging is on `v0.16.23-dev` (commit `5b5142f`) at https://next.exponential.at —
the new schema migrated on deploy (the image `CMD` is `migrate && start`).
Release flow: `/release-staging` (commit → tag `-dev` → pushsync → GHCR build →
`coolify deploy uuid i2h9ozcemp70yigkf8jylaq2`). Custom triggers still need a manual
`docker exec … psql` on the staging DB.

**GitHub-connect was failing on staging only because `EXPONENTIAL_GITHUB_OAUTH_CLIENT_ID`
wasn't set** (the OLD device flow). With the new web `linkSocial`, set
`GITHUB_CLIENT_ID`/`SECRET` instead and connect via `/account/integrations`.

---

## 9. Key file map (by platform)

- **Server/web:** `apps/web/src/lib/{auth,trpc,integrations,mcp,collections}.ts(x)`,
  `apps/web/src/routes/{api/shapes,api/trpc,api/mcp,w/$workspaceSlug/inbox,_authenticated/account/integrations}`,
  `packages/db-schema`, `packages/domain-contract`.
- **agent-core (Rust):** `crates/agent-core/src/{agent_run,pipeline,run_pipeline,
  dispatcher,state,git,github,mcp,trpc,pr_poll,ffi}.rs` + `include/agent_core.h`.
- **Linux (Zig):** `apps/linux/src/ui/{app,settings,terminal_dock,gtk,terminal}.zig`,
  `apps/linux/src/core/{agent,db,electric,api,auth}/*.zig`.
- **macOS (Swift):** `apps/ios/ExponentialMac/*.swift` (Mac*).
- **iOS (Swift):** `apps/ios/Exponential/**` + `apps/ios/ExpCore/Sources/**`.
- **Android (Kotlin):** `apps/android/app/src/main/java/com/exponential/app/**`.

The Linux box's `~/.claude/projects/-home-niach-Projects-2026-exponential/memory/`
has the running notes (`project_agent_redesign.md`, `project_github_web_oauth.md`,
`project_libghostty_embed.md`, etc.) — not in git, but the salient bits are
reproduced above.
