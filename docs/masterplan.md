# Exponential — Masterplan

**The execution blueprint for the Fable 5 refactor.** Read [`vision.md`](./vision.md) first for the north star; this is how we get there.

## Purpose & how to use this

This masterplan turns the vision into an ordered, verifiable refactor, written for a strong implementer (Fable 5) to execute top-to-bottom. **Section 1 is the delete-list + the do-not-regress contract.** Sections 2–8 are the detailed workstream specs. **Section 9 is the sequenced phase plan — start there for ordering, and treat Sections 1–8 as the spec each phase references.**

This is a **hard-cut refactor.** All existing code is treated as 100% replaceable, the database is **greenfield** (no data to preserve, no migrations — reset and re-migrate), and one entire subsystem — the Rust `agent-core` — **is deleted outright.** Where the earlier plan hedged, this one cuts.

## The core simplification

The v1 plan carried a heavy agent runtime forward: a ~4,860-line Rust `agent-core` cdylib with a frozen C ABI, a `run_request`↔`submit_run_result` handshake, a headless pipeline, an assignment-triggered dispatcher, a synthetic per-device agent user, and structured plan/approval state in `agent_runs`. **All of that is gone.** With headless runs, assignment-dispatch, and synthetic identity cut, the C-ABI boundary buys nothing.

The new coding flow is a small **native "Start coding" launcher** on each desktop:

> Resolve the issue's repo from the workspace registry → fetch a JIT GitHub-App token from the server → create a git worktree + `exp/<IDENTIFIER>` branch with a token-embedded remote → write a `.mcp.json` pointing at the web MCP toolset → spawn **`claude --dangerously-skip-permissions`** in an embedded libghostty terminal with a **plan-first prefilled prompt**. From there it's fully interactive; Claude commits, pushes, and opens its own PR via the MCP `open_pr` tool.

You work **as yourself**, not a bot. The "toolset" Claude uses is the existing web `/api/mcp` extended with `get_issue`/`update_status`/`open_pr` (all backed by the server's storage-free GitHub App). **Local dependencies are only `claude` + `git` — never `gh`.** Multi-window becomes trivially native (each tab/window is its own ghostty + Claude child in its own worktree — no shared slot pool, no core). The phone drives the same launcher remotely over the relay, and steers the live terminal bidirectionally.

## Where we stand today

Exponential is already a genuinely good multi-platform Linear alternative: five clients sync a shared set of Electric shapes, the issue/comment/label/**inbox**/subscription/@-mention/timeline/diff machinery is shipped, libghostty terminals are embedded on both desktops, and the GitHub App integration is storage-free and correct. The recent `a8826d2`/`084aa97` commits already built most of the host-side **run-config / play-button / preview** infra. What this refactor does is **subtract** the heavy agent runtime, **add** the thin native launcher + web-MCP toolset + the outbound remote-steer relay + first-class repositories + a free email channel, and **finish** Linux 1:1 parity. The macOS app and much of the iOS Swift remain **runtime-unverified** — a verify-and-polish track on real hardware (minus everything agent-core-related, which is deleted, not verified).

## Locked decisions (v2)

| # | Decision | Choice |
|---|---|---|
| 1 | Rust `agent-core` | **Deleted entirely** (+ both host FFI bridges). No cdylib, no C-ABI, no `run_request`/`submit_run_result`, no dispatcher/pipeline/headless. libghostty embedding stays (separate native code). |
| 2 | Database | **Greenfield** — clean target schema, no migrations/backfill; `backend:clear` + re-migrate. |
| 3 | Agents | **Interactive-in-terminal only.** No headless / background runs. |
| 4 | Identity & trigger | **No synthetic agent user, no registration, no assignment-trigger.** You code as yourself. A native **"Start coding" button** launches ghostty + `claude`. Drop `agent_registrations`, the `companion`/`agent` router, `expk_` keys, the `assigned-issues` shape. |
| 5 | Permissions | Always **`--dangerously-skip-permissions`** — never make the user click accept. |
| 6 | Toolset | **Web `/api/mcp`** extended (`get_issue`/`update_status`/`open_pr`), server-side via the GitHub App. No local core. Deps: `claude` + `git` only, **no `gh`**. |
| 7 | Git auth / PR | Server mints a **JIT GitHub-App installation token**; the launcher configures a token-embedded remote for `git push`; the server opens the PR via the App. |
| 8 | Desktop settings | JetBrains-SDK-style: **Claude CLI path + workspace/repos path** (prefilled, editable) + branch prefix + personal API key. |
| 9 | Session state | **`agent_runs` deleted** → slim synced **`coding_sessions`** (live "coding now" badge + Watch/Steer). PR fields on `issues`. |
| 10 | Remote control | Desktop holds an **outbound relay control connection** (device presence); phone **"Start on my desktop"** → relay → launcher. Full **bidirectional** watch + type; steer state in **relay memory** (no DB table). |
| 11 | Repositories | **First-class workspace entity, many-to-many, tRPC-managed** (not a synced shape). No repo → "Start coding" disabled + CTA. |
| 12 | Multi-window | **v1, trivially native** — each tab/window = its own ghostty + Claude child in its own worktree. No shared pool. |
| 13 | PR review | **Read-only syntax-highlighted diff on all platforms**, schema-anchored for later write-back. |
| 14 | Email + push | **Both free / table-stakes**; monetize on members/projects/repos/agents-capacity. |
| 15 | Cut list | Kanban, saved filters, cycles, sub-issues/deps, time tracking, estimates, custom fields, bulk edit, templates, agent marketplace, MCP browser, presence, timeline/Gantt, roadmap share, Linear import, **Google Calendar** — all cut. |
| 16 | Keep | **Issue-to-issue linking**, **duplicate** (`status='duplicate'` + `duplicateOfId`), **My Issues**, one-way widget **helpdesk**. |

## Target shape count

**14 synced Electric shapes:** workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, **coding_sessions**. Everything else — `repositories`, `project_repositories`, `user_notification_prefs`, `email_deliveries`, `widget_configs`, `widget_submissions` — is **server-only (tRPC)**. Steer state is **relay memory**, not a table.

## Table of contents

1. What we carry forward, and what we delete — *the delete-list + do-not-regress contract*
2. Target data model (greenfield)
3. Remote start + steer (the outbound relay)
4. Desktop IDE workstream (Linux + macOS)
5. Coordination clients workstream (web + iOS + Android)
6. Notifications, email & one-way helpdesk
7. GitHub, repositories & coding-first flow
8. Billing moat, self-hosted parity & the cut list
9. Sequenced execution plan for Fable

---

## 1. What we carry forward, and what we delete

This refactor swaps the entire agent runtime for a **host-side launcher + interactive-terminal** model, on a **greenfield schema** (no prod data, no migrations, no backfill — dev resets via `bun run backend:clear` + re-migrate). Section 1 is two lists. Part A is the demolition manifest: everything that leaves. Part B is the do-not-regress contract: the invariants that survive because they were never about the agent core — sync topology, the storage-free GitHub App, the libghostty embedding, platform divergence, and the small gotchas that each already bit once.

### 1.A What we DELETE

Treat all of the following as removed in this refactor. Do not "keep for one release," do not leave an alias, do not preserve the column.

- **`crates/agent-core/` in its entirety** (~4,860 LOC Rust) — the cdylib, its dispatcher, `run_pipeline`, `agent_run`/`state`, the Rust Electric client, the Rust MCP client, and `pr_poll`.
- **Both host FFI bridges.** Linux: `apps/linux/src/core/agent/*` (`agent_core_ffi.zig`, `agent_manager.zig`, `heartbeat.zig`, `identity_store.zig`, `registration.zig`). macOS: `apps/ios/ExponentialMac/MacAgentCore.swift`, `MacAgentRunner`/`MacAgentTerminalRunner`, `MacAgentRunMonitor.swift`, `MacAgentService.swift`, `MacAgentPanel.swift`, and the agent-wiring portions of `MacGhosttyApp.swift`. (The terminal-embedding parts of `MacGhosttyApp.swift` / `MacGhosttyTerminal.swift` / `MacTerminalDock.swift` stay — see 1.B.)
- **The frozen C ABI and its handshake.** `agent_core.h`, every `agent_core_*` symbol, the `run_request` → `submit_run_result` protocol (incl. the 5-arg form and `session_id` resume plumbing), `request_interactive` / `approve_interactive`, and the borrowed-string / owned-out-param memory contract. All gone — there is no local core process to have an ABI with.
- **`InteractiveSlot` and any slot pool.** No process-global interactive slot, no per-`runId` slot pool, no `maxConcurrent`, no claim-before-slow-I/O. Multi-window concurrency is now trivially native: each window/tab is its own ghostty surface + its own `claude` child in its own worktree, with no shared state to arbitrate.
- **Headless / plan-only runs.** No in-core execution, no background dispatch. The flow is interactive-in-the-terminal only.
- **The assignment-trigger + synthetic desktop-agent identity, whole.** `users.is_agent` as a desktop-agent concept, the `agent_registrations` table, `role=agent` membership (the device's `workspace_members` rows), device registration/heartbeat, the `companion.*` / `agent.*` tRPC router (`register`/`heartbeat`/`pollControl`/`repoToken`/`setupStatus`), the long-lived `expk_` agent API keys, and the web-only `assigned-issues` Electric shape + its proxy (`apps/web/src/routes/api/shapes/assigned-issues.ts`). The person coding is the **real user** under their own session. (Exception: the widget helpdesk keeps a minimal per-widget system/bot user as issue creator for externally-filed reports — clearly separate from, and unrelated to, this deleted desktop-agent identity.)
- **Structured plan/approval state and its UI.** The `agent_runs` table (drop it entirely — it is not the 14th shape anymore), `issues.agentPlanState`, the native Plan Panels, and `apps/web/src/lib/trpc/agent-plan.ts`'s approval state machine. Plan-and-wait is now a *prompt instruction* to Claude in the terminal, not a synced state machine you branch UI on.
- **The desktop `pr_poll`.** It lived in the Rust core; it is deleted with it. Merge detection stays server-side (webhook + self-host cron — see 1.B).
- **Google Calendar, entirely.** Drop all `issues.googleCalendar*` columns, `src/lib/integrations/google-calendar.ts`, `fireAndForgetSync`/`fireAndForgetDelete`, and the `/account/integrations` calendar-connect UI. No calendar invariant carries forward.
- **The cut-list product features** (already agreed): sub-issues, issue relations beyond simple link + duplicate, and bulk-edit.

### 1.B What CARRIES FORWARD (do not regress)

These invariants are load-bearing and orthogonal to the deleted agent core. Each survives; touching one without cause re-opens debugging that is already closed.

#### 1.B.1 Electric shape lockstep across five clients (14 synced shapes)

The target is **14 synced Electric shapes** — `workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, coding_sessions`. (`coding_sessions` replaces `agent_runs` as the 14th; there is **no** `assigned-issues` proxy — the count is 14, not 15.) `repositories`, `project_repositories`, `user_notification_prefs`, and the widget/email tables are **server-only tRPC, never synced**. Remote-steer state is **relay memory, never a table**.

Every synced shape or column must be mirrored **in the same change** across all of:

- `CLAUDE.md` — the shape count + list.
- `apps/web/src/lib/collections.ts` — collection def (**`columnMapper: snakeCamelMapper()` is mandatory**; without it `useLiveQuery` `where` filters on camelCase silently fail) **+ a proxy** under `apps/web/src/routes/api/shapes/` built with `createShapeRouteHandler` (`apps/web/src/lib/shape-route.ts`).
- Zig `apps/linux/src/core/electric/sync_manager.zig` `specs[]` (+ its `expectEqual` test).
- iOS/Android entity + DAO lists.
- `packages/electric-protocol/fixtures` + `packages/domain-contract`; when enum values change, run `bun run --filter @exp/domain-contract generate` to refresh Swift/Kotlin constants.

Client parity is across all five clients (web, iOS, Android, macOS, Linux). `isPublic` / `publicWritePolicy` field gating stays via the `WorkspacePermissions` mirror of `apps/web/src/hooks/use-workspace-permissions.ts`.

#### 1.B.2 GitHub App — storage-free, server-side, outbound-only

GitHub is a **storage-free GitHub App** and stays 100% server-side. Local dependencies are only `claude` + `git` — **never `gh`, never personal creds.** App JWT (RS256, `iss = app id`) → per-repo **installation token JIT** (`installationToken` / `resolveRepoInstallationToken` in `apps/web/src/lib/integrations/github-app.ts`); bot `[bot]` identity; install via `/account/integrations`. Four server-side responsibilities persist:

1. **Repo registry** — list/validate repos for the workspace `repositories` registry.
2. **Diff serving** — `issues.prFiles` via `fetchPullFiles`, feeding the read-only side-by-side diff on all platforms.
3. **JIT installation-token mint** — a **session-gated** tRPC proc (`repositories.installationToken({repositoryId})`, authorized by the human's session — **not** an `expk_` agent key). The desktop embeds the token in the worktree remote URL (`https://x-access-token:<token>@github.com/owner/repo.git`) so `git push` works with no `gh` and no personal creds.
4. **Merge detection** — the idempotent `applyPrMergeState` (`apps/web/src/lib/integrations/pr-sync.ts`), now fed by **two** triggers (the desktop `pr_poll` is deleted): the cloud webhook `POST /api/webhooks/github` (HMAC-verified via `createHmac`/`timingSafeEqual`, `GITHUB_WEBHOOK_SECRET`), and the self-host cron gated on `GITHUB_POLLING=true` (`apps/web/src/lib/bootstrap-self-hosted.ts`, decoupled from `SELF_HOSTED`).

PR↔issue linking is **deterministic** via the `exp/<IDENTIFIER>` branch name parsed on the webhook; the MCP `open_pr` tool also records the link. PR fields (`prUrl`, `prNumber`, `prState`, `branch`) live on `issues`. All GitHub token traffic is **outbound**; only the webhook is inbound + optional (self-host without a reachable webhook falls back to the cron).

The **web MCP server** (`apps/web/src/routes/api/mcp.ts`, Streamable-HTTP) is the coding session's toolset: `get_issue`, `get_comments`, `update_status` (`in_progress`/`in_review`), `open_pr` (the **server** opens the PR via the App and links it), and optionally `add_comment`. The worktree's `.mcp.json` points Claude here, authenticated with the **user's personal** Better Auth apikey (minted once, stored in the desktop keychain/config) — not an agent credential.

#### 1.B.3 libghostty embedding — the gotchas that survive

The terminal embedding is **separate native code and is kept** (it is not part of the deleted Rust core). Linux: `apps/linux/src/ui/ghostty_ffi.zig` (GtkGLArea + GL shim from the `douglas/ghostty` fork). macOS: the prebuilt `GhosttyKit.xcframework` at `apps/ios/vendor/` (fetched by `apps/ios/scripts/setup-ghostty-macos.sh`, wired via `apps/ios/Project.swift`). Its gotchas still bite:

- **(ghostty-a) Surface inits lazily only at NONZERO size** — mount the terminal only when its container is visible at a real height. Each detached window/tab must honor this independently.
- **(ghostty-b) `GHOSTTY_ACTION_RENDER` must be handled** in the action callback (queue a redraw) or the terminal never paints.
- **(ghostty-c) libghostty is NEVER built from source on macOS** — link the prebuilt `GhosttyKit.xcframework`; Linux uses the GL shim from the `douglas/ghostty` fork.

`claude` is always spawned `--dangerously-skip-permissions` (never make the user click accept), cwd = the worktree, seeded with the prefilled prompt.

#### 1.B.4 Platform-divergence and web-only rules (locked)

- **macOS keeps the glass aesthetic** (SwiftUI `.ultraThinMaterial`), aligning only semantic status/priority tokens. Do not flatten it to match Linux.
- **Linux must reach web 1:1 (pixel parity)** — button sizes, spacing, fonts, row virtualization; the biggest open UI-quality gap. The read-only syntax-highlighted side-by-side diff lands here, replacing Linux's plaintext diff.
- **Admin console is WEB-ONLY.**
- **Billing (Creem) is WEB-ONLY** — native clients show no billing UI (store-policy safe).
- **Self-hosted must fully support every feature** — the relay is optional / LAN-outbound-friendly; the email channel degrades gracefully (SMTP / Resend / none).

#### 1.B.5 The gotcha list (each already bit once)

- **(a) Native generic sync DROPS A WHOLE ROW** if the server adds a column the local table lacks. This forward-tolerance is the *one* piece of migration-defensiveness that survives the greenfield reset: guard with `apps/linux/src/core/db/database.zig`'s `tableColumnSet` + a self-heal `ALTER` on **every** native client for **every** new column. Applies directly to the `coding_sessions` and new `issues` columns this refactor adds.
- **(b) tRPC error code is `PRECONDITION_FAILED`** (not `FAILED_PRECONDITION`).
- **(c) Custom SQL triggers are NOT auto-applied** — `apps/web/src/db/out/custom/0001_triggers.sql` and `0002_public_workspace.sql` must be run manually after a fresh DB. Any new trigger inherits the same manual-apply obligation and must be documented in the release checklist.
- **(d) `snakeCamelMapper()` is mandatory** on every collection in `collections.ts` (restated because it is the single most common silent-failure).
- **(e) Use `and()`/`or()` from `@tanstack/react-db`** in `useLiveQuery` (never JS `&&`/`||`); return `undefined` (not `false`) to skip a query.

### Definition of done

- [ ] `crates/agent-core/` and both FFI bridges (`apps/linux/src/core/agent/*`, `apps/ios/ExponentialMac/MacAgent*.swift` + the agent wiring in `MacGhosttyApp.swift`) are deleted; no `agent_core_*` symbol, `agent_core.h`, or `run_request`/`submit_run_result` reference remains anywhere in the tree.
- [ ] No `InteractiveSlot` / slot pool / `maxConcurrent` / headless-run path survives; multi-window concurrency is native (one ghostty + one `claude` + one worktree per window), with nothing shared to arbitrate.
- [ ] The synthetic desktop-agent identity is fully removed: no `users.is_agent` (desktop sense), `agent_registrations`, `role=agent`, `companion.*`/`agent.*` router, `expk_` agent keys, or `assigned-issues` shape/proxy. The widget bot user is the only remaining system user and is clearly scoped to the helpdesk.
- [ ] `agent_runs` and `issues.agentPlanState` are dropped and the native Plan Panels + `agent-plan.ts` approval machine are gone; plan-and-wait is a prompt instruction, and `coding_sessions` is the slim synced state (id, issueId, workspaceId, userId, deviceLabel, status, startedAt, endedAt).
- [ ] Google Calendar (columns, `google-calendar.ts`, fire-and-forget sync, connect UI) is fully removed; no calendar invariant carried forward.
- [ ] The schema is greenfield — no migration/backfill/keep-column code; the only migration-defensiveness kept is the native `tableColumnSet` forward-tolerance (gotcha a).
- [ ] Synced-shape count is **14** (`coding_sessions` as the 14th, no `assigned-issues`); every synced shape/column is mirrored in lockstep across the sites in 1.B.1 with `snakeCamelMapper` applied; `repositories`/`project_repositories`/prefs/widget tables stay server-only; steer state stays relay-memory.
- [ ] GitHub stays storage-free, server-side, outbound-only: JIT installation-token mint is **session-gated** (not agent-keyed), diff serving via `issues.prFiles`, and `applyPrMergeState` is fed by the **two** surviving triggers (webhook + self-host cron); PR↔issue linking is deterministic via `exp/<IDENTIFIER>`.
- [ ] The web MCP server exposes `get_issue`/`get_comments`/`update_status`/`open_pr` (+ optional `add_comment`); the worktree `.mcp.json` authenticates with the user's personal apikey; local deps are only `claude` + `git` (never `gh`).
- [ ] libghostty embedding is preserved with its three gotchas respected; macOS links the prebuilt `GhosttyKit.xcframework` (never builds from source); `claude` is always spawned `--dangerously-skip-permissions`.
- [ ] macOS-glass / Linux-1:1 divergence, admin-web-only, billing-web-only, and self-hosted parity (optional relay + degradable email) all hold; the five remaining gotchas (a–e) are respected in new code paths.
- [ ] macOS/iOS remain a verify-and-polish track on real hardware — minus everything agent-core/FFI (deleted, not verified).

---

## 2. Target data model (greenfield)

This is the **clean target schema**, defined directly — not a migration off the current tables. There is no production data to preserve: dev resets with `bun run backend:clear` then `bun run migrate`. Every table below is the initial shape; there are no `ALTER TYPE` steps, no backfills, no "keep the column for one release." The only forward-evolution defensiveness we keep is that native clients **tolerate unknown/extra server columns** (the generic sync + `tableColumnSet` self-heal), so later additive columns don't drop rows — but the schema we ship first is fresh.

All schema lives in `packages/db-schema/src/schema.ts`; every enum value array lives in `packages/db-schema/src/domain.ts`, mirrored into `packages/domain-contract/contract.json` (regenerate Swift/Kotlin constants with `bun run --filter @exp/domain-contract generate`). Helpers: `uuidPk()` (UUID PK via `gen_random_uuid()`) and the shared `timestamps` object (`createdAt`/`updatedAt`, both `withTimezone`), both already in `schema.ts`.

### 2.1 The two tiers

**SYNCED — 14 Electric shapes** (one proxy + one client collection + native DAO each):

| # | Shape | Notes |
| --- | --- | --- |
| 1 | `workspaces` | |
| 2 | `projects` | drops `githubRepo` (moved to the `repositories` registry, §2.3); keeps `previewConfig` display mirror |
| 3 | `issues` | core fields + PR fields + `duplicateOfId` self-FK (§2.4) |
| 4 | `labels` | |
| 5 | `issue_labels` | |
| 6 | `users` | no `isAgent` desktop-agent concept; the widget bot user is an ordinary row (§2.7) |
| 7 | `workspace_members` | roles `owner`/`member` only — **no `agent` role** |
| 8 | `workspace_invites` | |
| 9 | `comments` | |
| 10 | `attachments` | |
| 11 | `notifications` | |
| 12 | `issue_events` | |
| 13 | `issue_subscribers` | gains nullable `userId` + nullable `email` for widget-reporter rows (§2.7) |
| 14 | **`coding_sessions`** | **the one new/renamed shape — replaces the deleted `agent_runs`** (§2.5) |

`agent_runs`, `agent_registrations`, and the `assigned-issues` proxy are **deleted** (there is no headless agent, no synthetic desktop-agent user, no device registration). Net: **14 synced shapes, 14 proxies** (the extra `assigned-issues` proxy is gone, so proxy count == shape count).

**SERVER-ONLY — tRPC / relay, never Electric-synced** (no proxy, no client collection, no native DAO):

- `repositories` — workspace repo registry (§2.3)
- `project_repositories` — many-to-many project↔repo join (§2.3)
- `user_notification_prefs` — per-user email/channel prefs + digest + unsubscribe token (§2.6)
- `email_deliveries` — email audit / idempotency ledger (§2.6)
- `widget_configs` / `widget_submissions` — the one-way feedback helpdesk (unchanged intent; §2.7)

Remote steer/viewer presence is **relay memory, not a table** — there is deliberately no `remote_steer_sessions`. The relay is ephemeral; who is watching/steering a live terminal is a relay frame, claimed and released in process.

### 2.2 The lockstep checklist (SYNCED shapes only)

This is the standing contract for the **14 synced shapes**. Skipping a step silently corrupts a client (see the "row-drop on unknown column" gotcha). Server-only tables (§2.3, §2.6) are tRPC-only and touch **none** of steps 3–8.

1. **Schema** — add the table in `packages/db-schema/src/schema.ts`; add the `select…Schema` (+ `create…Schema` where mutated) and the `InferSelectModel` type export at the bottom of the file.
2. **Enum contract** — if it adds/extends an enum, edit the `…Values` array in `domain.ts`, mirror into `contract.json`, and run `bun run --filter @exp/domain-contract generate`.
3. **Web collection** — add a `createCollection(electricCollectionOptions({…}))` block in `apps/web/src/lib/collections.ts` (`columnMapper: snakeCamelMapper()`, `parser: shapeParser`, `getKey`).
4. **Web shape proxy** — add `apps/web/src/routes/api/shapes/<name>.ts` via `createShapeRouteHandler` (`apps/web/src/lib/shape-route.ts`), workspace-scoped `where`.
5. **Zig sync** — add a `ShapeSpec` to the `specs` array in `apps/linux/src/core/electric/sync_manager.zig`, bump the count assertion in the `"shape registry: 14 shapes with matching tables"` test, and add the SQLite `CREATE TABLE` in `apps/linux/src/core/db/migrations.zig`.
6. **Zig self-heal** — extend `tableColumnSet` in `apps/linux/src/core/db/database.zig` and add the idempotent `ALTER TABLE … ADD COLUMN` in `migrations.zig` so an older local DB self-heals rather than dropping rows.
7. **iOS/macOS** — add the entity in `apps/ios/ExpCore/Sources/DB/Entities.swift`, register it in `DatabaseManager.swift`, add the shape to `apps/ios/ExpCore/Sources/Electric/SyncManager.swift`.
8. **Android** — add the entity in `.../data/db/Entities.kt`, the DAO in `Daos.kt`, register it in `ExponentialDatabase.kt` (bump the Room version), add the shape to `.../data/electric/SyncManager.kt`.
9. **CLAUDE.md + memory** — the synced-shape count is **14** and the client-parity list names `coding_sessions` (not `agent_runs`).

> The Zig test name literally reads "14 shapes" — with `coding_sessions` swapped in for `agent_runs` the count stays 14, so the assertion constant does not move; only the table name in the spec changes.

### 2.3 Repositories registry (server-only)

Repositories are a **first-class workspace entity**, tRPC-managed, **not** a synced shape (the desktop launcher and web settings read them over tRPC; native coordination clients don't need every repo row streamed). GitHub stays 100% server-side via the storage-free GitHub App.

**`repositories`** — workspace-scoped, one row per linked GitHub repo:

```ts
export const repositories = pgTable(
  `repositories`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    owner: varchar({ length: 255 }).notNull(),          // GitHub org/user
    name: varchar({ length: 255 }).notNull(),           // repo name
    defaultBranch: varchar(`default_branch`, { length: 255 }).notNull().default(`main`),
    // Cached GitHub App installation id for JIT installation-token minting;
    // nullable — the App JWT can still resolve it on demand (github-app.ts is
    // storage-free).
    installationId: bigint(`installation_id`, { mode: `number` }),
    branchPrefix: varchar(`branch_prefix`, { length: 64 }).notNull().default(`exp/`),
    mergeStrategy: varchar(`merge_strategy`, { length: 16 }).notNull().default(`squash`), // squash|merge|rebase
    ...timestamps,
  },
  (table) => [
    unique().on(table.workspaceId, table.owner, table.name),
    index(`idx_repositories_workspace`).on(table.workspaceId),
  ]
)
```

`mergeStrategy` is a documented `varchar` value set (`squash`/`merge`/`rebase`), not a pg enum — it is web-settings display metadata with no native picker and no cross-client constants.

**`project_repositories`** — many-to-many join (a repo may back several projects; a project may span several repos):

```ts
export const projectRepositories = pgTable(
  `project_repositories`,
  {
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
    repositoryId: uuid(`repository_id`)
      .notNull()
      .references(() => repositories.id, { onDelete: `cascade` }),
    // Denormalized project→workspace for a cheap workspace-scoped tRPC filter.
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    // The repo the launcher clones for an issue in this project by default.
    // Exactly one primary per project (partial unique index below).
    isPrimary: boolean(`is_primary`).notNull().default(false),
    ...timestamps,
  },
  (table) => [
    primaryKey({ columns: [table.projectId, table.repositoryId] }),
    index(`idx_project_repositories_repo`).on(table.repositoryId),
    index(`idx_project_repositories_workspace`).on(table.workspaceId),
  ]
)
```

Because these are server-only, `workspaceId` here is a plain convenience denorm set by the writing tRPC procedure — it needs **no** Electric-scoping trigger.

Add the partial unique index in the generated migration (Drizzle can't express it inline):
`CREATE UNIQUE INDEX uniq_project_primary_repo ON project_repositories (project_id) WHERE is_primary;`

**Resolution:** `repositories.forIssue({issueId})` walks issue → project → its `is_primary` `project_repositories` row → `repositories`, returning `{ owner, name, defaultBranch, repositoryId }`. `repositories.installationToken({repositoryId})` is a session-gated proc minting a JIT installation token for the host git remote. If a project has no linked repo, `forIssue` returns "not linked" and the desktop surfaces "Link a repo in workspace settings" — there is no fallback (no `projects.githubRepo` exists anymore).

### 2.4 `issues` additions

`issues` already carries the PR linkage columns in the current tree; they stay in the target schema as the canonical, synced PR state (one issue = one PR = one branch/worktree):

```ts
prUrl:    text(`pr_url`),
prNumber: integer(`pr_number`),
prState:  prStateEnum(`pr_state`),   // open|closed|merged|draft
branch:   text(`branch`),            // exp/<IDENTIFIER>
prMergedAt: timestamp(`pr_merged_at`, { withTimezone: true }),
```

PR↔issue linking is **deterministic** via the `exp/<IDENTIFIER>` branch parsed on the GitHub webhook (and recorded by the MCP `open_pr` tool). **Dropped from `issues`** vs. the old tree: `googleCalendarEventId`, `googleCalendarLastSyncedAt`, `googleCalendarLastSyncError` (Calendar is cut — Google OAuth *login* is unaffected), and `agentPlanState` (no structured plan-approval state; you watch the terminal).

**New — duplicate resolution.** Add a self-FK plus a new status value:

```ts
duplicateOfId: uuid(`duplicate_of_id`).references(
  (): AnyPgColumn => issues.id, { onDelete: `set null` }
),
```

and add `duplicate` to `issueStatusValues` in `domain.ts`. "Duplicate" is a **resolution**, not a relation graph: 1:1, terminal-ish (hidden from active lists like `done`/`cancelled`), and it drops straight into the existing `matchesFilters()` / status-group machinery. Both fields ride the already-synced `issues` shape — no new proxy. (Issue-to-issue *references* stay inline in GFM markdown as `#MET-1153` identifier tokens, resolved server-side like `@email` mentions — zero schema surface.)

### 2.5 `coding_sessions` — the live "coding now" shape (SYNCED)

The single synced record of an in-flight terminal coding session, so every coordination client shows a live "coding now" badge and a Watch/Steer button. This is the entire replacement for the deleted `agent_runs` — no structured plan/approval state, no run history, no slot pool.

```ts
export const codingSessions = pgTable(
  `coding_sessions`,
  {
    id: uuidPk(),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    // Denormalized issue→project→workspace so the Electric shape filter stays
    // workspace-scoped (populated by a trigger, mirroring issue_subscribers).
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    // The real user driving the session under their own auth — NOT a synthetic
    // agent identity.
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // Human label of the host device ("Dennis's MacBook"), shown on the badge.
    deviceLabel: varchar(`device_label`, { length: 255 }),
    status: codingSessionStatusEnum(`status`).notNull().default(`running`), // running|ended
    startedAt: timestamp(`started_at`, { withTimezone: true }).notNull().defaultNow(),
    endedAt: timestamp(`ended_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_coding_sessions_issue`).on(table.issueId),
    index(`idx_coding_sessions_workspace`).on(table.workspaceId),
    index(`idx_coding_sessions_user`).on(table.userId),
  ]
)
```

**Status** is a small pg enum `coding_session_status` (`running`/`ended`) — only two states, native badge logic keys off it, so it earns a real enum + generated constants. Multi-window is trivially native: each window = its own ghostty + its own `claude` child in its own worktree, so several `running` rows can coexist for one issue/user (no shared slot pool, no `runId` bookkeeping). PR outcome is **not** duplicated here — it lives on `issues` (`prUrl`/`prNumber`/`prState`/`branch`). Add a `populate_coding_session_workspace_id` trigger to `0001_triggers.sql`, mirroring `populate_issue_subscriber_workspace_id`. Full 2.2 lockstep; proxy `/api/shapes/coding-sessions`.

### 2.6 Notifications + email (server-only)

Email is a **delivery channel**, not a notification type — no `notification_type` value is added for it. In-app + push + email are the three free channels; the fan-out is plain user-to-user (issue created / assigned / commented / mentioned / pr-opened / pr-merged). No agent-action special-casing (there's no headless agent posting plan-ready/questions). Cloud uses Resend (`RESEND_API_KEY`/`EMAIL_FROM`); self-hosted uses SMTP or degrades to a logged no-op with no creds.

**`user_notification_prefs`** — per-user channel prefs + per-type toggles + digest + unsubscribe token:

```ts
export const userNotificationPrefs = pgTable(`user_notification_prefs`, {
  userId: text(`user_id`).primaryKey()
    .references(() => users.id, { onDelete: `cascade` }),
  emailEnabled: boolean(`email_enabled`).notNull().default(true),
  // Per-type opt-outs; a type absent from the map defaults to on. Keys are
  // notification_type values (issue_assigned, issue_comment, …).
  typePrefs: jsonb(`type_prefs`).$type<Partial<Record<NotificationType, boolean>>>()
    .notNull().default(sql`'{}'::jsonb`),
  digest: varchar({ length: 16 }).notNull().default(`off`), // off|daily|weekly
  // Stable per-user secret embedded in one-click List-Unsubscribe links.
  unsubscribeToken: varchar(`unsubscribe_token`, { length: 64 }).notNull().unique(),
  ...timestamps,
})
```

**`email_deliveries`** — audit + idempotency + a home for external-reporter mail (no `users` row required):

```ts
export const emailDeliveries = pgTable(
  `email_deliveries`,
  {
    id: uuidPk(),
    // Nullable: external widget reporters have no users row.
    userId: text(`user_id`).references(() => users.id, { onDelete: `cascade` }),
    toEmail: varchar(`to_email`, { length: 320 }).notNull(),
    // Idempotency key: one delivery per notification row.
    notificationId: uuid(`notification_id`).references(() => notifications.id, { onDelete: `set null` }),
    issueId: uuid(`issue_id`).references(() => issues.id, { onDelete: `set null` }),
    kind: varchar({ length: 32 }).notNull(),                 // notification|digest|widget_resolution
    status: varchar({ length: 16 }).notNull().default(`queued`), // queued|sent|failed
    provider: varchar({ length: 16 }),                        // resend|smtp
    providerMessageId: text(`provider_message_id`),
    error: text(),
    sentAt: timestamp(`sent_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_email_deliveries_user`).on(table.userId),
    index(`idx_email_deliveries_issue`).on(table.issueId),
    unique(`uniq_email_delivery_notification`).on(table.notificationId), // idempotent per notification
  ]
)
```

Neither is synced (delivery/audit concerns, not client state). The fan-out hooks the existing notification pipeline (`apps/web/src/lib/integrations/notifications.ts` + `fcm.ts`): after a notification row is created, if the recipient's `user_notification_prefs.emailEnabled` and `digest='off'` and the type isn't opted out, enqueue an `email_deliveries` row and send. Digest modes batch. The unsubscribe route resolves `unsubscribeToken` → flips `emailEnabled=false`. `digest`, `kind`, and `status` are documented varchars (server-only logic, no native constants).

### 2.7 Widget helpdesk (server-only, unchanged intent)

`widget_configs` / `widget_submissions` stay exactly as the current one-way feedback path. The **one** identity exception the v2 cuts preserve: each config owns a minimal per-widget **bot/system user** as the issue `creator_id` (so external reporters can file issues) — this is clearly separate from and unrelated to the deleted desktop-agent identity. **Never delete that user** (`issues.creator_id` cascades); `widget_configs.widgetUserId` uses `onDelete: restrict`.

**Reporter as subscriber.** `issue_subscribers` gains two nullable columns and a new source so an external reporter can be modeled directly without a fake `users` row:

```ts
// on issue_subscribers — userId becomes NULLABLE (was notNull):
userId: text(`user_id`).references(() => users.id, { onDelete: `cascade` }), // null for widget reporters
email: varchar({ length: 320 }),  // set for widget_reporter rows; null for member rows
```

- Member subscriptions: `userId` set, `email` null, `source ∈ {creator, assignee, commenter, manual, mention}`.
- Widget reporter: `userId` null, `email = reporter's address`, `source = 'widget_reporter'`.
- The `unique(issueId, userId)` constraint must move to a form that tolerates null userId — use two partial unique indexes: `(issue_id, user_id) WHERE user_id IS NOT NULL` and `(issue_id, email) WHERE email IS NOT NULL` (raw SQL in the migration).

When a widget submission creates an issue, insert a `widget_reporter` subscriber (email from `widget_submissions.reporterEmail`). On resolution — the issue transitions to `done`/`cancelled` — enqueue a one-way resolution email (`email_deliveries.kind = 'widget_resolution'`, `userId` null, `toEmail` = the reporter). `issue_subscribers` is a synced shape, so the two new columns run the full 2.2 lockstep on native clients (self-heal ALTER for `email`, nullable-`userId` decode).

### 2.8 Enum changes (consolidated)

| Enum (`domain.ts` array → `contract.json`) | Storage | Change |
| --- | --- | --- |
| `subscriberSourceValues` | pg enum `subscriber_source` | **+ `widget_reporter`** |
| `issueStatusValues` | pg enum `issue_status` | **+ `duplicate`** (add to `issueStatusOrder` / `displayOrder`, place after `cancelled`) |
| `codingSessionStatusValues` (**new**) | pg enum `coding_session_status` | new: `running`, `ended` |
| `workspaceRoleValues` | pg enum | **drop `agent`** — only `owner`, `member` remain (no agent-role membership) |
| `notificationTypeValues` | pg enum | **drop `agent_plan_review`, `agent_question`** (no headless agent notifications) |
| `agentPlanStateValues` | — | **deleted** (no structured plan state on issues) |
| `runModeValues` | — | **deleted** (`run_mode` had one consumer, `agent_runs`, now gone) |
| `issueEventTypeValues` | pg enum | keep timeline kinds; **drop the agent-only ones** (`plan_ready`, `agent_error`, `agent_started`, `agent_question`, `agent_answer`); keep `status_changed`, `assignee_changed`, `label_added`, `label_removed`, `pr_opened`, `pr_merged` |
| `platformValues` | varchar (contract-only) | **unchanged** (`web`/`android`/`ios` preview run-target backends; desktop host OS is free `deviceLabel` text, not a domain enum) |

Since the schema is greenfield, all of the above are just the initial enum definitions — no `ALTER TYPE`, no split-migration ceremony. `merge_strategy`, `email_deliveries.kind`/`status`, and `user_notification_prefs.digest` stay documented varchars (server-only, no native pickers).

### Definition of done

- [ ] Greenfield schema in `schema.ts` migrates cleanly from empty (`bun run backend:clear && bun run migrate`); no `ALTER TYPE`/backfill blocks in the generated migration.
- [ ] Exactly **14 synced shapes**: `agent_runs` replaced by `coding_sessions`; `agent_registrations` and the `assigned-issues` proxy are absent; proxy count == 14.
- [ ] `coding_sessions` runs the full 2.2 lockstep (collections.ts, `/api/shapes/coding-sessions`, Zig spec + `"14 shapes"` test still green + SQLite migration + self-heal, iOS/Android entity+DAO); `populate_coding_session_workspace_id` trigger in `0001_triggers.sql`; `coding_session_status` enum + generated constants.
- [ ] `repositories` + `project_repositories` exist as **server-only** tables (no proxy/collection/native DAO); partial unique index on the primary repo; `repositories.forIssue` + `repositories.installationToken` tRPC procs resolve clone target + JIT token; **no `projects.githubRepo` column exists**.
- [ ] `issues` carries `prUrl`/`prNumber`/`prState`/`branch`/`prMergedAt` + `duplicateOfId` self-FK; `issue_status` includes `duplicate`; **no `googleCalendar*` columns**, **no `agentPlanState`**.
- [ ] `user_notification_prefs` + `email_deliveries` exist as server-only; per-user email prefs + per-type toggles + digest + unsubscribe token; idempotent per-notification delivery; graceful no-op without Resend/SMTP.
- [ ] `issue_subscribers.userId` is nullable and `email` added; two partial unique indexes replace `unique(issueId, userId)`; `subscriber_source` includes `widget_reporter`; widget resolution email enqueued on `done`/`cancelled`.
- [ ] Enum set matches §2.8: `workspace_member_role` has no `agent`; `notification_type` and `issue_event_type` drop the agent-only kinds; `run_mode` and `agentPlanState` are gone.
- [ ] Widget bot/system user retained per config with `onDelete: restrict`; no other synthetic-user or `isAgent` desktop-agent identity anywhere.
- [ ] `contract.json` regenerated (`bun run --filter @exp/domain-contract generate`); `bun run typecheck` + `bun run test` green; `0001_triggers.sql` re-run after migrate; CLAUDE.md updated to **14 synced shapes** naming `coding_sessions`.

---

## 3. Remote start + steer (the outbound relay)

The killer flow: an issue lands while I'm away, my desktop is running at home, and from my phone I press **"Start on my desktop"** — the desktop spins up the native Start-coding launcher (clone → worktree → `claude --dangerously-skip-permissions` in an embedded ghostty terminal, see §4/§5) and I **watch and type into the live terminal from my phone**. Electric syncs rows and cannot carry a live PTY byte stream, so remote start + steering needs a **standalone relay service** that the desktop connects to **outbound** and that web/mobile viewers subscribe to.

There is **no Rust core, no agent-core, no runId, no dispatcher** anywhere in this path. The person coding is the real user under their own session. The desktop is the only place a `claude` child ever runs; web and mobile are pure remote viewers/steerers of a desktop-hosted PTY.

### 3.0 Why a separate service, and why outbound-from-desktop

- **Electric can't carry the PTY.** Electric is a Postgres shape-log replicator: ordered row snapshots + change deltas. A terminal is a high-frequency, ephemeral byte stream (thousands of tiny frames/sec at peak, worthless once consumed) with a **reverse input channel**. Persisting frames as rows would trash the WAL, and Electric has no viewer→writer RPC path. Terminal transport must be a separate, non-persisted, bidirectional channel. The synced `coding_sessions` row (the 14th Electric shape) is the **control-plane** record — "device D is coding on issue X right now," `running → ended`; the relay is the **data-plane** for bytes only.
- **Outbound-from-desktop is mandatory for self-hosted / NAT.** The desktop runs on a laptop at home behind NAT with no inbound reachability — the same constraint that already forced the push-relay (`apps/push-relay`) and the GitHub App (outbound App-JWT → installation token; only the webhook is inbound + optional) to be outbound-only. The desktop therefore **dials out** and holds a persistent control socket (device presence) plus, per session, a publisher socket. Viewers (web/mobile) also dial the relay (a reachable host). Nothing ever connects *into* the desktop.

### 3.1 New service: `apps/steer-relay`

Model it **exactly** on `apps/push-relay` — standalone Hono/Bun, separately deployed, its own Coolify app and `Dockerfile.steer-relay` (context `.`). Reuse its skeleton verbatim (`apps/push-relay/src/index.ts`): lazy singleton, `/healthz` unauth check for the Docker HEALTHCHECK, per-IP token-bucket rate limiting with the periodic sweep, `MAX_BODY_BYTES` guard, `clientIp()` X-Forwarded-For hardening, `PORT` from env. Add it as bun workspace `@exp/steer-relay`.

The one structural difference from push-relay: push-relay is a stateless request/response FCM forwarder; steer-relay is a **stateful WebSocket hub**. Use Bun's native `Bun.serve` `websocket` handler (export `{ port, fetch, websocket }`) rather than Hono's fetch-only path. The relay holds two in-memory registries:

1. **Device presence** — a map of `(userId → { deviceLabel, controlSocket, connectedAt })` for every desktop that currently holds an outbound control socket. This is what powers the phone's "Start on my desktop" device picker.
2. **Session rooms** — keyed by **`sessionId` (== `coding_sessions.id`, a desktop-generated UUID)**, NOT any runId. Each room has:
   - exactly one **publisher** socket (the desktop hosting that ghostty PTY),
   - zero-or-more **viewer** sockets (web/mobile),
   - a small **ring buffer** of the most recent N KB of terminal output (scrollback replay so a mid-session viewer sees current screen state, not a blank pane).

The relay is a **dumb pipe with auth + ephemeral presence**. It does not parse terminal escape codes, does not persist bytes, and holds no DB connection. **All steer state — device presence, viewer presence, and the single-steerer claim — lives in relay memory.** There is **no `remote_steer_sessions` table** (removed); the only durable session record is the synced `coding_sessions` row, written by the desktop over tRPC when it starts/ends a session.

Env vars (mirror push-relay naming):
```
STEER_RELAY_URL        # public relay base (wss://steer.exponential.at) — desktop + clients dial this
STEER_RELAY_SECRET     # shared HS256 secret: web app mints tickets, relay verifies
PORT                   # default 4002
```
`STEER_RELAY_URL` goes into the web env and into the desktop host config (JetBrains-SDK-style settings, §7) — the relay URL is a **host-app** concern.

### 3.2 Two channels: device control + session data

Every desktop, while the app is open, holds **one outbound control socket** to the relay. Each active coding session additionally opens **one publisher socket**. Viewers open **one viewer socket per session** they watch.

**Auth on connect.** Before joining anything, the client presents a short-lived **relay ticket** minted by the web app — never send raw session cookies or personal API keys to the relay. Tickets are compact HS256 tokens signed with `STEER_RELAY_SECRET`, minted by `steer.mintTicket` (a tRPC proc gated by session **or** the user's personal Better Auth apikey), which first verifies the caller's permission (§3.5). The relay verifies signature + `exp` only; all authorization was decided at mint time. Ticket claims:
```
{ userId, workspaceId, deviceLabel?, sessionId?, role: "control"|"publisher"|"viewer",
  perm: "view"|"steer", exp }
```
- **control** ticket → desktop registers device presence (no `sessionId` yet).
- **publisher** ticket → desktop attaches as the publisher for a specific `sessionId` (it always gets `role:"publisher"` for sessions it started).
- **viewer** ticket → web/mobile joins a `sessionId`; `perm` is `view` or `steer` per §3.5.

Framing: JSON for control frames; **raw binary** for terminal output (opcode byte `0x01` + payload) to avoid base64 bloat on the hot path.

**Wire protocol (JSON control frames `{t, ...}`):**

| `t` | dir | payload | meaning |
| --- | --- | --- | --- |
| `hello` | pub→relay | `{sessionId, issueId, cols, rows}` | desktop publisher registers; relay creates/attaches the room |
| `online` | desktop→relay | `{deviceLabel}` | desktop control socket announces device presence for `userId` |
| `start_session` | phone→relay→desktop | `{issueId, deviceId?}` | remote **Start on my desktop**; relay routes to the chosen online control socket |
| `join` | view→relay | `{sessionId}` | viewer subscribes; relay replays ring buffer then live-tails |
| `resize` | pub→relay→view | `{cols, rows}` | terminal geometry changed (viewers reflow) |
| `input` | view→relay→pub | `{bytes}` (utf8) | keystrokes from the **steer**-holding viewer, injected into the PTY |
| `presence` | relay→all | `{viewers:[{userId,name,perm}], steererId}` | who's watching/steering (drives UI avatars) |
| `claim` | view→relay | `{}` | request the exclusive steer token (§3.4) |
| `release` | view→relay | `{}` | give up steer |
| `kill` | view→relay→pub | `{}` | **kill-switch**: force-terminate the session (§3.5) |
| `bye` | pub→relay | `{outcome}` | session ended; relay closes room, evicts viewers |

**Terminal output** is the binary `0x01` frame pub→relay→all-viewers — verbatim ghostty PTY bytes (§3.3), so xterm.js on the web renders identically to the local terminal.

**Remote start path.** Phone sends `start_session{issueId, deviceId?}` (over a viewer/control ticket scoped to the workspace). The relay looks up the target desktop's control socket (the user's only online device, or the picked `deviceId`) and forwards `start_session`. The desktop runs the native Start-coding launcher (§4/§5): resolve repo, fetch JIT installation token, worktree + `exp/<IDENTIFIER>` branch, write `.mcp.json`, spawn `claude --dangerously-skip-permissions` in an embedded ghostty terminal, insert the `coding_sessions` row (`running`), then open its publisher socket with `hello{sessionId,…}` and start teeing PTY bytes. The phone, watching `coding_sessions` over Electric, sees the new `running` row and joins by `sessionId`.

**Backpressure.** The desktop is the fast producer; a phone on cellular is the slow consumer. The relay MUST NOT buffer unboundedly per viewer. Policy: each viewer socket has a bounded send queue; on overflow the relay **coalesces** by dropping intermediate output frames and requesting a **full-screen resync** from the publisher (ghostty dumps its current visible grid). Control frames (`input`, `claim`, `resize`, `kill`) are **never** dropped. Bun's `ws.send()` surfaces backpressure; a viewer saturated past a timeout is dropped with a `slow_consumer` close code (client shows "reconnecting"). The publisher is never throttled by a slow viewer.

**Reconnect.** Viewers reconnect with a fresh ticket and re-`join` (ring-buffer replay covers the gap). If the **publisher** socket drops, the relay marks the room `stale` and starts a grace timer; the desktop re-`hello`s on reconnect and resumes the same `sessionId`. If the desktop control socket drops, its device presence is evicted and it disappears from the phone's device picker.

### 3.3 The publisher = a native host component (Zig / Swift)

The publisher is **native host code** — there is no Rust core involved. It sits beside the terminal embedding already present on each desktop: Linux `apps/linux` (GtkGLArea wrapping a ghostty surface) and macOS `apps/ios/ExponentialMac` (GhosttyKit.xcframework). Per active session the host runs a `SteerPublisher`:

- **Tee the ghostty PTY output.** The `claude` child's PTY master is host-owned (the launcher spawns it, §5). Tee the PTY read stream → (a) the ghostty surface feed for local display **and** (b) the relay binary `0x01` frame. These are verbatim terminal bytes, so a remote xterm.js renders pixel-identically.
- **Full-screen resync/replay.** On `join`/`resync`, emit ghostty's current visible grid as an ANSI reconstruction (or compact cell snapshot) so a late viewer gets current state, then switch to live PTY bytes.
- **Inject remote input.** The relay forwards `input{bytes}` only from the socket currently holding the steer claim. The host writes those bytes into the **same PTY master write** the local keyboard writes to — i.e. into stdin, not the ghostty input API — so `claude` sees them as normal keystrokes. This is the single point where local + remote input merge on one stream.
- **Kill-switch.** On a `kill` frame, the host tears down the session's terminal (destroying the ghostty surface kills the `claude` child), flips `coding_sessions.status = 'ended'`, and closes the room with `bye`.

**Multi-window is trivially native.** Each window/tab is its own ghostty surface + its own `claude` child in its own worktree + its own `SteerPublisher` keyed by its own `sessionId`. **No shared slot pool, no `maxConcurrent` gate, no core** — concurrency falls out of running independent OS processes. One relay room per `sessionId` maps 1:1 to one terminal tab / detached window.

### 3.4 The claim model (single-steerer, in relay memory)

Multiple viewers may `view` concurrently; **at most one** holds `steer` at a time (a terminal has one input cursor — two people typing is chaos). The claim is **relay memory only**:

- First `claim` from a `steer`-perm viewer wins and becomes `steererId`; others see "X is steering" via `presence`.
- The relay forwards `input` frames **only** from the socket whose `userId == steererId`; everyone else's input is dropped.
- `release` (or that viewer disconnecting) frees the claim.
- **The local desktop user always types** into the terminal directly — local input is their machine and is never gated by the relay. When a remote steerer is active the desktop shows a small "remote steering — <name>" banner with a **Take over** button; pressing it sends a local `release`-then-`claim` on the user's own behalf (their machine wins immediately).

No `until`/expiry table, no `renewClaim`/`forceClaim` procs, no `interactiveClaimedExpiresAt` mirror — the whole claim is ephemeral. If the relay restarts, every socket reconnects and re-`claim`s; nothing durable is lost because nothing durable existed.

### 3.5 Security, permissions, kill-switch

- **Who can view / steer.** `steer.mintTicket` checks: caller is authenticated (session or personal apikey) and is a member of the session's `workspace_id`. Default rule — **workspace members with role `owner`/`admin` may `steer`; other members get `view`; non-members get nothing.** Reuse the server-side membership + permission helpers (`apps/web/src/lib/auth/membership.ts` and `apps/web/src/lib/auth/access.ts`), mirroring the client `WorkspacePermissions` semantics in `apps/web/src/hooks/use-workspace-permissions.ts`. The desktop publisher is implicitly the session owner's own machine, keyed to their `userId`.
- **No raw creds to the relay.** The relay only ever sees signed tickets; it verifies the HS256 signature with `STEER_RELAY_SECRET` and enforces a short `exp` (60s to connect; the socket then lives on its own). Compromising the relay leaks live terminal bytes of *active* sessions only — no persisted data, no DB, no ability to mint new access.
- **Kill-switch.** Any `steer`-perm member can `kill` a session from any client → relay forwards to the publisher → host tears down the terminal and flips `coding_sessions.status = 'ended'`. The desktop also watches its own `coding_sessions` row over Electric, so a server-side `steer.killSession` that sets `status='ended'` aborts the run even if the relay is unreachable.
- **Rate-limit + origin.** The relay applies the push-relay per-IP token bucket on connects and rejects `input` frames larger than a small cap.

### 3.6 Self-hosted & graceful degradation

- **Config.** `STEER_RELAY_URL` unset → the whole subsystem is **off**: `steer.mintTicket` returns a `disabled` result, the desktop never opens a control or publisher socket, and web/mobile show "Remote start & live steering unavailable on this instance." Local coding on the desktop still works fully (launcher, ghostty, PR via MCP) — the relay is purely additive, never load-bearing. This mirrors how `PUSH_RELAY_URL` unset disables push without breaking anything.
- **LAN-only.** Self-hosters can point `STEER_RELAY_URL` at a LAN address (e.g. `ws://relay.lan:4002`); because both desktop and clients dial *out*, it works with zero inbound firewall rules on the desktop. Ship the relay in `docker-compose.yaml` as an optional service and document it beside the push relay in CLAUDE.md's infra list.
- **Cloud.** New Coolify app `exponential-steer-relay` cloning the repo and building `Dockerfile.steer-relay` (context `.`), holding `STEER_RELAY_SECRET`; `/healthz` gates the HEALTHCHECK. Same manual-deploy posture as the other Coolify apps.

### 3.7 Web & mobile viewer UI

- **Web:** add `@xterm/xterm` (+ `@xterm/addon-fit`) to `apps/web`. A `<SteerTerminal>` component on the issue detail screen (beside the read-only PR diff): when a `running` `coding_sessions` row exists for the issue and steering is enabled, show **"Watch live"** / **"Take steering"**. It calls `trpc.steer.mintTicket`, opens the viewer WebSocket, `join`s by `sessionId`, pipes binary output frames into the xterm write path, and (if `perm === 'steer'` and it holds the claim) forwards xterm `onData` keystrokes as `input` frames. Show a presence bar (avatars from `presence`) and a "Start on my desktop" button when the user has an online device (sends `start_session`).
- **Mobile:** iOS/Android render bytes into a **lightweight native VT** (minimal parser fed from the WebSocket; SwiftTerm-class on iOS, a Compose terminal surface on Android). Read-only "watch" is the primary mobile use; "steer" sends soft-keyboard keystrokes as `input`. The phone is also the primary place to press **"Start on my desktop."** Both reuse the same `steer.mintTicket` proc and the same wire protocol, gated on the same permission + a `steer.config` proc that reports whether `STEER_RELAY_URL` is set.
- **No local runtime on web/mobile.** They have no terminal/agent runtime of their own — pure remote viewers/steerers of a desktop-hosted PTY.

### Definition of done

- [ ] `apps/steer-relay` (Hono/Bun, `@exp/steer-relay`) ships with `/healthz`, per-IP rate limit, `MAX_BODY_BYTES`, a Bun-native WebSocket hub, **device-presence registry** `(userId → deviceLabel/socket)`, **session rooms keyed by `sessionId` (== `coding_sessions.id`)**, and ring-buffer replay; `Dockerfile.steer-relay` + Coolify `exponential-steer-relay` app + optional `docker-compose.yaml` service.
- [ ] Wire protocol implemented on all four surfaces: control frames `hello / online / start_session / join / resize / input / presence / claim / release / kill / bye` + binary `0x01` output frames, with drop-non-input backpressure and slow-consumer eviction.
- [ ] `steer` tRPC router: `mintTicket` (session **or** personal apikey, workspace-permission-checked via `membership.ts` + `access.ts`, HS256 ticket signed with `STEER_RELAY_SECRET`), `killSession`, `config`. **No** `claim/renewClaim/release/forceClaim` procs (claim is relay-memory only). **No `remote_steer_sessions` table and no `steer_perm` enum** — removed.
- [ ] Native `SteerPublisher` on both desktops (Zig `apps/linux`; Swift `apps/ios/ExponentialMac`): tees the ghostty PTY bytes to the relay, injects remote `input` into the same PTY write as local keys, serves resync snapshots, honors `kill`, and is keyed by `sessionId` alongside its own ghostty surface + `claude` child. Multi-window = independent publishers, no shared slot pool.
- [ ] Remote-start path end-to-end: desktop holds an outbound control socket announcing device presence; phone `start_session{issueId,deviceId?}` → relay → chosen desktop runs the native launcher (§5), inserts `coding_sessions` (`running`), and starts publishing; phone joins by `sessionId` off the synced row.
- [ ] Single-steerer claim enforced in relay memory (only the `steererId` socket's `input` is forwarded); local desktop user always types + "Take over."
- [ ] Web `<SteerTerminal>` (xterm.js) + iOS/Android native VT viewers on the issue detail screen, with watch/steer + "Start on my desktop" buttons, presence bar, gated on permission + relay-enabled `steer.config`.
- [ ] `STEER_RELAY_URL` unset degrades cleanly everywhere (no control/publisher sockets, UI shows "unavailable", local coding + Electric sync unaffected); LAN-only outbound config documented in CLAUDE.md.

---

## 4. Desktop IDE workstream (Linux + macOS)

The two desktops are the IDE surface: an embedded libghostty terminal, JetBrains-style run configs with a play button, a **native "Start coding" launcher** that spawns the `claude` CLI against an issue, read-only PR diff review, and multi-window operation. There is **no Rust agent-core and no FFI** in this design — `crates/agent-core`, the C ABI, and both host bridges (`apps/linux/src/core/agent/*`, `apps/ios/ExponentialMac/MacAgentCore.swift` + the agent parts of `MacGhosttyApp.swift`) are **deleted**. What survives is purely native: the libghostty terminal embedding (Linux `GtkGLArea` + GL shim in `apps/linux/src/ui/terminal.zig` / `ghostty_ffi.zig`; macOS prebuilt `GhosttyKit.xcframework` in `MacGhosttyTerminal.swift`), the preview/run-target infrastructure, and the sync/UI layers. Linux (`apps/linux`, Zig + GTK4) must reach web 1:1 pixel parity; macOS (`apps/ios/ExponentialMac`, SwiftUI) keeps its glass aesthetic and is a **verify-and-polish** track on a real Mac.

The launcher and settings share one design, implemented twice (Zig + Swift). Local dependencies are **only** the `claude` CLI and `git` — **never `gh`**. All of GitHub stays server-side (the storage-free GitHub App), reached through tRPC and the web MCP server.

### 4a. The "Start coding" launcher (native, no core, no FFI)

**Decision (locked): the coding flow is a host-side native launcher.** A play/CLI button on an issue (local) OR a `start_session` command arriving over the relay (remote) runs the same sequence. There is no dispatcher, no assignment trigger, no plan-only headless run — the person coding is the **real signed-in user**, working interactively in the terminal. Steps (identical on both desktops):

1. **Resolve the repo (tRPC).** Call `repositories.forIssue({ issueId })` → `{ repositoryId, owner, name, defaultBranch }`. Repos come from the workspace repositories registry (server-only tRPC, not a synced shape). If the issue's project has no linked repo, the play button is disabled with a "Link a repository" hint.
2. **Mint a JIT push token (tRPC).** Call `repositories.installationToken({ repositoryId })` (session-gated). The server mints a short-lived GitHub App **installation token** for that repo (App JWT → per-repo installation token) and returns it. It is never persisted on the client beyond the life of the worktree remote.
3. **Host-side git — clone + worktree + branch (NO `gh`).** Under the configured workspace/repos root (settings, 4b), ensure a bare-ish local clone of `owner/name` exists (clone if missing, `fetch` otherwise). Create a git **worktree** with a new branch `exp/<ISSUE-IDENTIFIER>` off `origin/<defaultBranch>`. Configure that worktree's `origin` remote with a **token-embedded URL** — `https://x-access-token:<token>@github.com/owner/name.git` — so a later `git push` works with no `gh` and no personal credentials. (On Linux, extend `preview_config.repoCloneDir` / add a `git_worktree.zig`; on macOS, a `GitWorktree.swift` helper shelling out to `git`.)
4. **Write `.mcp.json` in the worktree.** Point Claude at the web MCP server (`<BASE_URL>/api/mcp`, Streamable-HTTP — the route in `apps/web/src/routes/api/mcp.ts`, server built by `apps/web/src/lib/mcp/server.ts`) authenticated with the **user's personal API key** (Better Auth apikey; managed in settings, 4b). Shape:
   ```json
   { "mcpServers": { "exponential": { "type": "http", "url": "<BASE_URL>/api/mcp",
       "headers": { "Authorization": "Bearer <personal-api-key>" } } } }
   ```
5. **Compose a plan-first prefilled prompt.** Build a prompt from the issue identifier + title + markdown description + the most relevant comments (fetched via tRPC or left for Claude to pull with the `get_issue` / `get_comments` MCP tools). The prompt instructs Claude to **first propose a concise plan and wait for the user's go-ahead, then implement**; when done, commit, push branch `exp/<IDENTIFIER>`, and open a PR via the `open_pr` MCP tool. Write it to a prompt file in the worktree.
6. **Spawn `claude` in an embedded ghostty terminal.** Launch `claude --dangerously-skip-permissions` (permissions **always** bypassed — the user never clicks accept), `cwd` = the worktree, seeded with the prefilled prompt (prompt file / arg). From there it is fully interactive: the local user (or a remote steerer) drives. Claude runs `git commit` / `git push` itself over the token remote and calls the `open_pr` MCP tool — **the server** opens the PR via the GitHub App and links it to the issue (deterministically, by parsing the `exp/<IDENTIFIER>` branch name; `open_pr` also records the link). PR fields (`prUrl`, `prNumber`, `prState`, `branch`) land on `issues`.
7. **Record the session + tee the PTY.** Insert a `coding_sessions` row (`{ issueId, workspaceId, userId, deviceLabel, status: 'running', startedAt }`) so coordination clients show a live "coding now" badge and a Watch/Steer button; flip to `ended` (`endedAt`) when the child exits. Attach the section-3 **`SteerPublisher`**, keyed by the `coding_sessions.id`, which tees the ghostty PTY bytes to the relay and injects remote `input` into the same PTY write as local keys.

**Trigger parity.** The local play/CLI button and the relay `start_session` command call the exact same launcher. The relay path (device presence, `start_session` fan-in) is owned by section 3; this section owns everything from "resolve repo" onward. One `coding_sessions` row ↔ one worktree ↔ one ghostty tab ↔ one relay steer room.

### 4b. Desktop settings (JetBrains-SDK style)

Both desktops surface a settings pane (Linux `apps/linux/src/ui/settings.zig`; macOS `MacSettingsView.swift`) with prefilled, editable defaults:

1. **Claude CLI path** — default `claude` resolved on `PATH`; editable to an absolute path. A one-shot `claude --version` "doctor" check (mirrors the existing preview `doctor()` in `preview_config.zig`).
2. **Workspace/repos root path** — where clones + worktrees live. Default `~/Exponential/repos` (macOS `~/Library/Application Support/...` or the same `~/Exponential/repos`), editable. Feeds step 3 of the launcher.
3. **Default branch prefix** — default `exp/`, editable; the launcher builds `<prefix><ISSUE-IDENTIFIER>`.
4. **Personal API key management** — mint once via Better Auth apikey and store in the desktop config / OS keychain (Linux already reads `apiKey` from the identity store, `settings.zig:878`; macOS Keychain). Show a "Generate / regenerate / copy" control and where it is written into `.mcp.json`. This replaces any deleted `expk_` agent-key concept — it is the **real user's** key.

`git` is assumed present (no install management); the settings doctor just reports its version.

### 4c. Run configs + play button (host-side arbitrary process launch)

**Decision (locked): run configs are HOST-SIDE**, spawned by the desktop app directly into a terminal-dock tab. This infrastructure **already largely exists** from commits `a8826d2` + `084aa97` (preview/run-target config): `apps/linux/src/ui/preview/preview_config.zig`, the `Mac*Preview*` files, and the committed `.exponential/config.json`. **Extend it — don't rebuild it** — by adding a generic `command` run target alongside the existing preview-shaped web/android/ios targets.

1. **Schema (`packages/db-schema/src/domain.ts`).** Add `command` to `platformValues` (currently `[web, android, ios]`, line 78) and a `commandTargetSchema` in the `runTargetSchema` discriminated union (line 356), added to the `RunTarget` TS union (line 294): `{ platform: 'command', id, name, argv: string[], cwd?: string (repo-relative, reject '..'), env?: Record<string,string> (strip PATH/LD_PRELOAD/DYLD_* like the other targets) }`. The DB mirror `ProjectPreviewMirror.targets` carries only `{id,name,platform}`, so command configs surface in web settings + cross-client for free.
2. **Linux parser (`preview_config.zig`).** Add `command` to the `Platform` enum + `fromString`/`label` (lines 24–36), add `argv: ?[]const []const u8` reusing `root_dir`/env on `RunTarget` (line 50), and a `parseTarget` arm (line 145) reading `argv` + `cwd`. Fold `argv`/`cwd` into `commandSetHash` (line 217) so the existing repo-trust gate re-prompts when a command changes — `.exponential/config.json` is repo-carried and agent-editable, so the trust prompt is the security boundary; do **not** weaken it.
3. **Host spawn (Zig).** Add a `run_launcher.zig` next to `terminal.zig` that, given a parsed `command` target and the repo clone dir (`preview_config.repoCloneDir`), spawns `argv` with `cwd`+`env` into a **new terminal-dock tab** (4e). Capture the child exit code directly (no core round-trip) and record it in an in-memory run-history ring per target.
4. **Play-button menu (top bar).** In `apps/linux/src/ui/app.zig`, add a play button to the `adw_header_bar`. Clicking opens a `GtkPopoverMenu` with two groups: **Start coding** (the 4a launcher, per current issue) and **Run configs** (parsed command + preview targets). Selecting a config launches it (or re-prompts trust); show last exit code + a spinner; a "Stop" entry destroys the tab's ghostty surface (which kills the child). Persist last-selected target id per repo (sibling `last-run.json` beside the trust store).
5. **macOS mirror.** `MacShell.swift` / `MacTerminalDock.swift` / `MacPreviewBackends.swift` already spawn preview backends. Add the same toolbar `Menu` (**Start coding** + **Run configs**) and a `Process`-based launcher into a `MacGhosttyTerminal` tab, reusing the `.exponential/config.json` read path (`MacPreviewConfig.swift`).

**Output/history** is host-side state only (not synced) for v1 — the menu shows "last run: exit 0 · 2m ago" per config. No new Electric shape.

### 4d. Multi-window + concurrent sessions (v1) — now trivially native

Without a Rust core there is **no shared slot pool and no ABI concurrency gate**. Each terminal tab / detached window is simply **one ghostty surface + one `claude` (or run-config) child in its own worktree**. Concurrency is inherent: N issues = N worktrees = N tabs. The only real constraints are the ghostty embedding gotchas.

**Linux — tabbed dock + detach.** Today `apps/linux/src/ui/terminal_dock.zig` holds one `term_slot`/`current_term` and `mountTerminal` replaces the prior run (`terminal_dock.zig:78`), driven from `apps/linux/src/ui/app.zig`'s single window. Rework:

1. **Terminal dock → tabbed.** Replace the single `term_slot`/`current_term` with an `AdwTabView` of terminal tabs keyed by `coding_sessions.id` (coding sessions) or run-config id. The `mountForManager`/`unmountForManager` hooks (`terminal_dock.zig:108/116`) become add-tab / close-tab-by-key. Honor the gotcha: a ghostty surface inits lazily **only at nonzero size**, so only realize a tab's `GtkGLArea` when the dock is expanded at a real height — keep the `set_size_request(-1, 200)` floor (`terminal_dock.zig:32`) and the paned split logic. And keep handling `GHOSTTY_ACTION_RENDER` in the action callback (`terminal.zig:115`, `ghostty_ffi.zig:62`).
2. **Detach-to-window.** Add a "pop out" affordance that **reparents** a tab's ghostty terminal into a new `adw_application_window` (`gtk_application` supports many top-levels — the single-window assumption is only in the current code, not in GTK). Because destroying a ghostty surface kills the child, **reparent, never recreate**. Same for the diff view (4e) and the preview webview → detached terminal / diff / preview windows.
3. Concurrent coding sessions coexist because each is its own worktree + tab; nothing is shared.

**macOS — multiple Window scenes.** `ExponentialMacApp.swift` has a single main scene and `MacTerminalDock.swift` binds one `MacGhosttyTerminalView`. Add a `WindowGroup(id:for:)` / `Window` scene for detached terminal/diff/preview windows keyed by `coding_sessions.id`, and make `MacTerminalDock` a tabbed host over concurrent sessions. Same rule: **reparent** the `GhosttyKit` surface into the detached window rather than tearing it down; respect the nonzero-size + `ACTION_RENDER` gotchas.

### 4e. Linux 1:1 web parity (the #1 UI-quality gap)

Linux is Adwaita-styled and visibly diverges from web — the biggest open UI-quality item. The fix is **hand-rolled native GTK widgets sized to the web's pixel dimensions**, not Adwaita defaults. Debt catalogue (read `apps/linux/src/ui/{app,gtk,widgets,format}.zig` + the app's CSS classes `exp-sidebar`, `card`, `diff-line`):

1. **Buttons too big / wrong metrics.** Define an `exp-btn` CSS class matching shadcn heights (default `h-9`=36px, sm `h-8`=32px, icon `h-5 w-5`), font-size, radius, and horizontal padding; apply everywhere instead of bare `gtk_button_new_*`. Sidebar/nav rows match the web row height; the 260px sidebar width is already pinned (`app.zig:698`).
2. **Wrong components / issue-row grid.** Audit `widgets.zig` for stock Adwaita rows/lists where web uses a specific shadcn primitive (status/priority dropdowns, label pills, filter pills). Rebuild the issue row as a fixed-column `GtkGrid`/`GtkBox` matching `grid-cols-[24px_72px_24px_1fr_auto]` (priority · identifier · status · title · labels+due) with the same gaps.
3. **Spacing / fonts / tokens.** Establish a CSS token layer (spacing scale, Inter stack, OKLCH zinc colors, `--radius`) mirroring `apps/web/src/styles.css`; replace ad-hoc `set_margin_*` with tokens.
4. **Row virtualization.** The list is a `gtk_list_box` in a `gtk_scrolled_window` (`app.zig:867–873`) that materializes every row — janky on large workspaces. Move to `GtkListView` + `GtkSignalListItemFactory` over a `GListModel` (recycling), or `GtkColumnView` for the multi-column row. Structural rewrite of `refreshIssues`/`onIssueActivated`; keep the status-group collapse behavior.
5. **Plaintext diff → syntax-highlighted side-by-side.** The current `diffFileWidget` (`app.zig:2024`) renders each patch line as a `GtkLabel` with `diff-add`/`diff-del`/`diff-hunk` CSS — unified, plaintext. Build a real **side-by-side** view with `GtkSourceView` (`GtkSourceLanguageManager` guesses language from `PullFile.filename`), two columns (old / new) driven by parsing the unified `patch` into hunks; keep the `+N -N` header. Data source is unchanged (`issues.prFiles` via `fetchPullFiles`, the same query the Zig side already calls in `prDiffWorker`). Read-only for v1 (locked), but keep the hunk model line-anchored so comments can attach later.

Deliver 4e as an incremental parity pass: (1) token+button CSS, (2) issue-row grid, (3) list virtualization, (4) side-by-side diff — each verified against a side-by-side web screenshot.

### 4f. macOS: keep glass, verify-and-polish on a real Mac

The macOS design divergence is LOCKED: keep the SwiftUI `.ultraThinMaterial` glass aesthetic, aligning only semantic status/priority tokens with web — do **not** chase pixel parity (that's Linux's job). The work is **runtime verification** on a real Mac against `next.exponential.at`, **minus everything agent-core/FFI** (deleted, so nothing there to verify — remove `MacAgentCore.swift`, the agent parts of `MacGhosttyApp.swift`, `MacAgentService.swift`/`MacAgentPanel.swift`/`MacAgentRunMonitor.swift` and their bindings):

- **Login + read-only live sync:** sign in (Better Auth session); confirm all 14 Electric shapes populate (workspaces/projects/issues/…/`coding_sessions`).
- **CRUD:** create/edit/status/priority/assignee/label-toggle/comment mutations round-trip via tRPC + `generateTxId` and appear over Electric.
- **Markdown editor:** the WYSIWYG description/comment editor round-trips the GFM contract (bold/italic/strike/code/H1–H3/lists/task-lists/blockquote/code blocks/links/images/@mentions) byte-identically to web/iOS; attachment upload works.
- **The NEW launcher (4a):** play button on an issue resolves the repo, mints the JIT token, builds the worktree + `exp/<IDENTIFIER>` branch with token remote, writes `.mcp.json` with the personal key, and spawns `claude --dangerously-skip-permissions` in a `MacGhosttyTerminal`; Claude's `git push` + `open_pr` MCP call opens a PR and links it; a `coding_sessions` row appears and ends.
- **Run configs + play menu (4c):** the menu launches command run targets into `MacGhosttyTerminal` tabs; exit code recorded.
- **Multi-window (4d):** multiple Window scenes; detached windows reparent `GhosttyKit` surfaces without killing children.
- **Steer publisher (§3):** the per-session publisher streams the PTY to the relay and injects remote steer input; local user can always type + take over.
- **libghostty terminal:** the `GhosttyKit.xcframework` surface renders on a real display and accepts input; honor the gotchas — surface inits only at nonzero size (mount only when expanded), handle `GHOSTTY_ACTION_RENDER` in the action callback, and **never build libghostty from source on macOS** (link the prebuilt xcframework).

Release-time notarization (Developer ID cert, real `codesign`, `notarytool submit`) stays a release-checklist item, not this workstream.

### Definition of done

- [ ] **Native "Start coding" launcher** on both desktops (Zig + Swift), no Rust core / no FFI: `repositories.forIssue` → `repositories.installationToken` → host-side git clone + worktree + `exp/<IDENTIFIER>` branch + token-embedded remote (NO `gh`) → `.mcp.json` (web `/api/mcp` + personal key) → plan-first prefilled prompt → `claude --dangerously-skip-permissions` in an embedded ghostty terminal (cwd = worktree).
- [ ] Claude self-drives `git commit`/`push` (token remote) and calls the `open_pr` MCP tool; the **server** opens + links the PR; `issues.prUrl/prNumber/prState/branch` populate; a `coding_sessions` row goes `running` → `ended`.
- [ ] Launcher is triggered by both the issue play/CLI button and a relay `start_session` command; the section-3 `SteerPublisher` is attached, keyed by `coding_sessions.id`.
- [ ] **Desktop settings** (both): Claude CLI path, workspace/repos root, branch prefix (`exp/`), and personal API key management (minted once, stored in config/keychain, written into `.mcp.json`); with a `claude --version` / `git --version` doctor.
- [ ] **Run configs:** `.exponential/config.json` schema + Linux parser extended with a generic `command` target (`argv`/`cwd`/`env`); `commandSetHash` covers the new fields so the trust gate re-prompts on command edits; host-side spawn into a terminal-dock tab on both desktops (exit code + per-config history captured).
- [ ] Top-bar **play menu** on both desktops grouping **Start coding** + **Run configs**, per-repo last-selected memory, and a Stop action.
- [ ] **Multi-window:** Linux tabbed `AdwTabView` dock + detach-by-reparenting (terminal/diff/preview); macOS multiple Window scenes + tabbed dock — concurrent coding sessions coexist, each its own ghostty + worktree, NO shared slot pool, honoring the nonzero-size + `ACTION_RENDER` gotchas (reparent, never recreate).
- [ ] **Linux parity:** shadcn-sized `exp-btn` CSS + token layer, issue-row `grid-cols-[24px_72px_24px_1fr_auto]` fixed-column grid, `GtkListView`/`GtkColumnView` virtualization, and a `GtkSourceView` side-by-side syntax-highlighted diff replacing `diffFileWidget`'s plaintext labels — each verified against a web screenshot.
- [ ] **macOS:** all agent-core/FFI code removed; login/sync/CRUD/editor + the new launcher + run configs + multi-window + steer publisher runtime-verified on a real Mac against `next.exponential.at`, honoring the ghostty gotchas; glass aesthetic preserved.

---

## 5. Coordination clients workstream (web + iOS + Android)

These are the three **non-IDE** surfaces. They create/triage issues, comment, review PRs, **remote-start** a coding session on the user's own desktop, and **remotely watch + steer** that live desktop terminal — but they run **no local terminal, no CLI, and no agent runtime**. All `claude`/`git` spawning is desktop-only (Sections 2 + 4); web/mobile only read Electric rows, call tRPC, and speak to the relay (Section 3). This boundary is load-bearing and store-policy safe: nothing here shells out.

There is **no assignment-to-agent concept anywhere in this workstream**. The person coding is the **real user** under their own session. "Start on my desktop" is a relay control command to that user's own online device — not an assignment, not a synthetic agent user, not a device-registration flow. Assignee stays a plain human-to-human field.

Ground-truth files this workstream touches:
- Web list/detail: `apps/web/src/components/issue-list.tsx`, `issue-detail-view.tsx`, `agent-panel.tsx` (repurposed → **session/steer panel**), `diff-view.tsx`; issue properties in `apps/web/src/components/issue-properties/`, row menu in `apps/web/src/components/issue-row-menu/`; sidebar `apps/web/src/components/workspace/sidebar.tsx`, mobile nav `apps/web/src/components/workspace/mobile-topbar.tsx`; routes under `apps/web/src/routes/w/$workspaceSlug/`.
- iOS: `apps/ios/Exponential/UI/Issue/{IssueListView,IssueDetailView,DiffView,CommentThreadView,PickerSheet}.swift`, `UI/Home/`, `UI/Navigation/{MobileTabBar,AppNavigator}.swift`, `UI/Markdown/`. Note: `UI/Issue/AgentPlanPanel.swift` is **deleted** (no plan-panel model in v2) and replaced by a steer panel.
- Android: `apps/android/app/src/main/java/com/exponential/app/ui/issue/{IssueListScreen,IssueDetailScreen,PrDiffSection,CommentThread,SwipeableIssueRow,IssuePickerSheet}.kt`, `ui/home/HomeScreen.kt`, `ui/markdown/`. Note: `ui/issue/AgentPlanPanel.kt` + `AgentPlanPanelViewModel.kt` are **deleted**, replaced by a steer panel.

### 5a. "My Issues" — first-class cross-project view (assignee = me)

Decided: a top-level, cross-project view filtered to `assigneeId == currentUser`, present on **all three** coordination clients, above the per-project lists. No new column and no new shape — `issues.assigneeId` already exists and is indexed; the `issues` Electric shape (one of the 14 synced shapes) already carries everything needed. Pure client work.

**Web**
1. New route `apps/web/src/routes/w/$workspaceSlug/my-issues/index.tsx`. Query `issueCollection` (`apps/web/src/lib/collections.ts`) with `useLiveQuery`, `where eq(issue.assigneeId, session.user.id)` across the whole workspace (join issues→projects to scope by workspace; projects are already synced). Reuse the status grouping and reuse `matchesFilters` / tab presets from `apps/web/src/lib/filters.ts`. Group by status like the project board; prefix each row's identifier with its project since rows span projects. Row click → the existing full-page detail route `projects/$projectSlug/issues/$issueIdentifier`.
2. Sidebar entry in `workspace/sidebar.tsx`: add a `SidebarMenuItem` in the same nav group as Search/Inbox, icon `CircleUser` from `lucide-react`, `Link to="/w/$workspaceSlug/my-issues"`, placed above Inbox.
3. Mobile: add "My Issues" to `workspace/mobile-topbar.tsx` navigation.
- Reminders: use `and()`/`or()` from `@tanstack/react-db` (never JS `&&`), return `undefined` (not `false`) to skip the query while `session` is loading, and rely on `snakeCamelMapper` (already set on the collection) so `assigneeId` resolves.

**iOS**
1. New `apps/ios/Exponential/UI/MyIssues/{MyIssuesView,MyIssuesViewModel}.swift`. The view model queries the local GRDB store for `assignee_id = activeUserId` across all projects in the active account (mirror `IssueListViewModel`'s fetch, drop the project predicate). Reuse the existing row cell from `IssueListView.swift`; show a project prefix per row.
2. Add a **My Issues** tab to `UI/Navigation/MobileTabBar.swift` (SF Symbol `person.crop.circle`) so the order is Projects · My Issues · Inbox; wire routing in `AppNavigator.swift`.

**Android**
1. New `ui/myissues/{MyIssuesScreen,MyIssuesViewModel}.kt`. View model observes the Room DAO with `assigneeId = currentUserId` across projects (mirror `IssueListViewModel`); reuse `SwipeableIssueRow`.
2. Add the destination to the bottom navigation used by `HomeScreen.kt`, matching the Projects/Inbox pattern with a person icon.

Cross-client parity note: keep "assignee = me, all projects, grouped by status" identical on all three; no saved-view or custom-filter machinery (cut list) — My Issues is a fixed built-in view.

### 5b. "Start on my desktop" — remote-start a coding session (no assignment, no agent user)

Decided: from an issue, a **Start on my desktop** button sends a relay `start_session` control command to the user's **online desktop device**. The desktop then runs the host-side launcher (Section 2/3): resolve repo → mint JIT installation token → worktree + `exp/<IDENTIFIER>` branch → write `.mcp.json` → spawn `claude --dangerously-skip-permissions` in an embedded libghostty terminal, and begin publishing the PTY to the relay. **No assignment, no synthetic agent user, no run endpoint** — the desktop is the user's own machine, driven over the relay's outbound control channel.

Device model (relay presence, no DB table):
- Each open desktop app holds an **outbound control connection** to the relay, publishing presence "device D of user U is online" with a human `deviceLabel`. Presence lives in **relay memory** (a relay frame), not a synced row.
- Coordination clients learn the user's online devices via a relay presence lookup (Section 3 exposes it through a tRPC proc, e.g. `relay.myDevices`, or a lightweight SSE presence subscription). The button is:
  - **enabled** when ≥1 of the current user's own devices is online; a **device picker** appears when there are several (pick which desktop to run on);
  - **disabled with a hint** ("No desktop online — open the Exponential desktop app to run here") when none are connected.
- Sending: `relay.startSession({ issueId, deviceId })` → relay forwards `start_session` to that device → desktop launches. The command carries only `issueId` (+ optional branch prefix override); the desktop resolves everything else server-side.

Where it lives in the UI:
- **Web**: a primary button in `agent-panel.tsx` (now the **session/steer panel**) on the issue detail, plus a compact entry in the issue-row overflow (`issue-row-menu/`). The panel reflects device online/offline state live.
- **iOS**: a button in `IssueDetailView.swift`'s session section; device picker as a `PickerSheet`.
- **Android**: a button in `IssueDetailScreen.kt`; device picker reusing `IssuePickerSheet`-style sheet.

Permission: gated by `WorkspacePermissions` (mirror of `apps/web/src/hooks/use-workspace-permissions.ts`) — only a member who may act on the issue can start a session; enforcement is relay/server-side, the UI just reflects it. Because the target device belongs to the requesting user, cross-user remote-start is not offered.

### 5c. Remote steer UI (watch + type into the live desktop terminal)

Decided: the coordination clients render a **live terminal** mirroring a running desktop coding session and can type into it, driven entirely through the relay. Electric carries only the `coding_sessions` row; the PTY frame stream and the input RPC ride the relay's WebSocket/SSE. Web/mobile are pure relay **clients** — they never spawn a PTY.

Discovering the live session (two signals, both required):
1. The synced **`coding_sessions`** shape (one of the 14) — read a row for this issue with `status = 'running'` (fields: `id, issueId, workspaceId, userId, deviceLabel, status, startedAt, endedAt`). This is the coordination anchor: it also drives a **"coding now" badge** on issue rows/detail and the **Watch/Steer** button across clients. No `agent_runs`, no structured plan state.
2. A relay **liveness** check — confirm the desktop is still publishing (the row can lag a crash by a heartbeat). Only show the terminal when both agree.

Relay session contract (consumed here, defined in Section 3):
- **`steer.mintTicket({ codingSessionId })`** → a short-lived ticket the client exchanges for a relay socket carrying PTY frames.
- **Steer claim + viewer presence live in relay memory** (no `remote_steer_sessions` table). One holder may type at a time; others are view-only. The relay publishes a **presence frame** (who's watching, who holds the claim). A **presence bar** in the UI renders it.
- While a client holds the claim, it forwards keystrokes client→relay→desktop; all clients receive frames desktop→relay→client.

**Web** — new `apps/web/src/components/steer-terminal.tsx` (`<SteerTerminal>`) using **xterm.js** (`@xterm/xterm` + `@xterm/addon-fit`; add to `apps/web/package.json`). Mount it inside `agent-panel.tsx` (the session/steer panel) when a running `coding_sessions` row + relay liveness exist. A **Take control** button requests the steer claim; while held, forward `onData` to the relay socket and `term.write` incoming frames. View-only clients get frames, no input. A presence bar shows watchers + current steerer. Also expose it on the full-page issue route so it survives navigation.

**iOS** — new `apps/ios/Exponential/UI/Session/SteerTerminalView.swift` (lightweight VT viewer). No SwiftUI terminal primitive exists; render frames into a monospaced scrollback surface with a VT100-subset parser (sufficient for a mirror — the real terminal runs on the desktop). A **Take control** toggle acquires the claim; a bottom input row sends keystrokes over the relay socket. A compact presence bar at top. Surfaced from `IssueDetailView.swift`.

**Android** — new `ui/session/SteerTerminalScreen.kt`, same shape: a monospaced scrollback `LazyColumn`/`Text` fed by relay frames, a claim toggle, an input field, and a presence bar. Surfaced from `IssueDetailScreen.kt`.

Permissions: `WorkspacePermissions` gates who may request `steer`; public/read-only viewers get view-only or nothing. Enforcement is relay-side; the UI reflects the granted permission.

### 5d. Read-only PR diff review on all platforms

Decided: **read-only, syntax-highlighted, side-by-side** diff on every client, with the data model kept **write-back-ready** so inline comments + approve/request-changes can be added later without a rewrite. All three clients already fetch patches via `trpc.issues.prFiles` (backed by `issues.prFiles`, populated server-side via the GitHub App's `fetchPullFiles`); today they render a flat `<pre>` with +/−/@@ coloring (web `diff-view.tsx`, iOS `DiffView.swift`, Android `PrDiffSection.kt`). Bring them to parity and upgrade rendering.

1. **Schema-anchored for write-back (do now, render later):** extend the `prFiles` payload's `PullFile` so each hunk/line carries the stable anchors GitHub needs for future review comments — `path`, `sha`/blob refs, per-line `side` (LEFT/RIGHT), and `line`/`position` — even though v1 discards them. The current web `PullFile` (`{ filename, status, additions, deletions, patch? }`) grows these fields; iOS `PrFile` and Android `data/api/PullFile` grow the matching fields. This payload concern is owned with Section 4 (GitHub/PR); the clients must **thread the fields through** so a later inline-comment layer has anchors with no rework. No comment UI in v1.
2. **Web**: replace the flat `<pre>` in `diff-view.tsx` with a real side-by-side view. Parse the unified patch into hunks (left/right columns), syntax-highlight with a lazy-loaded lightweight tokenizer (`shiki` or `prism`), keep the existing add/del/context/hunk color language. Preserve the "View changes" entry point in the session panel.
3. **iOS**: upgrade `DiffView.swift` to a two-column (or unified-with-gutter, given phone width) syntax-highlighted view; keep the glass tokens; reuse web's parse-hunks logic conceptually.
4. **Android**: upgrade `PrDiffSection.kt` similarly (Compose two-pane on tablet width, unified on phone).
5. Keep the existing loading/empty/error/binary-fallback states in all three (do not regress "No textual diff (binary or too large)" / "Renamed.").

### 5e. Issue-to-issue linking + duplicate-of

Decided: two features.

**(1) Clickable issue-reference pills** in descriptions and comments, resolved like `@mentions`. Extend the **existing mentions pipeline** (`apps/web/src/lib/integrations/mentions.ts`), do not fork it.
- Markdown interchange form: references are the **issue identifier** token (e.g. `MET-1153`) — the same "single interchange form, round-trips as plain GFM text" rule the contract uses for `@email` mentions. Byte-parity across web/iOS/Android editors stays intact.
- Server resolution: add `extractIssueRefs(text)` + `resolveIssueRefs(tx, text, workspaceId)` mirroring the existing `extractMentionEmails` / `resolveMentions` (same `(tx, text, workspaceId)` signature). Regex `{PREFIX}-{number}`; resolve against `issues` joined to `projects` in the same workspace (a ref only pills when the target is visible). Call from `lib/trpc/comments.ts` (alongside `resolveMentions`) and from `lib/trpc/issues.ts` on description save.
- Client rendering: the markdown renderers already pill known `@email` mentions; add an issue-identifier pill renderer in the same place — web editor (`apps/web/src/components/issue-editor/`, a TipTap node/decoration mirroring the mention pill) and the iOS/Android markdown renderers (`UI/Markdown/`, `ui/markdown/`). A resolved pill is clickable → navigate to that issue's detail route; an unresolved token renders as plain text.
- Editor autocomplete (web): mirror the existing `@`-mention autocomplete — trigger on a `#`-style prefix, offer workspace issues (title + identifier), insert the `{PREFIX}-{number}` token. Mobile ships pill **rendering** first; autocomplete is a fast-follow.
- No new notification type for a plain reference in v1 (keep it a link). A "referenced-in" signal, if wanted later, reuses the existing notification delivery (Section 6) — don't add it speculatively.

**(2) Duplicate-of** — mark an issue as a duplicate of a canonical issue (a resolution). The greenfield schema bakes this in from day one — no migration, no backfill.
- Schema (defined directly in `packages/db-schema/src/schema.ts`): `issues.duplicateOfId text references issues(id) on delete set null` (self-FK), and `issue_status` includes a **`duplicate`** value (added to `packages/domain-contract/contract.json`; run `bun run --filter @exp/domain-contract generate` to refresh the Swift/Kotlin constants). Both are part of the initial fresh schema, present across all synced surfaces from the start: the `issues` shape passes `duplicateOfId` through automatically; add the column/enum value to the web `Issue` type, the Zig `sync_manager.zig` specs[] (+ its expectEqual test), and the iOS/Android issue entity + DAO column lists. Native clients still tolerate an unknown column defensively (forward-evolution note), but no ALTER self-heal choreography is needed on greenfield.
- Behavior: marking duplicate sets `duplicateOfId` **and** moves `status` to the new terminal `'duplicate'` value (distinct from `cancelled`, so "duplicate" reads as a real resolution). Handled atomically by the update mutation in `lib/trpc/issues.ts`.
- UX (all three clients): a "Mark as duplicate…" action in the issue row/overflow menu (`issue-row-menu/` web; iOS `PickerSheet`/context action; Android `SwipeableIssueRow`/overflow) opening an issue picker (reuse the 5e-(1) ref-autocomplete list). On the detail view, when `duplicateOfId` is set, show a **canonical-issue banner** "Duplicate of {IDENTIFIER}" with a clickable pill (reuse the 5e-(1) pill component). Provide an "unmark" affordance clearing the FK and restoring status.

### 5f. No local terminal / agent runtime on web or mobile (confirm)

Confirmed and enforced by construction:
- The coordination clients contain **no** process spawning, **no** `claude`/`git`/`gh` invocation, **no** run-config execution, and **no** agent runtime of any kind. Those belong exclusively to the desktop apps (Sections 2 + 4). There is no Rust core, no FFI, no C ABI anywhere in this stack (deleted entirely in v2).
- The only "start" action here is **`relay.startSession`** (5b) — a control command to the user's own online desktop, not an assignment and not a local run.
- The only terminal here is a **remote mirror** of a desktop PTY over the relay (5c) — receive frames, send keystrokes while holding the claim; never a local shell.
- The diff (5d) is **read-only** on all clients in v1.
Any PR that adds a spawn, a bundled CLI, or a local PTY to web/iOS/Android is out of scope and must be rejected.

### Definition of done

- [ ] "My Issues" route + sidebar/tab entry live on web, iOS, and Android; filters issues by `assigneeId == me` across all projects in the workspace, grouped by status, rows clickable to detail. No new column/shape.
- [ ] "Start on my desktop" button on all three clients sends `relay.startSession({ issueId, deviceId })` to the user's own online device; device picker when several; enabled/disabled + hint driven by live relay presence; **no assignment, no agent user, no run endpoint**.
- [ ] `<SteerTerminal>` (web, xterm.js + addon-fit) and iOS/Android lightweight VT viewers connect via `steer.mintTicket`, render live PTY frames (view), and send keystrokes when holding the steer claim; presence bar renders relay presence; permission gated via `WorkspacePermissions`.
- [ ] Live session discovered via the synced `coding_sessions` row (`status='running'`) + relay liveness; "coding now" badge + Watch/Steer button shown from that row. No `agent_runs`, no plan panels.
- [ ] PR diff upgraded to syntax-highlighted side-by-side (web/iOS/Android), read-only, loading/empty/error/binary states preserved; `PullFile`/`PrFile` types carry write-back anchors (path/sha/side/line) though no comment UI ships.
- [ ] Issue-reference pills render + resolve in descriptions + comments on all clients via the extended `mentions.ts` (`extractIssueRefs`/`resolveIssueRefs`); identifier token is the GFM interchange form; web editor autocomplete inserts it.
- [ ] `issues.duplicateOfId` self-FK + `issue_status='duplicate'` in the greenfield schema and contract.json (regenerated constants), mirrored across web/Zig/iOS/Android sync; "mark as duplicate" UX + canonical-issue banner + unmark on all clients; marking sets status `'duplicate'`.
- [ ] Deleted: `AgentPlanPanel.swift`, `AgentPlanPanel.kt` + `AgentPlanPanelViewModel.kt`, and any assign-to-agent affordance; `agent-panel.tsx` repurposed to the session/steer panel.
- [ ] Zero local terminal / agent runtime / CLI spawn on web or mobile; the only terminal is a remote relay mirror; diff stays read-only in v1.

---

## 6. Notifications, email & one-way helpdesk

This workstream turns notifications into a **three-channel delivery layer** — in-app + push + **email**, all three **free and un-gated** — wires the away/phone killer flow end-to-end, and ships a **one-way helpdesk**: an external widget reporter gets a clean resolution email when their reported issue is closed. Email and push are **table-stakes**; the moat is seats/repos/tier, never "nothing gets lost."

There is **no headless agent** in v2. The person coding is the real user working under their own session in an interactive terminal (see the coding-launcher workstream). So there is no plan-ready/question event to route to an approver, and **all agent-action-notify special-casing is deleted**. Notifications are ordinary user-to-user fan-out: created / assigned / commented / mentioned / pr-opened / pr-merged.

Ground truth for this section: `apps/web/src/lib/integrations/notifications.ts` (the `deliver()` fan-out + the `fireAndForget*` callers), `apps/web/src/lib/email.ts` (the single sender), `apps/web/src/lib/integrations/fcm.ts` (`sendToUser`), `apps/web/src/lib/integrations/pr-sync.ts` (`applyPrMergeState`), and `apps/web/src/lib/widget/*` (submit pipeline + the per-widget bot user).

### 6.1 The email primitive (a delivery channel, not a notification type)

The core rule: notifications keep their `notification_type` enum (`packages/db-schema/src/domain.ts` → `notificationTypeValues`); do **not** add `email_*` variants. **Email is a third fan-out leg inside the existing `deliver()`**, beside the in-app row write and the push call in `notifications.ts`.

**Fix the notification-type enum for v2.** `notificationTypeValues` today is `issue_assigned`, `issue_comment`, `issue_status_changed`, `issue_mention`, plus the now-dead `agent_plan_review` and `agent_question`. **Remove `agent_plan_review` and `agent_question`** (no headless agent posts them) and **add `pr_opened` and `pr_merged`** as first-class notification types so PR events fan out on all three channels — today they only ride the push `data.type` discriminator and the activity feed. Mirror the change in `packages/domain-contract/contract.json` and run `bun run --filter @exp/domain-contract generate` to refresh the Swift/Kotlin constants. Because the DB is **greenfield** (no production data), this is just the initial enum definition — no `ALTER TYPE`, no migration gymnastics.

Also trim `PushType` in `notifications.ts` (lines 18–24): drop `plan_awaiting_approval` and `agent_error`; keep the notification types plus `pr_opened`/`pr_merged` (now themselves notification types, so `PushType` collapses toward `NotificationType`).

**Extend `deliver()` (`notifications.ts:104`).** Today it (1) writes `notifications` rows, then (2) fires push. Add a third leg after the row write:

- Add a stable `deepLinkPath` (and reuse `body` as the email body) to the `deliver` args so the email renders a real "Open in Exponential" button, e.g. `/w/{slug}/projects/{slug}/issues/{identifier}`. Resolve the workspace/project slugs by extending `loadIssueMeta`'s select (it already joins `projects`).
- For each **delivered** recipient (the `deliveredIds` set the dedupe insert `RETURNING`s — reuse it so email honors the same 30s dedupe window), resolve email eligibility and send. Fan email out with `Promise.all`, fully independent of the push branch. **Never let an email failure throw** — wrap per-recipient sends in try/catch with a `[notify]`-style `console.error` only.

**Un-gate push AND email from billing (both free).** In `apps/web/src/lib/billing.ts` the plan-limits shape carries a `push: boolean` (line ~23) and `canUsePush()` (line 263) returns `limits.push`. Per decision, both push and email are free: stop calling `canUsePush()` from `deliver()` — always attempt push and email. Remove `push` from the plan-limits shape and delete `canUsePush` if nothing else reads it (grep `canUsePush`/`limits.push`; note `assertWithinStorageLimit` and seat/repo gating stay). The `pushed_at` stamp in the insert becomes unconditional (`now`, not `canPush ? now : null`). Do **not** add a `canUseEmail`.

**Send path — `email.ts` is the single sender.** It already implements graceful degradation: `sendEmail()` no-ops with a stderr log when `RESEND_API_KEY` is unset, and exports `emailEnabled`. Extend it, don't fork it:

- Add an **SMTP transport** alongside Resend for self-host. Chosen default: `RESEND_API_KEY` set → Resend (existing fetch path); else `SMTP_HOST` set → SMTP via `nodemailer`; else the existing logged no-op. Update `emailEnabled = Boolean(RESEND_API_KEY || SMTP_HOST)`. New env: `SMTP_HOST`/`SMTP_PORT`/`SMTP_USER`/`SMTP_PASS`/`SMTP_SECURE` (or a single `SMTP_URL`), reusing `EMAIL_FROM`. Document in `.env.example` + the CLAUDE.md env table.
- Add `sendNotificationEmail({ to, subject, heading, body, actionLabel: "Open in Exponential", actionUrl, unsubscribeUrl })` next to `sendPasswordResetEmail`/`sendVerificationEmail`, reusing `actionEmailHtml` (extend it to append an unsubscribe footer line). The auth emails stay exactly as-is.

**Per-user email prefs + unsubscribe.** `users` is a Better Auth table (`packages/db-schema/src/auth-schema.ts`) — do **not** add app columns there. Add a dedicated **server-only** table (per target schema §10, `user_notification_prefs` is server-only, NOT an Electric shape):

- `user_notification_prefs`: `userId text pk → users.id (cascade)`, `emailEnabled boolean default true`, optional per-type opt-outs (`emailOnComment`, `emailOnStatus`, `emailOnMention`, `emailOnAssigned`, `emailOnPr`) and a `digest` enum (`off`|`daily`, default `off`). `unsubscribeToken text unique` (opaque `randomUUID`, minted lazily on first send). Missing row ⇒ all-defaults (email on). Defined in the greenfield schema; no backfill.
- Add `emailRecipients(userIds)` in `notifications.ts` that joins `users` (for `email`, skipping the bot/null-email users) and `user_notification_prefs`, returning `{ userId, email, unsubscribeToken }[]` filtered by the relevant per-type flag. `deliver()` calls it to build its email fan-out set.
- **Unsubscribe route:** a public file route `apps/web/src/routes/api/email/unsubscribe.ts` (GET `?token=…`) flips `emailEnabled=false` for the matching token and returns a small confirmation page. Every notification email's `unsubscribeUrl` is `${BETTER_AUTH_URL}/api/email/unsubscribe?token=…`. One-click / CAN-SPAM — non-optional.
- **Settings UI (web-only):** a small "Email notifications" panel under account settings toggling the same prefs via a tRPC `notifications.updateEmailPrefs` mutation. Native clients get no email-prefs UI for v1 (link to web, mirroring the billing/calendar web-only pattern).

**Optional digest (model now, cron later).** When `digest='daily'`, `deliver()` skips the immediate email; a cron (self-host env `EMAIL_DIGEST=true`, cloud a scheduled task) batches each user's unread `notifications` rows into one daily digest via `sendNotificationEmail`. Ship the pref + the skip-immediate branch now; the cron lands last with no schema change.

### 6.2 Normal user fan-out (created / assigned / commented / mentioned / pr-opened / pr-merged)

All fan-out is ordinary subscriber/assignee routing — no owner-hardcoded approver set, no agent special-casing:

- **Assigned** — `fireAndForgetAssignmentNotify` (`notifications.ts:162`) is unchanged in shape: targets the new assignee, writes `issue_assigned`, now emails too.
- **Commented / mentioned** — `fireAndForgetCommentNotify` (`notifications.ts:247`) is unchanged: mentioned users get `issue_mention`, other subscribers get `issue_comment` (mention wins), now with email.
- **Status changed** — `fireAndForgetStatusChangeNotify` (`notifications.ts:301`) fans `issue_status_changed` to subscribers, now with email.
- **Created** — creator auto-subscribes; assignment/mention at create time already fan out via the callers in `issues.ts` (create at line ~186).
- **PR opened / merged** — new fan-out. The MCP `open_pr` tool records the PR (server-side GitHub App) and links it to the issue; add a `fireAndForgetPrNotify({ issueId, type: 'pr_opened' | 'pr_merged', actorUserId })` that fans to **assignee + active subscribers** (`subscriberRecipients`, which already filters `unsubscribed=false` and de-dupes, unioned with `issue.assigneeId`). Call it from the `open_pr` MCP path for `pr_opened`, and from `applyPrMergeState` (`pr-sync.ts`) for `pr_merged` — inside its existing idempotent open→merged guard so a phone user gets exactly one "it's merged" email. (The old `agent-plan.ts` that emitted these is deleted with the agent-core workstream; PR<->issue linking is now deterministic via the `exp/<IDENTIFIER>` branch name parsed on the GitHub webhook, plus the `open_pr` tool.)

**Delete `fireAndForgetAgentActionNotify` and `workspaceOwnerRecipients`** (`notifications.ts:194,212`) — the plan-review/question routing they served no longer exists. The comment router's note about "action-needed alerts go through fireAndForgetAgentActionNotify instead" (`comments.ts:102`) is removed with it.

**Keep a minimal bot-user filter.** The synthetic **desktop-agent** user is deleted (no `agent_registrations`, no `role=agent` device membership, no `expk_` keys). But the **widget helpdesk still owns one bot user per config** as the issue `creatorId` (`apps/web/src/lib/widget/widget-user.ts`) — that must stay out of the human fan-out. Keep a small "is this a system/bot user, skip it" filter in `deliver()` (today `withoutAgents` via `users.isAgent`), but reframe/rename it around the widget bot user specifically (e.g. `isBot`/`isSystem`), decoupled from the deleted desktop-agent identity. It only needs to drop the widget bot creator so it never receives its own notifications.

### 6.3 The away/phone killer flow — notification edges

This workstream owns only the **notification + email edges** of the north-star loop; the rest is cross-referenced:

1. **Issue arrives** — created, assigned, or a comment/mention. The `fireAndForget*Notify` callers fire from the tRPC mutations (`issues.ts` create ~186 / update assignment ~444 / status ~451, `comments.ts:104`).
2. **`deliver()` fans out in-app + push + email** (6.1). On a sleeping phone, **push + email** are what reach the user while away — the reason both must be free and reliable.
3. **Open on phone** — the push `data.issueId`/`identifier` (already carried, `notifications.ts:148`) and the email `actionUrl` both deep-link the native issue detail. Use the same `identifier` deep link for both.
4. **"Start on my desktop"** — from the phone, the user triggers a `start_session` command over the relay to their online desktop device; the desktop runs the host-side coding launcher (worktree + branch `exp/<IDENTIFIER>` + `claude --dangerously-skip-permissions` in a libghostty terminal) and begins publishing the PTY. This is the **remote-start-and-steer relay** workstream — NOT this one. A live `coding_sessions` row (synced) surfaces the "coding now" badge + Watch/Steer button.
5. **Watch / steer** — the live PTY stream is relay memory (viewer/steer presence is a relay frame, no DB table). This workstream contributes nothing to the stream; it only got the user to the phone.
6. **Review the diff & merge** — the read-only side-by-side PR diff (PR-review workstream). On merge, the GitHub webhook (or the self-host cron) calls `applyPrMergeState`, which flips `prState='merged'`, emits the `pr_merged` activity event, **and now fires `fireAndForgetPrNotify(type:'pr_merged')`** so the away user gets the in-app + push + **email** "it's merged" confirmation.

Deliverable for the flow: **every step that should reach an away phone also emails** — assignment, mention, comment, PR opened, PR merged. Verify each `fireAndForget*` path produces an email when the recipient has `emailEnabled`.

### 6.4 One-way helpdesk (external reporter → resolution email)

A widget reporter is an end user who is **not** a workspace member and has **no account, no inbox, no push**. v1 is one-way: they get a **resolution email** when their reported issue is closed. Model it thread-ready so a two-way reporter thread + public status page can be added later without a rewrite.

**Capture the reporter as a subscriber on submit.** `createWidgetSubmission()` (`apps/web/src/lib/widget/service.ts:113`) already parses `reporterEmail` (form field or host `identify()`; `submitFieldsSchema.email`, line 60) and writes it to `widget_submissions.reporterEmail`. The submit transaction (line 209) writes issue + attachment + `widget_submissions` and explicitly creates no subscriber. Change:

- **Add subscriber source `widget_reporter`** to `subscriberSourceValues` (`packages/db-schema/src/domain.ts:82`, alongside `creator`/`assignee`/`commenter`/`manual`/`mention`). Enum protocol: update `packages/domain-contract/contract.json` (`subscriberSource`) and run `bun run --filter @exp/domain-contract generate`. Greenfield schema, so no `ALTER TYPE` migration — it's part of the initial enum.
- **`issue_subscribers` gets a nullable external identity.** Today `userId` is `text NOT NULL → users.id` (`schema.ts:497`) and the unique is `(issueId, userId)`. For external reporters make `userId` **nullable** and add a nullable `email varchar(320)`; `widget_reporter` rows carry `email` and null `userId`, member rows keep `userId` and null `email`. Adjust the `(issueId, userId)` unique so it doesn't collapse multiple external reporters (chosen default: partial unique on `(issueId, userId) where userId is not null`, plus `(issueId, email) where email is not null`). This avoids minting throwaway `users` rows and keeps reporters out of member fan-out. Reflect the nullable `userId` + new `email` column across the native mirrors of the `issue_subscribers` shape (Zig `sync_manager.zig` specs, iOS/Android entity + DAO) — native clients tolerate the extra column, but the shape spec must include it.
- In the submit transaction, when `reporterEmail` is present, insert an `issue_subscribers` row: `source='widget_reporter'`, `unsubscribed=false`, `userId=null`, `email=reporterEmail`, `workspaceId=config.workspaceId`, `issueId`. Reporter identity precedence: form email wins; else the host `identify()` email if the widget forwards it (extend the widget/submit to forward an identified email when present). Reuse the already-persisted `widget_submissions.reporterEmail`.

**Send the resolution email on close.** When status transitions to `done` or `cancelled` (both "closed"; note the target `issue_status` also gains `duplicate` — treat `duplicate` as closed too for resolution purposes if desired), email external reporters:

- Hook the status-change path in `issues.ts` where `fireAndForgetStatusChangeNotify` fires (~451). Add `fireAndForgetReporterResolution({ issueId, toStatus })` in `notifications.ts` that guards on `toStatus ∈ {done, cancelled}`, loads external subscribers (`issue_subscribers.source='widget_reporter'`, `unsubscribed=false`, non-null `email`), and for each sends a **plain reporter-facing** email via a new `sendReporterResolutionEmail({ to, issueTitle, unsubscribeUrl })`. **No in-app/push rows for reporters** (no account). Copy: "Your report '{title}' has been resolved." **No internal metadata** — no assignee names, no page/UA/customData, none of the `buildWidgetDescription` block. Reporters must never see workspace-internal context (see 6.5).
- **Idempotency:** a reopen→re-close must not double-email. Chosen default: set-once per submission via `widget_submissions.resolvedNotifiedAt timestamptz`; skip if already set; do not clear on reopen (no re-notify on churn).
- **Thread-ready modeling** (shape only, not the feature): the durable reporter contact lives on `widget_submissions`; reserve a per-reporter reply token on the subscriber/submission row (unused in v1) so a future inbound-reply route can attach a reporter comment. Do **not** build inbound parsing, a public status page, or reporter auth now. (`widget_submissions` stays server-only per target schema §10.)

### 6.5 Dogfood widget + reporter-PII containment

Exponential embeds **its own** feedback widget on a public feedback workspace. This already exists — extend, don't rebuild:

- `apps/web/src/lib/bootstrap-cloud.ts` creates the public `feedback` workspace (`isPublic:true`, `publicWritePolicy:everyone`, slug `feedback`), the `Exponential App` widget config, and `ensureDogfoodProject()` links the dogfood project (gated behind `DOGFOOD_REPO`). `findDogfoodWidgetKey()` (`apps/web/src/lib/widget/dogfood.ts`) resolves the key for the in-app FeedbackButton.
- **Link a repository row, not `projects.githubRepo`.** Repositories are first-class in v2 (workspace-level, tRPC-managed, server-only — NOT a synced shape; `repositories` + `project_repositories` per target schema §10). Update `ensureDogfoodProject` to link the dogfood project to a **repository row** so the coding launcher can resolve a clone target and dogfood coding works end-to-end. Cross-ref the repositories workstream.
- **Reporter PII stays owner-only.** `widget_submissions` (reporter email / page / UA / customData) is server-only, read via the `widgets` tRPC router — keep it owner-visible only. The reporter resolution email (6.4) must render the clean template only, never the `buildWidgetDescription` metadata block.

### 6.6 Self-hosted email / relay optionality

- **Email degrades gracefully.** With neither `RESEND_API_KEY` nor `SMTP_HOST` set, `email.ts` no-ops with a log (existing contract) — in-app + push still work and the helpdesk resolution email is simply skipped. No fire-and-forget path throws on a self-host box without email. The web UI hides email-only affordances via `emailEnabled` (extend beyond forgot-password to the email-prefs panel).
- **Push is optional too:** `sendToUser` (`fcm.ts`) already no-ops when `PUSH_RELAY_URL` is unset. Keep that.
- **The remote-start/steer relay** (device presence + live PTY + `start_session`) is a separate service and workstream; its LAN-only / outbound-friendly self-host requirement is documented there. This workstream only guarantees the **notification edges** (the push/email that tell you to go steer) degrade cleanly when a self-hoster runs without email or without the relay.

### Definition of done

- [ ] `deliver()` fans out to a **third email channel** off the same deduped `deliveredIds` set; push and email are **no longer plan-gated** (`canUsePush`/`limits.push`/the `push` plan field removed from the delivery path; `pushed_at` stamped unconditionally).
- [ ] `notification_type` enum drops `agent_plan_review`/`agent_question` and adds `pr_opened`/`pr_merged`; `contract.json` + generated Swift/Kotlin refreshed; `PushType` trimmed (no `plan_awaiting_approval`/`agent_error`).
- [ ] `fireAndForgetAgentActionNotify` and `workspaceOwnerRecipients` deleted; no caller references them.
- [ ] `email.ts` supports Resend (cloud) **and** SMTP (self-host) with a graceful logged no-op fallback; `emailEnabled` reflects both; `SMTP_*` env documented in `.env.example` + CLAUDE.md; `sendNotificationEmail` added, auth emails unchanged.
- [ ] `user_notification_prefs` (server-only) + `notifications.updateEmailPrefs` + one-click unsubscribe route (`/api/email/unsubscribe`) shipped; missing-row defaults to email-on; digest pref + immediate-skip branch in place.
- [ ] Fan-out emails fire on assignment, mention, comment, **pr-opened**, and **pr-merged**; `pr_merged` wired inside `applyPrMergeState`'s idempotent guard; `pr_opened` wired from the MCP `open_pr` path.
- [ ] `subscriber_source` gains `widget_reporter` (enum + `contract.json` + generated constants); `issue_subscribers.userId` made nullable + nullable `email` column added, unique constraints adjusted, native shape mirrors updated.
- [ ] Widget submit records the reporter as an external `widget_reporter` subscriber (null `userId`, `email` set); closing a widget-sourced issue emails the reporter a clean resolution notice (no internal metadata leak), idempotent via `widget_submissions.resolvedNotifiedAt`; no in-app/push rows for reporters.
- [ ] Reporter thread-readiness reserved (durable contact + reply token) without building inbound / public-status.
- [ ] Dogfood widget still resolves via `findDogfoodWidgetKey`; dogfood project linked to a **repository row**; reporter PII stays owner-only.
- [ ] All paths no-op cleanly on a self-host instance with no email transport and no push relay; no fire-and-forget path throws.

---

## 7. GitHub, repositories & coding-first flow

This workstream makes repositories a first-class **workspace** entity, hard-wires the "no linked repo => can't code" rule so Exponential is coding-first, and ships a real syntax-highlighted read-only PR diff on every client while leaving the schema anchored for PR write-back later. GitHub stays **100% server-side** through the storage-free GitHub App (App JWT → JIT per-repo installation token). The only local dependencies for the coding flow are the `claude` CLI and `git` — **never `gh`, never a stored PAT**. The native "Start coding" launcher (Section 5) pulls its repo + a short-lived push token from the two tRPC procs defined here.

Greenfield schema: define these tables directly in `packages/db-schema/src/schema.ts`. No migration/backfill, no `projects.github_repo` fallback — that column does **not** exist in the target schema.

### 7a. Repositories as a first-class workspace entity (tRPC-managed, NOT synced)

Repositories are **server-only** — read over tRPC, never an Electric shape. They change rarely, are owner/admin-managed, and don't need live fan-out to every client, so they stay out of the 14 synced shapes. Native clients read them via tRPC on demand (repo picker, launcher resolution), not through the sync engine.

**New tables** (`packages/db-schema/src/schema.ts`):

- `repositories` — one row per connected GitHub repo, workspace-scoped:
  - `id uuid pk`, `workspaceId uuid → workspaces.id (cascade)`, `fullName text notnull` (`owner/name`), `defaultBranch text notnull default 'main'`, `private boolean notnull default false`, `installationId bigint` (nullable; mirror of the GitHub App installation that grants access, from `listInstallationRepos`), `sortOrder doublePrecision`, `archivedAt timestamptz`, `...timestamps`.
  - `unique().on(workspaceId, fullName)`; index on `workspaceId`.
- `projectRepositories` — the many-to-many join (composite PK), so one repo can back many projects and one project can list many repos:
  - `projectId uuid → projects.id (cascade)`, `repositoryId uuid → repositories.id (cascade)`, `isPrimary boolean notnull default false` (the default clone/PR target when a project links more than one repo).
  - `primaryKey({ columns: [projectId, repositoryId] })`; index on `repositoryId`; a partial unique index enforcing at most one `isPrimary = true` per project.
- Export `Repository` / `ProjectRepository` (`InferSelectModel`) types and `selectRepositorySchema` / `selectProjectRepositorySchema`. No `workspace_id` denormalization trigger is needed here — these tables aren't synced, so tRPC queries just join `project_repositories → projects → workspaces` for the workspace scope.

**tRPC — `repositories` router** (`apps/web/src/lib/trpc/repositories.ts`, mounted as `repositories` in `api/trpc/$.ts` alongside the existing routers):

- `list({ workspaceId })` — member-readable; returns the workspace's repos with their project links.
- `add({ workspaceId, fullName, defaultBranch, private, installationId })` — **owner/admin only**; validates the App is actually installed on that repo via `resolveRepoInstallationToken` (`github-app.ts`; non-null ⇒ installed) before persisting.
- `remove({ repositoryId })` — owner/admin; also clears its `project_repositories` links.
- `linkProject({ projectId, repositoryId, isPrimary })` / `unlinkProject({ projectId, repositoryId })` / `setPrimary({ projectId, repositoryId })` — owner/admin; `setPrimary` flips the partial-unique `isPrimary` within the project.
- `forIssue({ issueId })` — **the launcher's resolution proc**; session-gated, member-readable. Resolves the issue → project → primary `project_repositories` link (else the sole link, else `null`) and returns `{ repositoryId, fullName, defaultBranch } | null`. This is what the native "Start coding" launcher calls first to decide the clone target.
- `installationToken({ repositoryId })` — see 7b.

Reuse `integrations.github.repos` (backed by `listInstallationRepos` in `github-app.ts`) to populate the picker; the registry `add` mutation persists the chosen repo as a `repositories` row. The existing per-user repo cache in `integrations.ts` stays as-is.

**Repositories management UI:**

- **Web** (owner/admin): new workspace-settings section `apps/web/src/components/workspace/repositories-section.tsx`, parallel to `projects-section.tsx` and registered in the settings nav. Lists workspace repos; "Connect repository" opens the **existing** `GithubRepoPicker` (`apps/web/src/components/github-repo-picker.tsx`) — which already handles the App-not-configured / not-installed / searchable-list states and the inline install flow — and calls `repositories.add`. A per-project link editor (multi-select of workspace repos with a primary star) drives `linkProject`/`setPrimary`.
- **Native (iOS / macOS / Android / Linux)**: a read + link surface in workspace settings via the `repositories` tRPC procs (no Electric shape to add). Native clients do **not** carry the GitHub-App install flow — they link to web for install, mirroring the billing/calendar web-only pattern. Linux must reach web 1:1 per the parity mandate (list + per-project link).

### 7b. The JIT installation-token proc (native push, no gh, no stored secret)

`repositories.installationToken({ repositoryId })` — **session-gated** (the real signed-in user's session; there is no agent identity, no `expk_` key, no device gate). It:

1. Loads the `repositories` row, verifies the caller is a member of its workspace.
2. Mints a short-lived, repo-scoped token via `resolveRepoInstallationToken(fullName)` (`github-app.ts`) — the storage-free App JWT → installation-token path. GitHub caps these at ~1h; the App is granted only `contents` + `pull_requests`.
3. Returns `{ token, fullName, defaultBranch, expiresAt }`, or a typed error if the App isn't installed on that repo (the launcher surfaces "reconnect this repo").

The native launcher (Section 5) uses this to build a token-embedded remote — `https://x-access-token:<token>@github.com/owner/repo.git` — on the worktree so `git push` works with no `gh` and no personal credentials. The token is never persisted; it's fetched per session and expires. This **replaces** the deleted `companion.repoToken` / `agent.repoToken` proc entirely.

### 7c. The MCP `open_pr` tool (server opens the PR, links it to the issue)

The coding agent runs against the existing web MCP server (`/api/mcp`, Streamable-HTTP, `apps/web/src/lib/mcp/`), authenticated with the user's **personal** API key (Better Auth apikey) written into the worktree's `.mcp.json` by the launcher. The current MCP toolset already exposes issues/projects/comments/labels; this workstream extends it with the coding-flow tools and **removes the agent-plan / agent-report tools** (`exponential_agent_plan_*`, `exponential_agent_open_pr`, `exponential_agent_report_pr`, `exponential_agent_report_error`) that belonged to the deleted headless runtime.

Target tools in `apps/web/src/lib/mcp/tools.ts` (naming aligned to the existing `exponential_*_*` convention; the agent-facing names are `get_issue` / `get_comments` / `update_status` / `open_pr` / `add_comment`):

- `get_issue({ issueId | identifier })`, `get_comments({ issueId })`, `add_comment({ issueId, body })` — read/write context (mostly present today; ensure they resolve by human identifier too).
- `update_status({ issueId, status })` — restricted to `in_progress` / `in_review` for the coding flow (`in_review` maps to the `issue_status` value used for "PR open, awaiting review").
- `open_pr({ issueId, title, body, head?, base? })` — **the server** opens the PR via the GitHub App:
  1. Resolves the issue's repo through the same `forIssue` logic (primary `project_repositories` link).
  2. `head` defaults to the launcher's branch `exp/<ISSUE-IDENTIFIER>`; `base` defaults to the repo `defaultBranch`.
  3. Mints an installation token (`resolveRepoInstallationToken`) and calls `createPullRequest` (`github-pr.ts`).
  4. Writes the PR linkage onto the issue in one transaction — `prUrl`, `prNumber`, `prState = 'open'`, `branch = head` — and records a `pr_opened` issue event (`recordIssueEvent`), which fans out the pr-opened notification.

PR ↔ issue linking is **deterministic two ways**: `open_pr` records it directly, and the merge webhook independently parses the `exp/<IDENTIFIER>` branch name (see 7e), so a PR opened out-of-band still links.

### 7d. Coding-first: no linked repo ⇒ "Start coding" disabled

The coding-first funnel is a single rule enforced everywhere the launcher can start: **an issue whose project has no linked repository cannot start a coding session.**

- The native + web "Start coding" (play/CLI) button resolves through `repositories.forIssue({ issueId })`. `null` → the button renders **disabled** with a "Link a repository" CTA that deep-links to the workspace **Repositories** settings section (web) or the native equivalent.
- The remote-start path (phone → relay → desktop, Section 8) performs the same `forIssue` check before spawning; a missing repo returns a "link a repository" error to the phone instead of starting.
- There is no `needs_human` status, no `repo_not_linked` error code, no agent-panel gate — those belonged to the deleted assignment-triggered runtime. The check is purely "does `forIssue` return a repo?" at launch time.

### 7e. PR review — read-only diff, schema-anchored for later write-back

**Keep the storage-free serving path unchanged.** `issues.prFiles` served via `fetchPullFiles` (`github-pr.ts`) over a JIT installation token stays exactly as-is. `pr-sync.ts` (`applyPrMergeState`) remains **merge-detection only** — it gains no review state. No new inbound surface beyond the existing optional webhook.

**Ship syntax-highlighted side-by-side on every platform.** Web's `diff-view.tsx` is today a single-column colored `<pre>`. Upgrade to real side-by-side:

- **Web (decided)**: a lightweight unified→split parser over `PullFile.patch` plus a token highlighter (Shiki or highlight.js keyed off file extension). Two gutters (old/new line numbers), aligned hunks, intra-line add/remove backgrounds. Keep the existing "no textual diff (binary/too large)" fallback and the `+adds/−dels` header.
- **Linux (the gap — priority)**: today only plaintext. Build a native GTK side-by-side diff widget in `apps/linux/src/ui/` reading the same `issues.prFiles` payload (fetched via the Linux tRPC client), with syntax highlighting (GtkSourceView language guessing by filename, or a hand-rolled tokenizer to hit web 1:1). Part of the Linux pixel-parity mandate.
- **iOS / macOS / Android**: side-by-side syntax-highlighted diff from the same `prFiles` payload; macOS keeps the glass aesthetic, the others match web.

**Anchor the model so write-back lands later without a rewrite (v1 = read-only, no review tables built yet).** Don't build review tables now, but lock the diff shape:

- Extend the `PullFile` return in `github-pr.ts` to also carry `sha` and `previousFilename` (both already in the GitHub files API) so line anchoring survives force-pushes and renames.
- The diff parser must key hunks by **`(filename, side, line)`** — `side ∈ {old, new}`, `line` = old/new line number — now, so inline-comment anchors already exist when review comments arrive.
- Future `pr_review_comments` (`issueId`, `prNumber`, `filename`, `side`, `line`, `body`, `authorId`, `githubCommentId?`, `state`) and `pr_reviews` (`event: approve|request_changes|comment`, `submittedBy`, `githubReviewId`) will POST to `/repos/{repo}/pulls/{n}/reviews` (and the review-comments API) using the **same** `resolveRepoInstallationToken` path — the auth is already correct and outbound-only. Note in-code that `pr-sync.ts` is intentionally merge-only today.

### 7f. Merge detection (webhook + self-host cron)

Unchanged in spirit, minus the deleted desktop poller:

- **Cloud**: the GitHub App webhook `/api/webhooks/github` on `pull_request` closed+merged → `applyPrMergeState` flips `prState = 'merged'`, stamps `prMergedAt`, and emits one idempotent `pr_merged` event. Match to the issue by exact `prUrl`, **and** (add) fall back to parsing the `exp/<IDENTIFIER>` head-branch name → issue, so merges of out-of-band-linked PRs still land.
- **Self-host behind NAT**: the outbound merge cron (`GITHUB_POLLING`) using `fetchPullState` remains the webhook-less path, calling the same `applyPrMergeState`.
- **Dropped**: the desktop `pr_poll` (it lived in the now-deleted Rust `agent-core`). Merge detection is server-side only.

### Definition of done

- [ ] `repositories` + `project_repositories` tables in `packages/db-schema/src/schema.ts` (greenfield — no migration/backfill, no `projects.github_repo` column) with `Repository`/`ProjectRepository` types + zod schemas.
- [ ] `repositories` tRPC router mounted (`list` / `add` / `remove` / `linkProject` / `unlinkProject` / `setPrimary` / `forIssue` / `installationToken`), owner/admin-gated on writes, session-gated on `installationToken`/`forIssue`. **Not** an Electric shape — still 14 synced shapes.
- [ ] Web Repositories settings section shipped (owner/admin) reusing `GithubRepoPicker`; native clients read + link repos via the `repositories` tRPC procs.
- [ ] `repositories.installationToken({ repositoryId })` returns a JIT App installation token the native launcher uses for a token-embedded push remote — no `gh`, no stored secret. (Replaces the deleted `companion.repoToken`.)
- [ ] MCP `open_pr` (+ `get_issue` / `get_comments` / `update_status` / `add_comment`) live in `tools.ts`; `open_pr` server-creates the PR via the GitHub App and writes `prUrl`/`prNumber`/`prState`/`branch` + a `pr_opened` event. The old `exponential_agent_plan_*` / `exponential_agent_*_pr` / `exponential_agent_report_error` tools removed.
- [ ] "Start coding" is disabled with a "Link a repository" CTA (deep-linking to Repositories settings) whenever `repositories.forIssue` returns `null`, on both local and remote-start paths.
- [ ] Syntax-highlighted side-by-side read-only diff shipped on web, Linux (was plaintext), iOS, macOS, Android — all reading `issues.prFiles`.
- [ ] Diff parser anchors hunks by `(filename, side, line)` and `PullFile` carries `sha`/`previousFilename`, so inline-comment + approve/request-changes write-back can be added later without reworking the diff or auth path.
- [ ] Merge detection is webhook + self-host cron only; the webhook links by `prUrl` **and** `exp/<IDENTIFIER>` branch parse. The desktop `pr_poll` is gone.

---

## 8. Billing moat, self-hosted parity & the cut list

This workstream is where the product's positioning becomes code: **simpler and cheaper than Linear**, self-hostable to full parity, and ruthless about what it refuses to build. It touches billing (web-only), the self-hosted gating of the features this refactor keeps, and — the headline of v2 — the mass deletion of the old agent runtime. The DELETE-NOW list below is the point: the codebase gets dramatically smaller and every remaining line is fully replaceable.

### 8a. The billing moat — workspace-flat-rate, value-based

**Decided model: flat rate per workspace, not per seat.** Keep the existing tier shape (`free` / `pro` / `business` / `unlimited`) defined in `apps/web/src/lib/billing.ts` (`PLAN_LIMITS`, `PlanTier`) and priced in `apps/web/src/components/workspace/plan-comparison.tsx` (`TIERS`: $18/yr Pro, $60/yr Business, annual-only, `FOUNDING` 50%-off code). This is already flat-per-workspace — the moat is to **keep it that way** and never drift into per-seat metering.

**Monetize on value, never on notifications.** Charge on the axes that scale with how much a team gets out of the coding superpower: **seats (member cap), projects, linked repositories, and concurrent coding sessions (capacity)**. Do **not** monetize on delivery reliability — email and push are **both free / table-stakes** on every tier. (There is no headless agent to meter anymore — the paid capacity axis is concurrent human-driven `coding_sessions`, i.e. how many worktrees/terminals a workspace can have running at once, if a cap is wanted at all.)

Concrete changes required in `PLAN_LIMITS` (`apps/web/src/lib/billing.ts`):

- The `free` tier currently has `push: false`; `pro`/`business`/`unlimited` have `push: true`. **Flip `push` on for every tier** (or remove the `push` limit entirely) — push is no longer paywalled. `getWorkspacePlan().limits.push` is consumed by `isPushEnabledForWorkspace` (billing.ts, returns `limits.push`) and by the notification path (`apps/web/src/lib/integrations/notifications.ts`, which gates a "plan-gated push"); both callers must stop treating push as paid.
- **Email (the delivery channel from §Email) is likewise never plan-gated.** Do not add an `email` boolean to `PlanLimits`.
- Add the real value axes instead: a `repositories` cap (per §Repositories) and, optionally, a concurrent-`coding_sessions` cap belong in `PlanLimits`. This is where new paid limits go — not on notifications.
- Update `plan-comparison.tsx` `TIERS` (`push` field on the `TierInfo` rows): drop the "Push notifications" `FeatureRow` gate — show it enabled on every tier or remove it as a baseline feature — and surface the real differentiators (members, projects, repositories, storage, concurrent coding sessions).

**Make limits non-opaque + nudge on hit.** The billing surface already shows usage bars (`WorkspaceBillingSection` → `UsageBar` for members/projects/storage in `billing-section.tsx`) and a full `PlanComparison`. Extend, don't rebuild:

- Every server-side limit throw in `billing.ts` (`assertCanCreateWorkspace`, the project-count guard, `assertStorageWithinLimit`, `assertWithinPlanLimits`, etc.) returns a `PRECONDITION_FAILED` (the tRPC code is `PRECONDITION_FAILED`, not `FAILED_PRECONDITION`) with a human message like "Your plan allows up to N …. Upgrade to …". **Standardize these** so the client catches them and renders an inline upgrade nudge (a small "Upgrade" CTA deep-linking to workspace settings → billing / `PlanComparison`) rather than a bare toast.
- In `plan-comparison.tsx`, ensure each tier row states **what unlocks** at the next tier (concrete numbers, not "contact us"). The moat is that a user always sees exactly what they get and what the next dollar buys.
- Add a repositories usage bar to `WorkspaceBillingSection` once the repositories entity lands (§Repositories), reading a new `usage.repositories` from `getWorkspaceUsage`.

**Billing stays strictly WEB-ONLY.** No native client (iOS / Android / macOS / Linux) shows any billing UI — store-policy safe. The `billingRouter` (`apps/web/src/lib/trpc/billing.ts`) and Creem checkout/portal routes (`/api/auth/creem/*`) are web-only by construction. Native clients that hit a paid limit link to the web app; they never render `PlanComparison`.

### 8b. Self-hosted parity — every kept feature has a self-hosted path

**Rule: self-hosted fully supports every feature.** Billing is the *only* thing that degrades on self-host — and it degrades to *unlimited*, not to disabled. The single gate is `process.env.SELF_HOSTED !== 'true'` via `isCloudInstance()` (`apps/web/src/lib/bootstrap-cloud.ts`); when self-hosted, `getUserPlan` / `getWorkspacePlan` (`billing.ts`) early-return `plan: 'unlimited'` with `Infinity`/max limits, `billingRouter.workspacePlan` / `.userPlan` short-circuit, and `buildRuntimeConfig()` (`apps/web/src/lib/runtime-config.ts`) nulls the Creem product IDs so no checkout UI renders.

The features this refactor keeps each need an explicit self-hosted path:

- **Repositories + coding flow (§Repositories, §Coding flow):** the workspace repo registry + GitHub App integration is storage-free and outbound-only (`apps/web/src/lib/integrations/github-app.ts` mints per-repo installation tokens JIT). The new desktop launcher pulls a JIT installation token from a session-gated tRPC proc (`repositories.installationToken`) and pushes with a token-embedded remote URL — **no `gh`, no personal creds**. Self-hosted works unchanged; it just needs a GitHub App configured via the existing `GITHUB_APP_ID` / `GITHUB_APP_SLUG` / `GITHUB_APP_PRIVATE_KEY` / `GITHUB_WEBHOOK_SECRET` env vars. Merge detection on self-host uses the outbound cron (`GITHUB_POLLING=true`) since the inbound webhook may be NAT-blocked. PR↔issue linking is deterministic via the `exp/<IDENTIFIER>` branch name. **No repositories cap on self-host** (unlimited plan).
- **Email delivery (§Email):** degrades gracefully. Cloud uses Resend (`RESEND_API_KEY` / `EMAIL_FROM`). Self-hosted email is **optional** — configured via SMTP env (add e.g. `SMTP_HOST` / `SMTP_PORT` / `SMTP_USER` / `SMTP_PASS` / `EMAIL_FROM`) **or left unset**, in which case the email channel is simply skipped and in-app + push still work. The sender probes config at startup and no-ops (logs, never throws) when neither Resend nor SMTP is configured — the same fire-and-forget discipline the notification path already uses. Per-user email prefs + unsubscribe tokens (server-only `user_notification_prefs` / `email_deliveries`) work on self-host when email is configured.
- **Remote-start + steer relay (§Remote-steer):** the relay is **LAN-only / outbound-friendly** — mirror the push-relay + GitHub-App outbound-only insight. While the desktop app is open it holds an *outbound* control connection to the relay (device presence); the phone's "Start on my desktop" and viewer connections both go through the relay; nothing requires an inbound port on the desktop. Steer claim + viewer presence live in **relay memory** (a relay frame), never a DB table. Self-hosted deploys the relay alongside the stack (like the standalone `apps/push-relay`), pointed at via `STEER_RELAY_URL` + shared `STEER_RELAY_SECRET` (mirror `PUSH_RELAY_URL` / `PUSH_RELAY_SECRET`). If unset on self-host, remote-steer degrades to "watch/steer on the desktop only" — coding still works locally.
- **Feedback widget dogfood** already has both paths (`runtime-config.ts`): cloud resolves the dogfood `expw_` key from the DB (`findDogfoodWidgetKey`), self-hosted points at the cloud via `FEEDBACK_WIDGET_SCRIPT_URL` + `FEEDBACK_WIDGET_KEY`, or runs its own config. No change needed — just don't regress it.

**Enumerated `SELF_HOSTED` gating touch-points** (verify each still holds after this refactor):

- `apps/web/src/lib/bootstrap-cloud.ts` — `isCloudInstance()` (`SELF_HOSTED !== 'true'`), the canonical gate.
- `apps/web/src/lib/bootstrap-self-hosted.ts` — self-host bootstrap (feedback widget, etc.).
- `apps/web/src/lib/runtime-config.ts` — `buildRuntimeConfig()` nulls Creem IDs + resolves the feedback widget per deployment.
- `apps/web/src/lib/billing.ts` — every `getUserPlan` / `getWorkspacePlan` / `assert*` early-returns `unlimited` when not cloud.
- `apps/web/src/lib/trpc/billing.ts` — `workspacePlan` / `userPlan` short-circuit when `!isCloudInstance()`.
- New: the email sender's config probe (Resend vs SMTP vs none) and the relay URL resolution both join this list once built.

### 8c. The cut list — the competitive edge (NEVER build)

**The refusal to build these IS the moat.** Every item below is something Linear (or a Linear clone) has; each is deliberately *not* in Exponential because simplicity is the product. Do **not** build, and **remove any half-present remnants** of:

- Kanban drag-drop board (there is board-adjacent test code — audit and remove any board view; the issue list is grid-only per the existing UX conventions)
- Saved filters / custom saved views (keep only the fixed tab presets in `apps/web/src/lib/filters.ts` — all/active/backlog; no "save this filter")
- Cycles / sprints
- Sub-issues / dependencies (already cut — keep it cut; no parent/child, no dependency graph)
- Time tracking
- Estimates / story points
- Custom fields
- Bulk edit (multi-select-and-mutate)
- Issue templates
- Agent marketplace
- MCP server browser (the coding flow writes a fixed `.mcp.json`; no in-app MCP discovery UI)
- Presence / typing indicators
- Timeline / Gantt
- Public roadmap share
- Linear import

### 8d. DELETE-NOW — the v2 amputation

This is a **greenfield rebuild with no production data to preserve**, so these come out wholesale — no migrations, no backfill, no "keep the column for one release." The new coding flow is a host-side native launcher (play/CLI button → resolve repo → JIT token → worktree + `exp/<IDENTIFIER>` branch → `.mcp.json` → `claude --dangerously-skip-permissions` in an embedded libghostty terminal), driven entirely by the **real user's** session and the web `/api/mcp` toolset. Everything below existed to serve the old headless/assignment-triggered Rust runtime and is now dead weight:

- **The entire Rust agent core.** Delete the `crates/agent-core/` crate wholesale (~4,860 LOC: dispatcher, pipeline, `agent_run`, state, the Rust Electric client, the Rust MCP client, `pr_poll`, the `agent_core_*` C ABI in `include/agent_core.h`, `run_request` ↔ `submit_run_result`, `InteractiveSlot` / any slot pool). It is not a bun workspace; remove it from the Cargo build and any packaging steps.
- **Both host FFI bridges.** Delete `apps/linux/src/core/agent/*` (`agent_core_ffi.zig`, `agent_manager.zig`, `heartbeat.zig`, `identity_store.zig`, `registration.zig`) and `apps/ios/ExponentialMac/MacAgentCore.swift` plus the agent bits of `MacGhosttyApp.swift`. **KEEP the libghostty terminal embedding** — the Linux `GtkGLArea` + GL shim and the macOS prebuilt `GhosttyKit.xcframework` are separate native code, not part of the deleted Rust core, and the new launcher spawns `claude` into exactly that terminal.
- **The device/agent identity + registration stack.** Delete the `companion.*` / `agent.*` tRPC router entirely (`apps/web/src/lib/trpc/companion/` — `hub.ts` / `identity.ts` / `index.ts` / `setup.ts` / `shared.ts`: `register` / `heartbeat` / `pollControl` / `repoToken` / `setupStatus`) and both its mount lines in `apps/web/src/routes/api/trpc/$.ts` (`agent: companionRouter`, `companion: companionRouter`). Drop the `agent_registrations` table, the `role=agent` membership concept (the device's `workspace_members` rows), and the `expk_` agent API keys. Remove `users.is_agent` **as a desktop-agent concept** (`packages/db-schema/src/auth-schema.ts`) — see the KEEP note below for the widget's separate reuse. The person coding is the real user under their own session; there is no synthetic desktop-agent user and no device registration.
- **The assigned-issues Electric shape + proxy.** Delete `apps/web/src/routes/api/shapes/assigned-issues.ts` and its collection wiring. The 14 synced shapes no longer include it; assignment no longer triggers any dispatch.
- **`agent_runs` + structured plan state + Plan Panels.** Drop the `agent_runs` table (`packages/db-schema/src/schema.ts`), the `issues.agentPlanState` column and its `agentPlanStateValues` enum (`domain.ts`), the whole `apps/web/src/lib/trpc/agent-plan.ts` router (`agentPlan.getState` and all approval/question procedures), its mount in `$.ts` (`agentPlan: agentPlanRouter`), and every native structured Plan Panel / plan-approval UI. There is no headless agent posting plan-ready/questions — **you watch the terminal**. Session state is replaced by the slim synced `coding_sessions` table (`id`, `issueId`, `workspaceId`, `userId`, `deviceLabel`, `status running|ended`, `startedAt`, `endedAt`) that only powers a "coding now" badge + Watch/Steer button. PR fields (`prUrl`, `prNumber`, `prState`, `branch`) live on `issues`.
- **The desktop PR poll.** Gone with the Rust core (`pr_poll` lived there). Merge detection stays 100% server-side: GitHub App webhook, plus the self-host `GITHUB_POLLING=true` cron.
- **Google Calendar — cut entirely.** Delete `apps/web/src/lib/integrations/google-calendar.ts`; the `fireAndForgetSync` / `fireAndForgetDelete` calls in `apps/web/src/lib/trpc/issues.ts`; the `GOOGLE_CALENDAR_ENABLED` flag handling in `apps/web/src/lib/auth/config.ts` and `apps/web/src/lib/trpc/integrations.ts`; the Calendar connect UI in `apps/web/src/routes/_authenticated/account/integrations.tsx`; and the `issues.googleCalendarEventId` / `googleCalendarLastSyncedAt` / `googleCalendarLastSyncError` columns (`packages/db-schema/src/schema.ts`). Greenfield: just define the clean schema without them — no drop-column migration needed.

**Frame it as the win:** deleting the C-ABI handshake, the slot pool, the two FFI bridges, the whole Rust core, the companion router, four tables, and the calendar integration removes the single largest source of cross-language coupling in the repo. Multi-window becomes trivially native (each window = its own ghostty + its own `claude` child in its own worktree; no shared slot pool, no core). What's left is a plain TanStack Start app + `/api/mcp` + native terminal embedding.

### 8e. KEEPs — explicitly wanted, must survive the cut

- **Issue-to-issue linking** — clickable issue pills referencing other workspace issues inside descriptions/comments, resolved like `@mentions` (mirror `apps/web/src/lib/integrations/mentions.ts`).
- **Duplicate** — mark an issue as a duplicate of a canonical one via `issues.duplicateOfId` (self-FK) + the `duplicate` value in the `issue_status` enum.
- **My Issues** — cross-project view (assignee = me) with a sidebar entry on web + mobile.
- **The widget helpdesk creator user** — the one-way feedback widget still needs a per-widget system/bot user to own issues filed by external reporters (`issues.creator_id` cascades — **never delete it**). This reuses the `users.is_agent`/`role=agent` plumbing *for the widget only*; it is **completely separate from and unrelated to** the deleted desktop-agent identity above. When ripping out the desktop-agent concept, preserve exactly this minimal bot-user path.

These are the *counter-signal* to the cut list: Exponential cuts complexity but keeps the small number of high-leverage relational features (linking, duplicates, My Issues) users genuinely loved in Linear — plus the one bot user the helpdesk can't live without.

### Definition of done

- [ ] `PLAN_LIMITS` in `apps/web/src/lib/billing.ts` has push free on **all** tiers; no `email` limit added; `isPushEnabledForWorkspace` + the notification path no longer treat delivery as paid.
- [ ] Any paid axis (repositories and/or concurrent `coding_sessions`) lives in `PlanLimits`, not on notifications; `plan-comparison.tsx` reflects the real differentiators and drops the push paywall row.
- [ ] Limit-hit `PRECONDITION_FAILED` throws are standardized and the client renders an inline upgrade nudge deep-linking to `PlanComparison`; billing UI stays web-only (no native billing surface).
- [ ] Self-hosted returns `unlimited` for all plan checks; Creem IDs nulled off-cloud; email degrades gracefully (Resend / SMTP / none, never throws); relay is LAN-outbound-friendly with a graceful "watch-only" fallback; feedback-widget dogfood path unregressed.
- [ ] All `SELF_HOSTED` touch-points (bootstrap-cloud, bootstrap-self-hosted, runtime-config, billing lib + router, new email/relay probes) verified consistent.
- [ ] No cut-list feature (Kanban, saved filters, cycles, sub-issues/deps, time tracking, estimates, custom fields, bulk edit, templates, agent marketplace, MCP browser, presence, timeline/Gantt, roadmap share, Linear import) exists in the codebase.
- [ ] `crates/agent-core/` deleted wholesale; `apps/linux/src/core/agent/*` + `MacAgentCore.swift` + the agent bits of `MacGhosttyApp.swift` deleted; libghostty terminal embedding retained.
- [ ] `companion.*` / `agent.*` tRPC router + both `$.ts` mounts removed; `agent_registrations` table, `role=agent` membership, and `expk_` keys gone; `users.is_agent`-as-desktop-agent removed while the widget bot-user path is preserved.
- [ ] `assigned-issues` shape + proxy deleted; `agent_runs` table, `issues.agentPlanState` + `agentPlanStateValues`, `agent-plan.ts` (incl. `agentPlan.getState`) + its `$.ts` mount, and all structured Plan Panel UI removed; slim synced `coding_sessions` table added.
- [ ] Desktop `pr_poll` gone (server-side merge detection only); Google Calendar fully removed (lib, `fireAndForget*` calls, flag/env, integrations UI, three `issues.googleCalendar*` columns).
- [ ] Greenfield schema defined directly (no migrations/backfill); KEEPs preserved — issue linking, `duplicateOfId` + `duplicate` status, My Issues, and the widget helpdesk creator user.

---

## 9. Sequenced execution plan for Fable

This is the ordered Phase 0..10 plan that turns the v2 hard cuts (§1–§8) into a build sequence. The spine is: **delete first, then define the clean schema, then the server contract, then the desktop launcher, then the relay, then the desktop IDE, then the coordination clients, then notifications/helpdesk, then billing/self-host** — with a macOS/iOS real-hardware verify track interleaved from the moment its dependencies land. Every phase cites the authoritative section that specifies its work, states an acceptance gate, and names the platforms it touches.

Guiding invariants (from §1.B, do not regress in any phase): **14 synced Electric shapes** in lockstep across five clients (web/iOS/Android/macOS/Linux) with `snakeCamelMapper` on every collection; GitHub stays storage-free/server-side/outbound-only; libghostty embedding is KEPT (not part of the deleted Rust core); `claude` is always spawned `--dangerously-skip-permissions`; local deps are only `claude` + `git` (never `gh`); admin + billing are web-only; self-hosted supports every feature (relay optional/LAN-outbound, email optional). This is a **greenfield DB** — no migrations, no backfill, no keep-a-column; dev resets via `bun run backend:clear` + `bun run migrate` + manual `0001_triggers.sql`.

---

### Phase 0 — Delete first (clear the deck)

**Goal.** Amputate the entire v1 agent runtime in one wholesale pass so every later phase builds on a clean, dramatically smaller tree. This is the largest single deletion in the refactor and it unblocks the greenfield schema.

**Deliverables** (§1.A, §8d):
- Delete `crates/agent-core/` wholesale (~4,860 LOC: dispatcher, pipeline, `agent_run`, state, Rust Electric client, Rust MCP client, `pr_poll`, the `agent_core_*` C ABI in `include/agent_core.h`, `run_request` ↔ `submit_run_result`, `InteractiveSlot`/slot pool); remove it from the Cargo build + any packaging steps.
- Delete both host FFI bridges: Linux `apps/linux/src/core/agent/*` (`agent_core_ffi.zig`, `agent_manager.zig`, `heartbeat.zig`, `identity_store.zig`, `registration.zig`); macOS `apps/ios/ExponentialMac/MacAgentCore.swift`, `MacAgentService.swift`, `MacAgentPanel.swift`, `MacAgentRunMonitor.swift`, and the agent-wiring portions of `MacGhosttyApp.swift`. **KEEP** the libghostty terminal embedding (Linux `GtkGLArea` + GL shim `ghostty_ffi.zig`/`terminal.zig`; macOS prebuilt `GhosttyKit.xcframework`).
- Delete the device/agent identity stack: `apps/web/src/lib/trpc/companion/*` and both `$.ts` mounts (`agent:`, `companion:`); `users.is_agent`-as-desktop-agent, `role=agent`, `expk_` agent keys.
- Delete the structured-plan stack: `apps/web/src/lib/trpc/agent-plan.ts` + its `$.ts` mount; the assigned-issues shape + proxy (`apps/web/src/routes/api/shapes/assigned-issues.ts`) + its collection wiring; the native Plan Panels (`AgentPlanPanel.swift`, `AgentPlanPanel.kt` + `AgentPlanPanelViewModel.kt`).
- Delete Google Calendar entirely: `google-calendar.ts`, `fireAndForgetSync`/`fireAndForgetDelete` calls in `issues.ts`, the `GOOGLE_CALENDAR_ENABLED` handling in `auth/config.ts` + `integrations.ts`, the connect UI in `account/integrations.tsx`.
- Delete `fireAndForgetAgentActionNotify` + `workspaceOwnerRecipients` (§6.2) and the `comments.ts` note referencing them.
- Remove cut-list remnants (§8c): any Kanban/board view code, saved-filter machinery beyond the fixed tab presets in `filters.ts`.
- **KEEP** (do not sweep away with the above): the widget helpdesk bot-user path (`widget-user.ts`; `issues.creator_id` cascades) — clearly re-scoped to the widget only.

**Acceptance gate.** No `agent_core_*` symbol, `agent_core.h`, `run_request`/`submit_run_result`, `InteractiveSlot`, `companion.*`/`agent.*` router, `agent-plan.ts`, `assigned-issues`, or `google-calendar.ts` reference remains anywhere in the tree (grep-clean). `bun run typecheck` and the web build are green with the deletions in place (schema still references the doomed tables until Phase 1; if typecheck can't pass until then, land Phase 0 + Phase 1 as one deletion-and-redefine commit). Rust/Cargo build no longer includes agent-core; Linux + macOS still compile with only the libghostty embedding retained.

**Platforms.** All (web, Linux, macOS, iOS, Android) — deletions land per-client; native clients drop their Plan Panels + agent bridges.

---

### Phase 1 — Greenfield target schema (14 synced + server-only, one clean pass)

**Goal.** Define the clean v2 schema directly — no migration, no backfill, no `ALTER TYPE`. One fresh `schema.ts` + `domain.ts` + `contract.json` pass that migrates cleanly from an empty DB.

**Deliverables** (§2, §1.B.1):
- Define the **14 synced shapes** (§2.1): `workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, coding_sessions`. `coding_sessions` (§2.5) replaces `agent_runs`.
- `issues` (§2.4): keep `prUrl`/`prNumber`/`prState`/`branch`/`prMergedAt`; add `duplicateOfId` self-FK; **drop** all `googleCalendar*` + `agentPlanState`.
- Define **server-only** tables (§2.3, §2.6): `repositories`, `project_repositories` (partial-unique primary index), `user_notification_prefs`, `email_deliveries`, plus existing `widget_configs`/`widget_submissions`. No proxy, no collection, no native DAO for these.
- `issue_subscribers` (§2.7): `userId` nullable + nullable `email`; two partial unique indexes replace `unique(issueId,userId)`.
- Widget bot user retained with `onDelete: restrict` (§2.7).
- Enum set (§2.8): `+widget_reporter`, `+issue_status 'duplicate'`, new `coding_session_status(running|ended)`; **drop** `workspace_member_role 'agent'`, `notification_type` agent kinds (`agent_plan_review`/`agent_question`) and **add** `pr_opened`/`pr_merged`, drop agent-only `issue_event_type` kinds, delete `agentPlanState`/`run_mode`. Run `bun run --filter @exp/domain-contract generate`.
- Custom trigger `populate_coding_session_workspace_id` in `0001_triggers.sql` (mirrors `populate_issue_subscriber_workspace_id`).
- Run the **§2.2 lockstep** for `coding_sessions` and the new `issues`/`issue_subscribers` columns across all five clients: `collections.ts` (+`snakeCamelMapper`) + `/api/shapes/coding-sessions` proxy; Zig `sync_manager.zig` `specs[]` (keep the `"14 shapes"` test) + `migrations.zig` CREATE/self-heal ALTER + `tableColumnSet`; iOS/macOS `Entities.swift`/`SyncManager.swift`; Android `Entities.kt`/`Daos.kt`/`ExponentialDatabase.kt` (Room version bump)/`SyncManager.kt`; `packages/electric-protocol/fixtures`.

**Acceptance gate.** `bun run backend:clear && bun run migrate` succeeds from empty; `0001_triggers.sql` applies; the generated migration contains **no** `ALTER TYPE`/backfill blocks. Zig `"14 shapes"` test green; each native client compiles with the coding_sessions entity. `bun run typecheck` + `bun run test` green. CLAUDE.md updated to 14 synced shapes naming `coding_sessions`.

**Platforms.** All (schema + five-client lockstep).

---

### Phase 2 — Server contract: repositories tRPC + installation-token + MCP toolset + coding-first funnel

**Goal.** Stand up everything the launcher will call before there is a launcher: the repositories registry, the JIT push-token proc, and the extended web MCP toolset that the terminal-driven `claude` will use.

**Deliverables** (§7, §1.B.2):
- `repositories` tRPC router (§7a) mounted in `$.ts`: `list`/`add`/`remove`/`linkProject`/`unlinkProject`/`setPrimary` (owner/admin writes), `forIssue({issueId})` (session-gated resolution → `{repositoryId, fullName/owner+name, defaultBranch} | null`), reusing `integrations.github.repos` + `resolveRepoInstallationToken` (`github-app.ts`).
- `repositories.installationToken({repositoryId})` (§7b): session-gated JIT App installation token via `resolveRepoInstallationToken` — replaces the deleted `companion.repoToken`. Never persisted.
- Extend the web MCP server (§7c, `apps/web/src/lib/mcp/tools.ts`): `get_issue`, `get_comments`, `add_comment`, `update_status` (`in_progress`/`in_review`), `open_pr` (server opens PR via GitHub App `createPullRequest`, writes `prUrl`/`prNumber`/`prState='open'`/`branch`, records `pr_opened` event). **Remove** the old `exponential_agent_plan_*` / `exponential_agent_*_pr` / `exponential_agent_report_error` tools. Auth via the user's personal Better Auth apikey.
- Web Repositories settings section (`repositories-section.tsx`, owner/admin) reusing `GithubRepoPicker` (§7a).
- Merge detection (§7f): webhook + self-host cron (`GITHUB_POLLING`) → `applyPrMergeState`; link by exact `prUrl` **and** by parsing the `exp/<IDENTIFIER>` head branch.

**Acceptance gate.** `repositories.forIssue` returns a repo for a linked project and `null` otherwise; `installationToken` returns a working short-lived token (test a `git ls-remote` against a token-embedded URL). MCP `open_pr` opens a real PR against a test repo and writes the four `issues.*` PR fields + a `pr_opened` event. Still exactly 14 synced shapes (repositories are NOT synced). `bun run typecheck` green.

**Platforms.** Web (server + settings UI). Native repo read/link surfaces are deferred to Phase 7.

---

### Phase 3 — Native "Start coding" launcher + desktop settings

**Goal.** The core coding funnel: a play/CLI button on an issue runs the host-side launcher (no Rust core, no FFI) and spawns `claude` in an embedded ghostty terminal.

**Deliverables** (§4a, §4b, §5-launcher, §7d):
- The launcher, implemented twice (Zig + Swift), running the identical sequence (§4a): `repositories.forIssue` → `repositories.installationToken` → host-side git clone + worktree + `exp/<IDENTIFIER>` branch off `origin/<defaultBranch>` + token-embedded remote (`https://x-access-token:<token>@github.com/owner/name.git`, **no `gh`**) → write `.mcp.json` (web `/api/mcp` + personal apikey) → compose plan-first prefilled prompt → spawn `claude --dangerously-skip-permissions` (cwd = worktree) in an embedded ghostty terminal → insert `coding_sessions` row (`running` → `ended`). Linux: `git_worktree.zig` beside `preview_config.zig`; macOS: `GitWorktree.swift`.
- Desktop settings pane (§4b), JetBrains-SDK-style, both desktops (Linux `settings.zig`, macOS `MacSettingsView.swift`): Claude CLI path (+ `claude --version` doctor), workspace/repos root, branch prefix (`exp/`), personal API key management (mint once, config/keychain, written into `.mcp.json`) — replacing any deleted `expk_` concept with the real user's key.
- Coding-first gate (§7d): the play button is **disabled with a "Link a repository" CTA** whenever `forIssue` returns `null`; same check on the (later) remote-start path.

**Acceptance gate.** On Linux, pressing play on a linked issue clones/worktrees, writes `.mcp.json`, launches `claude --dangerously-skip-permissions` in ghostty, and Claude can call `open_pr` to open a real PR; a `coding_sessions` row goes `running`→`ended`. On an unlinked issue the button is disabled with the CTA. macOS launcher verification is deferred to the Phase 6 real-hardware track (build-green here).

**Platforms.** Linux (full), macOS (build-green; runtime-verified in Phase 6).

---

### Phase 4 — Relay: outbound control channel + steer data-plane

**Goal.** Ship the standalone steer relay so a phone can start a session on the desktop and watch/steer the live terminal.

**Deliverables** (§3):
- New `apps/steer-relay` (`@exp/steer-relay`, Hono/Bun, modeled on `apps/push-relay`): `/healthz`, per-IP token bucket, `MAX_BODY_BYTES`, Bun-native WebSocket hub. Two in-memory registries: **device presence** `(userId → {deviceLabel, controlSocket})` and **session rooms keyed by `sessionId == coding_sessions.id`** with one publisher, N viewers, and a ring-buffer replay. All steer state (presence + single-steerer claim) is relay memory — **no `remote_steer_sessions` table** (§3.4).
- Wire protocol (§3.2): control frames `hello/online/start_session/join/resize/input/presence/claim/release/kill/bye` + binary `0x01` output frames; drop-non-input backpressure + slow-consumer eviction.
- `steer` tRPC router (§3.5): `mintTicket` (session **or** personal apikey; workspace-permission-checked via `membership.ts` + `access.ts`; HS256 ticket signed with `STEER_RELAY_SECRET`), `killSession`, `config`. No claim procs (relay-memory).
- Native `SteerPublisher` on both desktops (§3.3): tees the ghostty PTY bytes → relay `0x01` frames; injects remote `input` into the **same PTY master write** as local keys; serves resync snapshots; honors `kill`; keyed by `sessionId`. Wire it into the Phase 3 launcher (attach on session start).
- Remote-start path end-to-end (§3.2): desktop holds an outbound control socket announcing presence; phone `start_session{issueId, deviceId?}` → relay → chosen desktop runs the Phase 3 launcher and starts publishing.
- Env + deploy (§3.6): `STEER_RELAY_URL`/`STEER_RELAY_SECRET`; `Dockerfile.steer-relay`; Coolify `exponential-steer-relay`; optional `docker-compose.yaml` service; `STEER_RELAY_URL` unset ⇒ subsystem cleanly off.

**Acceptance gate.** Desktop A runs a session; a second connection (web viewer stub) `join`s by `sessionId`, receives ring-buffer replay + live bytes, claims steer and injects keystrokes that reach `claude`; local user always types + "Take over" works. `start_session` from a stub client launches a session on the desktop. `STEER_RELAY_URL` unset disables everything without breaking local coding. `/healthz` green.

**Platforms.** Relay (new service), Linux (publisher, full), macOS (publisher build-green; verified Phase 6).

---

### Phase 5 — Desktop IDE: run configs, native multi-window, Linux parity + diff

**Goal.** Round out the Linux IDE to web 1:1 and make concurrency trivially native.

**Deliverables** (§4c, §4d, §4e):
- Run configs + play menu (§4c): extend the existing preview/run-target infra (`preview_config.zig`, `.exponential/config.json`) with a generic `command` target (`argv`/`cwd`/`env`, `command` added to `platformValues`, `commandTargetSchema` in `runTargetSchema`); fold `argv`/`cwd` into `commandSetHash` so the trust gate re-prompts. Host spawn (`run_launcher.zig`) into a terminal-dock tab; top-bar play menu grouping **Start coding** + **Run configs** with per-repo last-selected memory + Stop. macOS mirror in `MacShell.swift`/`MacTerminalDock.swift`.
- Native multi-window (§4d): Linux terminal dock → `AdwTabView` keyed by `coding_sessions.id`/run-config id + detach-by-reparenting (never recreate a ghostty surface). macOS multiple `Window` scenes + tabbed dock. Concurrent sessions coexist (one ghostty + one `claude` + one worktree each) — **no shared slot pool**. Honor the nonzero-size + `ACTION_RENDER` ghostty gotchas.
- Linux 1:1 parity pass (§4e), incremental: (1) `exp-btn` CSS + token layer mirroring `styles.css`; (2) issue-row `grid-cols-[24px_72px_24px_1fr_auto]` fixed-column grid; (3) `GtkListView`/`GtkColumnView` virtualization replacing the materializing `gtk_list_box`; (4) `GtkSourceView` side-by-side syntax-highlighted diff replacing `diffFileWidget`'s plaintext labels (reads the same `issues.prFiles`; hunks anchored by `(filename, side, line)`).

**Acceptance gate.** Each parity sub-step verified against a side-by-side web screenshot. A command run config launches into a tab and records its exit code; the trust gate re-prompts when the command changes. Two coding sessions run concurrently in two tabs; a tab detaches into its own window without killing its `claude` child. Linux diff is side-by-side + syntax-highlighted, read-only.

**Platforms.** Linux (full), macOS (mirror; verified Phase 6).

---

### Phase 6 — macOS / iOS real-hardware verify-and-polish (interleaved track)

**Goal.** On a real Mac, verify everything that depends on macOS-native code now that its dependencies (Phases 1–5) have landed — **minus** all agent-core/FFI (deleted, nothing to verify). This phase runs in parallel with Phase 5 once Phase 3/4 land and gates before final sign-off.

**Deliverables** (§4f):
- Confirm `MacAgentCore`/`MacAgentService`/`MacAgentPanel`/`MacAgentRunMonitor` and the agent bits of `MacGhosttyApp` are removed (Phase 0 residue check).
- Verify on real hardware against `next.exponential.at`: login + all **14 Electric shapes** populate (incl. `coding_sessions`); CRUD round-trips via tRPC + `generateTxId`; the GFM markdown editor round-trips byte-identically incl. images/@mentions.
- Verify the **new launcher** (§4a) on macOS: repo resolve → JIT token → worktree + `exp/<IDENTIFIER>` + token remote → `.mcp.json` → `claude --dangerously-skip-permissions` in `MacGhosttyTerminal` → `open_pr` links a PR → `coding_sessions` row appears/ends.
- Verify run configs + play menu (§4c), multi-window reparenting (§4d), the `SteerPublisher` (§3), and libghostty render on a real display (honoring nonzero-size + `ACTION_RENDER`; never build from source — link the prebuilt xcframework).
- iOS: confirm build-green minus all deleted agent/FFI code; 14-shape sync + CRUD + editor verified.

**Acceptance gate.** All macOS runtime items above pass on a real Mac; iOS builds green and syncs 14 shapes. Glass aesthetic preserved. (Notarization stays a release-checklist item, not this phase.)

**Platforms.** macOS (real hardware), iOS (build + sync).

---

### Phase 7 — Coordination clients (web + iOS + Android)

**Goal.** Make the phone/web the coordination + remote-control surface: My Issues, Start-on-my-desktop, remote steer UI, PR diff, issue links + duplicate. No local terminal/CLI anywhere here.

**Deliverables** (§5):
- **My Issues** (§5a): cross-project `assigneeId == me` view + sidebar/tab entry on web/iOS/Android; grouped by status; rows → detail. No new column/shape.
- **Start on my desktop** (§5b): `relay.startSession({issueId, deviceId})` to the user's own online device; device picker for multiple; enabled/disabled + hint from live relay presence. No assignment, no agent user, no run endpoint. Lives in the repurposed `agent-panel.tsx` (→ session/steer panel), `IssueDetailView.swift`, `IssueDetailScreen.kt`.
- **Remote steer UI** (§5c): discover live session via synced `coding_sessions` (`status='running'`) + relay liveness → "coding now" badge + Watch/Steer. Web `<SteerTerminal>` (`steer-terminal.tsx`, xterm.js + addon-fit); iOS `SteerTerminalView.swift` + Android `SteerTerminalScreen.kt` lightweight VT viewers; `steer.mintTicket`; presence bar; claim gated by `WorkspacePermissions`.
- **Read-only PR diff** (§5d): syntax-highlighted side-by-side on web/iOS/Android from `issues.prFiles`; `PullFile`/`PrFile` carry write-back anchors (`path`/`sha`/`side`/`line`); preserve loading/empty/error/binary states.
- **Issue links + duplicate** (§5e): reference pills via extended `mentions.ts` (`extractIssueRefs`/`resolveIssueRefs`) + `#`-autocomplete (web); "Mark as duplicate" sets `duplicateOfId` + status `'duplicate'` atomically, with a canonical-issue banner + unmark, on all three clients.
- Repositories read/link native surfaces (§7a deferred from Phase 2): read + link via `repositories` tRPC (no shape); Linux reaches web 1:1; native clients link to web for the GitHub-App install flow.
- Confirm **no** process spawn / `claude`/`git`/`gh` / local PTY on web/iOS/Android (§5f).

**Acceptance gate.** My Issues lists cross-project assigned issues on all three clients. Start-on-my-desktop launches a real session on an online desktop and the phone joins the live terminal, watches, and steers while holding the claim. PR diff renders side-by-side + highlighted, read-only. Duplicate marking moves status to `'duplicate'` and shows the banner. Grep confirms zero local terminal/CLI in coordination clients.

**Platforms.** Web, iOS, Android (+ Linux for the native repo-link parity item).

---

### Phase 8 — Notifications, email & one-way helpdesk

**Goal.** Turn notifications into a three-channel (in-app + push + email) free delivery layer, wire PR-opened/merged fan-out, and ship the one-way reporter resolution email.

**Deliverables** (§6):
- Email primitive (§6.1): add email as a third leg in `deliver()` off the deduped `deliveredIds`; `email.ts` gains SMTP alongside Resend with graceful logged no-op (`emailEnabled = Boolean(RESEND_API_KEY || SMTP_HOST)`); `sendNotificationEmail` + deep-link `actionUrl`. **Un-gate push AND email from billing** (remove `push` from `PlanLimits`/`canUsePush`; `pushed_at` unconditional).
- `user_notification_prefs` (server-only) + `notifications.updateEmailPrefs` (web-only UI) + one-click `/api/email/unsubscribe` route; missing-row defaults to email-on; digest pref + immediate-skip branch (cron later).
- Normal fan-out (§6.2): assignment/comment/mention/status-change now email too; **new** `fireAndForgetPrNotify` — `pr_opened` from the MCP `open_pr` path, `pr_merged` inside `applyPrMergeState`'s idempotent guard. Keep the minimal widget-bot-user filter in `deliver()` (renamed `isBot`/`isSystem`, decoupled from the deleted desktop-agent identity).
- One-way helpdesk (§6.4): widget submit records a `widget_reporter` subscriber (null `userId`, `email` set); on close (`done`/`cancelled`) `fireAndForgetReporterResolution` sends a clean `sendReporterResolutionEmail` (no internal metadata); idempotent via `widget_submissions.resolvedNotifiedAt`; thread-ready reply token reserved.
- Dogfood widget (§6.5): link the dogfood project to a **repository row** (not `projects.githubRepo`); reporter PII stays owner-only.
- Self-host optionality (§6.6): email/push/relay all degrade cleanly; no fire-and-forget path throws.

**Acceptance gate.** Assignment/mention/comment/pr-opened/pr-merged each produce an in-app row + push + email when the recipient has `emailEnabled`. Closing a widget-sourced issue emails the reporter a clean notice exactly once (reopen→re-close does not re-email). Unsubscribe link flips `emailEnabled=false`. With no Resend/SMTP configured, email silently no-ops and in-app/push still work. `notification_type` enum has `pr_opened`/`pr_merged`, no agent kinds.

**Platforms.** Web (server + prefs UI); native clients receive push + in-app as before (no native email-prefs UI).

---

### Phase 9 — Billing moat & self-hosted parity finalize

**Goal.** Lock the cheaper/simpler billing positioning and verify every kept feature has a self-hosted path.

**Deliverables** (§8a, §8b):
- `PLAN_LIMITS` (§8a): push free on **all** tiers (flip/remove `push`); no `email` limit; move paid axes to real value (repositories cap and/or concurrent `coding_sessions`). `plan-comparison.tsx` drops the push paywall row and surfaces real differentiators (members, projects, repositories, storage, concurrent sessions). Add a repositories usage bar to `WorkspaceBillingSection`.
- Standardize limit-hit `PRECONDITION_FAILED` throws → inline client upgrade nudge deep-linking to `PlanComparison`. Billing stays web-only.
- Self-hosted parity sweep (§8b): confirm `SELF_HOSTED` returns `unlimited` for all plan checks; Creem IDs nulled off-cloud; email (Resend/SMTP/none) + relay (LAN-outbound, watch-only fallback) degrade cleanly; feedback dogfood unregressed. Verify all enumerated `SELF_HOSTED` touch-points (bootstrap-cloud/-self-hosted, runtime-config, billing lib + router, new email/relay probes).
- Final cut-list audit (§8c): none of Kanban/saved-filters/cycles/sub-issues/time-tracking/estimates/custom-fields/bulk-edit/templates/agent-marketplace/MCP-browser/presence/Gantt/roadmap-share/Linear-import exists.

**Acceptance gate.** Push + email work on the free tier. Hitting a plan limit renders an inline upgrade nudge. A self-hosted (`SELF_HOSTED=true`) instance runs the full coding flow (repo registry, JIT token, launcher, MCP `open_pr`, merge cron), email off cleanly if unconfigured, relay LAN-outbound or watch-only. Cut-list grep-clean.

**Platforms.** Web (billing), all (self-host parity verification).

---

### Phase 10 — Final green + release cut

**Goal.** Whole-tree green across all five clients and a clean release.

**Deliverables.**
- `bun run typecheck`, `bun run test`, `bun run test:e2e`, `bun run build` green; Zig `"14 shapes"` test green; Android builds; iOS + macOS build-green (macOS runtime-verified in Phase 6).
- CLAUDE.md + memory reflect: 14 synced shapes naming `coding_sessions`, the steer relay in the infra list, no agent-core/companion/calendar, the coding launcher flow, `STEER_RELAY_*` env.
- Release via the `release` / `release-staging` skills (tag, pushsync, GHCR build, Coolify deploy); apply `0001_triggers.sql` after migrate on each target.

**Acceptance gate.** All five clients build/sync 14 shapes; a full away→push/email→Start-on-my-desktop→watch/steer→review-diff→merge loop works end-to-end on staging; grep confirms zero Rust core / FFI / agent identity anywhere.

**Platforms.** All.

---

### How Fable should work this plan

- **Greenfield reset is expected and safe.** No production data — never write a migration, backfill, or "keep the column for one release." Define the clean schema directly; reset dev with `bun run backend:clear && bun run migrate`, then **manually apply `apps/web/src/db/out/custom/0001_triggers.sql`** (and any new trigger) after every fresh migrate — it is NOT auto-applied.
- **14-shape lockstep is non-negotiable.** Any synced shape/column change runs the full §2.2 checklist across all five clients in the **same change** (web collection + proxy with `snakeCamelMapper`, Zig `specs[]` + `"14 shapes"` test + SQLite migration + self-heal ALTER + `tableColumnSet`, iOS/macOS entity+DAO, Android entity+DAO+Room-version bump, electric-protocol fixture, CLAUDE.md count). Skipping a native mirror silently drops whole rows (gotcha a).
- **Delete before you build.** Phase 0 is a hard prerequisite; do not carry agent-core/FFI/companion/agent-plan/calendar forward "just in case." If typecheck can't pass with the tables still referenced, land Phase 0 + Phase 1 together.
- **Keep iOS/Android green throughout; verify macOS on a real Mac.** Native clients must at minimum compile after every phase that touches shared shapes; macOS runtime verification (Phase 6) needs real hardware and links the prebuilt `GhosttyKit.xcframework` (never build libghostty from source).
- **NO Rust core anywhere.** The coding flow is a host-side native launcher + `claude --dangerously-skip-permissions` in embedded libghostty + the web `/api/mcp` toolset. Local deps are only `claude` + `git` (never `gh`); GitHub stays 100% server-side (storage-free App, session-gated JIT token). Web/mobile never spawn a process — the only "start" is `relay.startSession`, the only terminal a remote relay mirror.
- **Respect the surviving gotchas** in every new path: tRPC code is `PRECONDITION_FAILED`; use `and()`/`or()` + `undefined`-to-skip in `useLiveQuery`; `snakeCamelMapper` on every collection; ghostty inits only at nonzero size + must handle `GHOSTTY_ACTION_RENDER`.
