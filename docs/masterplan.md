# Exponential — Masterplan

**The execution blueprint for the Fable 5 refactor.** Read [`vision.md`](./vision.md) first for the north star; this document is how we get there.

## Purpose & how to use this

This masterplan turns the vision into an ordered, verifiable refactor, written for a strong implementer (Fable 5) to execute top-to-bottom. **Section 1 is the do-not-regress contract.** Sections 2–8 are the detailed workstream specs. **Section 9 is the sequenced phase plan — start there for ordering, and treat Sections 1–8 as the spec each phase references.**

Every claim is grounded in real files under `apps/`, `packages/`, and `crates/`. Where the parallel drafters diverged, the **Editor's reconciliations** box below is authoritative and wins everywhere.

## Where we stand today

Exponential is **~70% of the way to the vision, and the remaining 30% is the load-bearing part.** It is already a genuinely good multi-platform Linear alternative: five clients (web, iOS, Android, macOS, Linux) sync the same fourteen Electric shapes, share a domain contract, and already run a working coding-agent pipeline (plan → approve → code → PR) driven by a frozen Rust `agent-core` C ABI, with libghostty terminals embedded on both desktops. Issues, comments, labels, the loved Inbox, subscriptions, @-mentions, the activity timeline, and a diff view are shipped. The GitHub App integration is storage-free and correct.

What separates today from the vision is concentrated and structural:

- **(a)** the IDE is half-built — agents launch, but there are no JetBrains-style run configs, no play button, and no multi-window;
- **(b)** the phone→desktop **remote-steer** loop does not exist at all (a greenfield transport — Electric syncs rows, it cannot stream a PTY);
- **(c)** **repositories** are a `text` column on `projects`, not a first-class workspace entity;
- **(d)** there is **no email channel** anywhere, which blocks both the away/phone killer flow and the helpdesk loop;
- **(e)** **Linux UI parity** is the biggest visual-quality gap (functionally complete, visually wrong).

The single largest carry-forward caveat: the **entire macOS app and much of the iOS Swift are build-green but runtime-unverified** — that track is verify-and-polish, not build-from-scratch.

## Locked decisions (this refactor)

| # | Decision | Choice |
|---|---|---|
| 1 | Remote agent control | **Full bidirectional** — watch + type into a live desktop agent terminal from web/mobile, via a new outbound relay (Electric can't stream a PTY) |
| 2 | Repositories | **First-class workspace entity, many-to-many with projects**; GitHub coding-first; no repo → agent marks `needs_human` |
| 3 | PR review | **Read-only syntax-highlighted diff on all platforms in v1**; schema anchored for later inline-comment / approve write-back |
| 4 | Multi-window | **v1 requirement** on both desktops; forces the core's global interactive slot → per-`runId` pool |
| 5 | Run configs | **JetBrains-style play button, host-side launch** (core stays agent-only); canonical config in git-committed `.exponential/config.json` |
| 6 | Google Calendar | **Cut entirely** (columns + lib + flag) |
| 7 | Helpdesk | **One-way resolution email** v1; reporter becomes a `widget_reporter` subscriber; modeled thread-ready |
| 8 | Email + push | **Both free / table-stakes**; monetize on agents / seats / repos / tier, never on notifications |
| 9 | Cut list | Kanban, saved filters, cycles, sub-issues/deps, time tracking, estimates, custom fields, bulk edit, templates, agent marketplace, MCP browser, presence, timeline/Gantt, roadmap share, Linear import — **all cut** |
| 10 | Keep | **Issue-to-issue linking** (clickable pills, resolved like @mentions) + **duplicate** (`status='duplicate'` + `duplicateOfId`) |
| 11 | My Issues | **First-class cross-project view** (assignee = me) on web + mobile |
| 12 | Notification routing | Agent action-needed alerts route to **assignee + subscribers** (owners only as fallback), not hardcoded owners |

## Editor's reconciliations (authoritative)

The eight workstreams were drafted in parallel and diverged on a few naming/scope points. These resolutions override any conflicting wording in the sections below:

1. **Synced shape count → 15 synced / 16 proxies.** Only **`repositories`** becomes a new synced Electric shape. `project_repositories` (join), `agent_run_history`, `remote_steer_sessions`, `user_notification_prefs`, and `email_deliveries` are **server-only, not synced** — clients read the active run via `agent_runs.currentRunId` (which rides the existing `agent_runs` shape). This overrides the "[SYNCED]" / 17-shape wording in §2 and the "own proxy" note on `agent_run_history` in §2.3. Promote `agent_run_history` or `project_repositories` to synced later only if a client genuinely needs them live.
2. **Duplicate = a dedicated `duplicate` status value + self-FK `issues.duplicateOfId`** (§2.7 wins over §5d's reuse-of-`cancelled`) — matches the user's "change status to duplicate" intent. The column name is **`duplicateOfId`** everywhere.
3. **`remote_steer_sessions` uses the §3.3 shape** — the `steer_perm` pg enum (`view|steer`) and §3.3 column names — overriding §2.4's varchar / alternate names. Still server-only.
4. **The reporter subscriber uses §6.4's model** — add a nullable `email` column to `issue_subscribers` and make `userId` nullable for `widget_reporter` rows (overrides §2.5's "only if `identify()` maps a known user"). The prefs table is **`user_notification_prefs`** (§6.1's name), superseding §2.6's `user_email_prefs`.
5. **Relay env is `STEER_RELAY_URL` / `STEER_RELAY_SECRET`** and the service is **`apps/steer-relay` (`@exp/steer-relay`)** — overriding §8b's `REMOTE_STEER_RELAY_URL` naming.
6. **`projects.github_repo` is kept for one release** as a read-through fallback (§7a) and dropped in **Phase 9** — it is NOT dropped in the Phase 1 migration.

## Table of contents

1. Architecture invariants to carry forward — *the do-not-regress contract*
2. Data-model & sync changes
3. Remote agent steering (the outbound relay subsystem)
4. Desktop IDE workstream (Linux + macOS)
5. Coordination clients workstream (web + iOS + Android)
6. Notifications, email & built-in helpdesk
7. GitHub, repositories & coding-first flow
8. Billing moat, self-hosted parity & the cut list
9. Sequenced execution plan for Fable

---

## 1. Architecture invariants to carry forward

This is the "do not regress" contract. The refactor **adds** remote-steer, repositories, run configs, email, issue-linking, and the CUT list — it does **not** rewrite the agent runtime, the sync topology, or the GitHub token model below. Every invariant here is load-bearing and already shipped; touching it without cause re-opens debugging that is already closed. Where a change is unavoidable (multi-window, the 15th+ shape), the exact seam and the mirror obligation are named.

### 1.1 The agent-core frozen C ABI (`agent_core.h`)

`crates/agent-core/include/agent_core.h` is a **frozen, hand-maintained C ABI**; the impl is `crates/agent-core/src/ffi.rs`. Both desktop hosts bind to it: macOS via a clang module map + thin Swift wrapper, Linux via hand-declared `extern` (Zig `translate-c` cannot parse the header's transitive deps — do **not** try to switch Linux to `@cImport` on the full header). The boundary is synchronous and thread-safe: every call returns immediately, all work runs on the core's background runtime, and results flow back **exclusively** through the single event callback.

**Rule: the Rust core is ONLY the agent loop.** The tracker data/sync layer is per-platform (macOS reuses the iOS Swift sync extracted into `ExpCore`; Linux has its own Zig sync engine under `apps/linux/src/core/`). Run configs (Decision 5) are **host-side and MUST NOT enter agent-core** — the desktop app spawns arbitrary build/test/dev commands directly into a terminal-dock tab, bypassing the core entirely. Do not add a `run_config` event vocabulary to the ABI.

Any change to a function signature, the event JSON shape, or an `agent_error` code is an ABI break and must be mirrored in **both** hosts plus `agent_core.h` in the same change. Frozen symbols:

- `agent_core_create` / `_set_event_callback` / `_start` / `_stop` / `_free` — lifecycle.
- `agent_core_claim_setup` / `_github_device_login` / `_uninstall` — setup/identity.
- `agent_core_submit_run_result` / `_cancel_run` / `_cancel_issue` — run bridge.
- `agent_core_request_interactive` / `_approve_interactive` — host-triggered sessions.
- `agent_core_string_free` — the ONLY way to release a `char**` out-param.

**Memory contract (will re-bite if broken):** strings passed to the event callback are **BORROWED** — valid only for the duration of the call; the host **must copy** them before returning. Strings returned via `char**` out-params are **owned by the caller** and released with `agent_core_string_free()`.

### 1.2 The `run_request` / `submit_run_result` handshake

The core **NEVER spawns the CLI for interactive runs.** For an interactive stage it emits a `run_request` event; the GUI launches `claude`/`codex` in a visible libghostty terminal and reports the exit via the **5-arg** `agent_core_submit_run_result(core, run_id, exit_code, final_text, session_id)`. `session_id` may be NULL (headless / core-pins-its-own via `--session-id`/`--resume`); `final_text` may be empty (plan/code results are verified out-of-band). Do not "simplify" this back to a 4-arg form — the 5th arg is what lets the host hand the CLI's session id back for resume.

**Headless runs execute IN-CORE and never reach the host** — no `run_request` for them. Only interactive runs cross the boundary.

Outbound event vocabulary (stable — extend, never rename):

- `run_request` — blocks the pipeline thread; fields `runId, issueId, issueIdentifier, cwd, mode(plan|code), program, argv[], env{}, mcpConfigPath, systemPrompt, userPrompt, interactive, continueSessionId`.
- `run_started` — `{issueId, issueIdentifier, runId, mode}`.
- `run_finished` — `{issueId, runId, exitCode, outcome: ok|failed|cancelled}`.
- `run_cancelled` — `{issueId?, runId}` → host tears down the matching terminal (destroying the surface kills the CLI child).
- `agent_error` — `{issueId, code, message}`.
- `log` — `{level, message}`.

**Stable `agent_error` codes** (native Plan Panels branch on these — do not rename): `repo_not_linked`, `repo_token_unavailable`, `plan_not_submitted`, `no_commits`, `pipeline_failed`, `interactive_failed`, plus the informational rejections `interactive_session_active` and `run_already_in_flight` (a trigger refused because a session/run is already live — **nothing failed**). Decision 2 (repos mandatory) reuses `repo_not_linked` for the "no linked repo → needs-human" path; do not invent a new code.

**Reference host bridge to mirror (do not reinvent):** `apps/linux/src/core/agent/agent_manager.zig` writes a per-run bash wrapper, runs it in the embedded terminal, submits the exit code, and tears down the terminal on `run_cancelled`. macOS mirror: `MacAgentRunner` / `MacAgentTerminalRunner`.

### 1.3 Host-runs-interactive vs headless-in-core, and the interactive slot

Today the core holds **one** process-global interactive slot: `run_pipeline::InteractiveSlot { owner: Mutex<Option<String>> }` in `crates/agent-core/src/run_pipeline.rs`, claimed before any slow I/O (MCP fetch, clone) so two near-simultaneous triggers can't both reach the worktree. Its doc comment asserts "both desktop hosts dock exactly ONE embedded terminal." **Decision 4 (multi-window / concurrent sessions) changes exactly this and nothing else in the runtime**: rework the single `InteractiveSlot` into a **per-session terminal-slot pool keyed by `runId`**. Invariants that survive the rework:

- The claim-before-slow-I/O ordering (prevents the worktree double-press race) stays — now per-slot.
- `interactive_session_active` becomes "this run's slot is busy," emitted only when a *specific* run's slot collides, never a blanket global refusal.
- `agent_core_approve_interactive` remains **resume-only** — it assumes the host already approved the plan **with the human's session** (the agent credential cannot self-approve). See gotcha (g).
- Concurrency respects `maxConcurrent` in `CoreConfigDto`.

### 1.4 Agent identity + registration

Agent identity is a **human-owned synthetic user**: `users.is_agent` (`packages/db-schema/src/auth-schema.ts`) + the `agent_registrations` table (`packages/db-schema/src/schema.ts`, `unique(owner_user_id, device_id)`, one row per physical desktop device). Registration is authorized by a **human session** through `companion.register`, now mounted as `agent.*` in `appRouter` (`apps/web/src/routes/api/trpc/$.ts`), with a **temporary `companion.*` alias to be dropped**. Registration mints a single long-lived `expk_` API key (Better Auth apiKey plugin) and fans the device user into every workspace the owner belongs to as `role=agent`.

**Assigning an issue to the desktop-agent user ENQUEUES it** (v1 = desktop-app-open only, no headless daemon). The owner's running desktop picks it up via the **web-only `assigned-issues` Electric proxy** (`/api/shapes/assigned-issues`, `expk_`-gated) + the core dispatcher. Do not add a headless/systemd mode in this refactor.

### 1.5 One issue = one PR = one branch/worktree; `agent_runs` is the plan-state shape

Each issue maps to exactly **one PR, one branch, one worktree.** All agent run state — plan / question / revision / approval / session / PR / error — lives in the **`agent_runs` table = the 14th synced Electric shape.** Native Plan Panels read this shape **directly** (not via an `agentPlan.getState` round-trip; that proc is a drainable fallback). Plan approval also touches `issues.agent_plan_state` (`varchar(32)`, `packages/db-schema/src/schema.ts`) with the run details in `agent_runs`; server logic in `apps/web/src/lib/trpc/agent-plan.ts`. Remote-steer (Decision 1) rides **alongside** this — the steer session model is a new relay concern; run state stays in `agent_runs`. Do not fold PTY-stream state into `agent_runs` (Electric rows cannot carry a live PTY stream — that is the whole reason the relay exists).

### 1.6 GitHub App storage-free token model + the 3 merge-detection triggers

GitHub is a **storage-free GitHub App**: App JWT (RS256, `iss = app id`) → per-repo **installation token JIT** (`resolveRepoInstallationToken` / `installationToken` in `apps/web/src/lib/integrations/github-app.ts`), bot `[bot]` identity, install via `/account/integrations`. The server opens PRs and serves diffs (`issues.prFiles`); the desktop fetches JIT repo tokens (`companion.repoToken`, agent-gated) **ONLY for git transport**. **All GitHub token traffic is OUTBOUND**; only the webhook is inbound + optional. Decision 2 (repositories first-class) changes the **clone-target resolution** (per-issue from the workspace repo registry instead of `project.github_repo`) but **must not** change this token model.

Merge detection is one idempotent function — `applyPrMergeState` (`apps/web/src/lib/integrations/pr-sync.ts`) — fed by **exactly three triggers**, all of which must keep working:

1. **Desktop `pr_poll`** — outbound, always-on while the app is open (`crates/agent-core/src/pr_poll.rs`).
2. **Cloud webhook** — `POST /api/webhooks/github`, HMAC-verified (`createHmac`/`timingSafeEqual`, `GITHUB_WEBHOOK_SECRET`).
3. **Self-host cron** — gated on `GITHUB_POLLING=true` (`apps/web/src/lib/bootstrap-self-hosted.ts`), decoupled from `SELF_HOSTED`.

### 1.7 Electric shape discipline — lockstep across five clients

Today: **14 synced shapes** (`workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, agent_runs`) + **1 web-only `assigned-issues` proxy** = **15 proxies**. This refactor adds shapes (repositories + the project↔repo join, and whatever remote-steer/run-config mirror rows are synced). **Every new synced shape or column must be mirrored in the SAME change across all of:**

- `CLAUDE.md` (the shape count + list).
- `apps/web/src/lib/collections.ts` (collection def, **`columnMapper: snakeCamelMapper()` is mandatory** — without it `useLiveQuery` `where` filters on camelCase silently fail) **+ a new proxy** under `apps/web/src/routes/api/shapes/` built with `createShapeRouteHandler`.
- Zig `apps/linux/src/core/electric/sync_manager.zig` `specs[]` (+ its `expectEqual` test).
- iOS/Android entity + DAO lists.
- `packages/electric-protocol/fixtures` + `packages/domain-contract` for cross-client alignment; when enum values change also run `bun run --filter @exp/domain-contract generate`.

### 1.8 The gotcha list (each has already bitten once)

- **(a) Native generic sync DROPS A WHOLE ROW** if the server adds a column the local table lacks. Guard with `apps/linux/src/core/db/database.zig`'s `tableColumnSet` + a migrations self-heal `ALTER` on **every** native client for **every** new column. This applies directly to the repositories / run-config / email-prefs columns this refactor adds.
- **(b) ghostty surface inits lazily only at NONZERO size** — mount the terminal only while the dock is expanded at a real height. Multi-window (Decision 4) must honor this per detached window.
- **(c) `GHOSTTY_ACTION_RENDER` must be handled** in the action callback (queue a redraw) or the terminal never paints.
- **(d) libghostty is NEVER built from source on macOS** — link the prebuilt `GhosttyKit.xcframework`; Linux uses the GL shim from the `douglas/ghostty` fork.
- **(e) tRPC error code is `PRECONDITION_FAILED`** (not `FAILED_PRECONDITION`).
- **(f) Custom SQL triggers are NOT auto-applied** — `apps/web/src/db/out/custom/0001_triggers.sql` (and `0002_public_workspace.sql`) must be run manually after a fresh DB. Any new trigger this refactor adds inherits the same manual-apply obligation and must be documented in the release checklist.
- **(g) "Approve & continue here" is a HUMAN action** — the agent credential cannot self-approve. The host approves with the **human session** and then calls `agent_core_approve_interactive` (resume only). Remote-steer permission tiers (view|steer) must preserve this: a steer viewer types into the terminal, but plan **approval** still routes through a human-session tRPC call, not the relay.

### 1.9 Platform-divergence and web-only rules (locked)

- **macOS keeps the glass aesthetic** (SwiftUI `.ultraThinMaterial`), aligning only the semantic status/priority tokens. Do **not** flatten it to match Linux.
- **Linux must reach web 1:1 (pixel parity)** — button sizes, spacing, fonts, row virtualization; this is the biggest open UI-quality gap and may require hand-rolling native GTK widgets. The syntax-highlighted side-by-side diff (Decision 3) lands under this bar, replacing Linux's current plaintext diff.
- **Admin console is WEB-ONLY.**
- **Billing (Creem) is WEB-ONLY** — native clients show no billing UI (store-policy safe). Note Decision 6 **cuts Google Calendar entirely**; do not carry any calendar invariant forward.
- **Self-hosted must fully support every feature** — including the remote-steer relay (LAN-only / outbound-friendly) and the email channel (SMTP or graceful degrade).

### 1.10 Verification debt (inherit as a track, not a footnote)

The **entire macOS app** (roadmap phases A1–A5, Phase F, UI-parity, device-preview) and much of the blind-written iOS Swift are "build + launch green but never exercised on a real Mac / display." Treat the macOS + iOS Swift work as a **verify-and-polish effort on real hardware**, not build-from-scratch. Any invariant above that is only asserted on macOS by inspection (the ABI binding, the terminal-slot pool, the glass tokens) must be **exercised on a Mac** before it counts as done.

### Definition of done

- [ ] `agent_core.h` and `ffi.rs` remain byte-frozen except the deliberate `InteractiveSlot` → per-`runId` slot-pool change; every remaining symbol/signature/event/`agent_error` code is unchanged and any change is mirrored in both hosts.
- [ ] `submit_run_result` stays 5-arg; borrowed-callback-string and owned-out-param memory contracts are still honored in both hosts.
- [ ] Interactive stays host-run, headless stays in-core; multi-window uses a per-`runId` slot pool with claim-before-slow-I/O preserved and `approve_interactive` still resume-only.
- [ ] Agent identity remains a human-owned `is_agent` synthetic user via `agent_registrations`; assignment-enqueues-via-`assigned-issues` is intact; `companion.*` alias removal is tracked (not silently kept).
- [ ] One-issue-one-PR-one-worktree holds; `agent_runs` is still the authoritative plan-state shape read directly by native Plan Panels; no PTY state leaked into it.
- [ ] GitHub App token model stays storage-free + outbound-only (JIT installation tokens); all 3 merge triggers still feed the idempotent `applyPrMergeState`; repo-registry change touches only clone-target resolution.
- [ ] Every new shape/column added by the refactor is mirrored in lockstep across the 8 sites in 1.7, with `snakeCamelMapper` and the `tableColumnSet` self-heal guard applied.
- [ ] All eight gotchas remain respected in new code paths (esp. (a) row-drop guard for new columns, (f) manual-trigger docs for any new trigger, (g) human-session approval under remote-steer).
- [ ] macOS-glass / Linux-1:1 divergence, admin-web-only, and billing-web-only rules are upheld; Google Calendar is fully removed; self-hosted parity (relay + email) is preserved.
- [ ] macOS/iOS invariants are re-verified on a real Mac, not assumed from a green build.

---

## 2. Data-model & sync changes

All schema lives in `packages/db-schema/src/schema.ts`; all enum value arrays live in `packages/db-schema/src/domain.ts` and are mirrored into `packages/domain-contract/contract.json` (regenerate Swift/Kotlin constants with `bun run --filter @exp/domain-contract generate`). Every new column or table below is defined against those files. Migrations are generated with `bun run migrate:generate && bun run migrate`; custom trigger SQL in `apps/web/src/db/out/custom/0001_triggers.sql` is **not** auto-applied and must be re-run after the migration (see the denormalization triggers below).

### 2.0 The lockstep checklist (applies to EVERY new synced shape or synced column)

This is the standing contract for any change in this section marked **[SYNCED]**. Skipping any step silently corrupts a client (see gotcha "row-drop on unknown column"):

1. **Schema** — add the table/column in `packages/db-schema/src/schema.ts`; add the `select…Schema` (and `create…Schema` where mutated) + `InferSelectModel` type export at the bottom of that file.
2. **Enum contract** — if the change adds/extends an enum, edit the `…Values` array in `domain.ts`, mirror it into `packages/domain-contract/contract.json`, and run `bun run --filter @exp/domain-contract generate`.
3. **Web collection** — add a `createCollection(electricCollectionOptions({…}))` block in `apps/web/src/lib/collections.ts` (import the new `select…Schema`, use `columnMapper: snakeCamelMapper()`, `parser: shapeParser`, `getKey`).
4. **Web shape proxy** — add `apps/web/src/routes/api/shapes/<name>.ts` built with `createShapeRouteHandler` (`apps/web/src/lib/shape-route.ts`), workspace-scoped `where`.
5. **Zig sync** — add a `ShapeSpec` entry to the `specs` array in `apps/linux/src/core/electric/sync_manager.zig` and bump the count assertion in the test `"shape registry: 14 shapes with matching tables"`; add the SQLite `CREATE TABLE` + column set in `apps/linux/src/core/db/migrations.zig`.
6. **Zig self-heal** — extend `tableColumnSet` handling in `apps/linux/src/core/db/database.zig` and add the idempotent `ALTER TABLE … ADD COLUMN` in `migrations.zig` so an older local DB self-heals rather than dropping whole rows.
7. **iOS/macOS** — add the entity in `apps/ios/ExpCore/Sources/DB/Entities.swift`, register it in `apps/ios/ExpCore/Sources/DB/DatabaseManager.swift`, and add the shape to `apps/ios/ExpCore/Sources/Electric/SyncManager.swift`.
8. **Android** — add the entity in `apps/android/app/src/main/java/com/exponential/app/data/db/Entities.kt`, the DAO in `.../data/db/Daos.kt`, register it in `.../data/db/ExponentialDatabase.kt` (bump the Room version), and add the shape to `.../data/electric/SyncManager.kt`.
9. **CLAUDE.md + memory** — bump the synced-shape count and the client-parity list.

Net shape count after this section: the current **14 synced shapes** become **17** (`+ repositories`, `+ project_repositories`, `+ agent_run_history`) and proxies go **15 → 18**. `run_configs`, `remote_steer_sessions`, `user_email_prefs`, and `email_deliveries` are **server-only, NOT synced** (justified per table below), so they do NOT touch steps 3–8.

---

### 2.1 Repositories as a first-class workspace entity (Decision 2) **[SYNCED]**

Two new tables plus a data migration off `projects.githubRepo`.

**`repositories`** — workspace-scoped, one row per linked GitHub repo:

```ts
export const repositories = pgTable(
  `repositories`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`).notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    // GitHub coordinates, split (was the packed `owner/name` TEXT on projects).
    owner: varchar({ length: 255 }).notNull(),
    name: varchar({ length: 255 }).notNull(),
    defaultBranch: varchar(`default_branch`, { length: 255 }).notNull().default(`main`),
    // Cached GitHub App installation id for JIT token resolution; nullable — the
    // App JWT can still look it up on demand (github-app.ts is storage-free).
    installationId: bigint(`installation_id`, { mode: `number` }),
    // Per-repo agent settings.
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

`mergeStrategy` uses a plain `varchar` with a documented value set (`squash`/`merge`/`rebase`) rather than a pg enum — it is web-settings-only display metadata and never needs cross-client enum constants. (If a native picker is later added, promote it to a `merge_strategy` enum via the step-2 flow.)

**`project_repositories`** — many-to-many join (a repo may back several projects; a project may span several repos):

```ts
export const projectRepositories = pgTable(
  `project_repositories`,
  {
    projectId: uuid(`project_id`).notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
    repositoryId: uuid(`repository_id`).notNull()
      .references(() => repositories.id, { onDelete: `cascade` }),
    workspaceId: uuid(`workspace_id`).notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }), // denormalized for the shape filter
    // Which repo the agent clones when an issue in this project has no explicit
    // override. Exactly one primary per project (partial unique index below).
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

Add a partial unique index (raw SQL in the migration, drizzle can't express it inline): `CREATE UNIQUE INDEX uniq_project_primary_repo ON project_repositories (project_id) WHERE is_primary;`.

**Data migration (in the generated migration file, after the two `CREATE TABLE`s):** for every `projects` row where `github_repo IS NOT NULL`, split on `/` → insert a `repositories` row `(workspaceId, owner, name)` (upsert on the unique constraint so shared repos dedupe), then insert a `project_repositories` row with `is_primary = true`. Then **drop `projects.github_repo`** (Decision 2 — it is replaced by the registry). Keep the drop in the same migration so no code reads the stale column.

**Agent clone-target resolution (Decision 2):** the dispatcher resolves the clone target per-issue as: issue's project → its `is_primary` `project_repositories` row → `repositories`. If a project has **no** linked repo, the agent must mark the issue `needs_human` and emit `agent_error` code `repo_not_linked` (already a stable code in the vocabulary) — do not fall back to `project.github_repo` (gone). This resolution replaces every current read of `projects.githubRepo` in `apps/web/src/lib/trpc/companion/` and `apps/web/src/lib/integrations/github-app.ts` / `pr-sync.ts`.

**Denormalization triggers:** `project_repositories.workspace_id` is denormalized from project→workspace; add a `populate_project_repository_workspace_id` trigger to `0001_triggers.sql` mirroring the existing `populate_issue_subscriber_workspace_id` pattern, so the Electric shape filter stays workspace-scoped (stable, no 409 churn).

Both tables are **[SYNCED]** — run the full 2.0 checklist for `repositories` (proxy `/api/shapes/repositories`) and `project_repositories` (proxy `/api/shapes/project-repositories`). Web repo-registry settings UI (workspace settings → Repositories) is the primary editor.

---

### 2.2 Run configs (Decision 5) — host-side canonical, DB display mirror only (**NOT synced**)

Run configs are **host-side**: the desktop app (Zig/Swift) spawns arbitrary build/test/dev commands directly into a terminal-dock tab, bypassing agent-core (honoring "core is ONLY the agent loop"). The canonical source is the committed **`.exponential/config.json`** working-tree file — the same file that already carries `ProjectPreviewConfig` (`packages/db-schema/src/domain.ts`). **Extend that file's schema rather than adding a new file.**

Add a `runConfigs` array to `ProjectPreviewConfig` in `domain.ts`:

```ts
export interface RunConfig {
  id: string            // stable key for last-selected memory + play-button menu
  name: string          // display label ("Dev server", "Unit tests")
  program: string       // argv[0]
  args?: string[]
  cwd?: string          // repo-relative; rejected if it contains ".."
  env?: Record<string, string>  // PATH/LD_PRELOAD/DYLD_* stripped host-side
}
export interface ProjectPreviewConfig {
  version: 1
  targets: RunTarget[]
  runConfigs?: RunConfig[]   // NEW
}
```
Add `runConfigSchema` + extend `projectPreviewConfigSchema` in `domain.ts`.

**DB mirror for cross-client display** — extend the existing display-only `ProjectPreviewMirror` (already on `projects.preview_config`, jsonb, never executed) rather than adding a table. This mirror is synced via the existing `projects` shape, so web/mobile can *show* the available run configs (and drive remote-launch RPC) without a new shape:

```ts
export interface ProjectPreviewMirror {
  targets: { id: string; name: string; platform: Platform }[]
  runConfigs?: { id: string; name: string }[]   // NEW — id+name only, never commands
  feedbackProjectId?: string
}
```
Update `projectPreviewMirrorSchema`. The desktop populates `runConfigs` into the mirror after it clones + parses `.exponential/config.json` (same flow that fills `targets`). **No new table, no new shape** — this rides the `projects` shape already in the checklist. Exit code + output history is captured host-side per run in the terminal-dock tab; it is not persisted server-side in v1.

---

### 2.3 `agent_run_history` — append-only per-run log (Decision 4) **[SYNCED]**

Today `agent_runs` is keyed by `issueId` (PRIMARY KEY on `issue_id`, verified `schema.ts:353-393`) = **current-state only, one row per issue**. Concurrent multi-window runs (Decision 4) and a per-`runId` terminal-slot pool need a stable run UUID and a history. Introduce an append-only sibling:

```ts
export const agentRunHistory = pgTable(
  `agent_run_history`,
  {
    id: uuidPk(), // the run UUID (the runId the desktop keys its terminal-slot pool by)
    issueId: uuid(`issue_id`).notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    workspaceId: uuid(`workspace_id`).notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }), // denormalized (trigger)
    // The desktop device that executed this run.
    hostDeviceId: text(`host_device_id`),
    mode: runModeEnum(`mode`), // background|interactive (reuse existing run_mode enum)
    status: varchar({ length: 32 }).notNull(), // queued|planning|awaiting_approval|coding|pushed|merged|cancelled|failed|needs_human
    sessionId: text(`session_id`),   // claude/codex session for --continue
    prUrl: text(`pr_url`),
    prNumber: integer(`pr_number`),
    lastError: text(`last_error`),
    startedAt: timestamp(`started_at`, { withTimezone: true }),
    finishedAt: timestamp(`finished_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_agent_run_history_issue`).on(table.issueId),
    index(`idx_agent_run_history_workspace`).on(table.workspaceId),
    index(`idx_agent_run_history_device`).on(table.hostDeviceId),
  ]
)
```

**Decided:** keep `agent_runs` as-is (the hot current-state row every Plan Panel reads directly) and add `agent_run_history` alongside it — do NOT re-key `agent_runs`. `agent_runs` gains one column, `currentRunId uuid` (FK → `agent_run_history.id`, `on delete set null`), so a client can jump from the live badge to the active history row. This is the minimal change that unblocks the per-session terminal-slot pool (agent-core keys slots by the history `id`) without rewriting the existing Plan Panel read path on four clients. `status` is a documented `varchar` (mirrors `agentPipeline.nonTerminalStatuses`/`reentryStatuses` in `contract.json`) rather than a new pg enum, matching how `agentPlanState` is already stored.

**[SYNCED]** — full 2.0 checklist; proxy `/api/shapes/agent-run-history`; workspace-scoped filter; add `populate_agent_run_history_workspace_id` trigger to `0001_triggers.sql`. The `currentRunId` column addition to `agent_runs` is also **[SYNCED]** (rides the existing `agent-runs` shape — just add the column + self-heal ALTER on native clients).

---

### 2.4 `remote_steer_sessions` — bidirectional remote steer (Decision 1) — **NOT synced**

Governs who is watching/steering a live desktop terminal session over the outbound relay. **Not synced via Electric** — the relay service (its own workstream) owns the live session lifecycle over its socket; this table is the durable session ledger + permission source, read/written via tRPC. Electric cannot carry a PTY stream, so it must not be the transport here.

```ts
export const remoteSteerSessions = pgTable(
  `remote_steer_sessions`,
  {
    id: uuidPk(),
    runId: uuid(`run_id`).references(() => agentRunHistory.id, { onDelete: `cascade` }),
    issueId: uuid(`issue_id`).notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    workspaceId: uuid(`workspace_id`).notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    hostDeviceId: text(`host_device_id`).notNull(), // desktop streaming frames
    viewerUserId: text(`viewer_user_id`).notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    permission: varchar({ length: 8 }).notNull().default(`view`), // view|steer
    claimedUntil: timestamp(`claimed_until`, { withTimezone: true }), // steer-claim window
    endedAt: timestamp(`ended_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_remote_steer_run`).on(table.runId),
    index(`idx_remote_steer_viewer`).on(table.viewerUserId),
    index(`idx_remote_steer_device`).on(table.hostDeviceId),
  ]
)
```

`permission` is a documented `varchar` value set `view|steer`. Enforcement: only one live row per `runId` may hold `permission='steer'` with `claimedUntil > now()` (a steer claim); others are `view`. The relay checks this table (or a cached projection) before forwarding an input RPC. Self-hosted: the relay is outbound/LAN-friendly; this table works identically regardless of relay reachability.

---

### 2.5 Helpdesk — one-way reporter resolution (Decisions 7 & 8)

**Reuse `issue_subscribers`** for the reporter (Decision 7) — add a new subscriber source. In `domain.ts`, `subscriberSourceValues` becomes:

```ts
export const subscriberSourceValues = [
  `creator`, `assignee`, `commenter`, `manual`, `mention`,
  `widget_reporter`, // NEW
] as const
```
Mirror into `contract.json` `subscriberSource.values` and regenerate constants. Because `subscriber_source` is a **pg enum** (`subscriberSourceEnum`, `schema.ts:93`), the migration must `ALTER TYPE subscriber_source ADD VALUE 'widget_reporter'` (drizzle emits this; it cannot run inside a transaction with other DDL on some PG setups — verify the generated migration splits it).

**Reporter email** already has a home: `widget_submissions.reporterEmail` (`schema.ts:603`). When a widget submission creates an issue, also insert an `issue_subscribers` row `source='widget_reporter'` for the reporter. But `issue_subscribers.userId` is a **non-null FK to `users`** — a widget reporter is not a member. Two options; **decided:** reuse the config's existing synthetic `widgetUserId` agent user as the subscriber `userId` is wrong (it's the creator, not the reporter). Instead, the resolution email is keyed off `widget_submissions.reporterEmail` directly (no `users` row needed for an external reporter), and the `issue_subscribers` `widget_reporter` row is only inserted **if** `identify()` maps the reporter to an existing workspace user. For the common external-reporter case, the email path reads `widget_submissions.reporterEmail` at close-time. This keeps the FK honest and still models the reporter as a subscriber when they happen to be a known user. (Future two-way threads can add a nullable-user reporter table without reshaping this.)

**Trigger (Decision 7):** when an issue tied to a `widget_submissions` row transitions to `done`/`cancelled` (a resolution), enqueue a resolution email to `reporterEmail` via the email-delivery path (2.6). One-way only in v1; the description-metadata block already carries context.

---

### 2.6 Email as a delivery channel (Decision 8) — **NOT synced**

Email is a **delivery channel**, not a notification type — do **not** add a `notification_type` value. In-app + push + email are the three fanned channels; email + push are free/table-stakes. Cloud uses Resend (`RESEND_API_KEY`/`EMAIL_FROM` already wired); self-hosted uses SMTP or degrades gracefully (no creds → email path is a no-op, logged).

**`user_email_prefs`** — per-user channel prefs + digest opt-in (server-only):

```ts
export const userEmailPrefs = pgTable(`user_email_prefs`, {
  userId: text(`user_id`).primaryKey()
    .references(() => users.id, { onDelete: `cascade` }),
  emailEnabled: boolean(`email_enabled`).notNull().default(true),
  digest: varchar({ length: 16 }).notNull().default(`off`), // off|daily|weekly
  // Stable per-user secret embedded in unsubscribe links (one-click list-unsubscribe).
  unsubscribeToken: varchar(`unsubscribe_token`, { length: 64 }).notNull().unique(),
  ...timestamps,
})
```

**`email_deliveries`** — audit + idempotency + per-message unsubscribe (server-only):

```ts
export const emailDeliveries = pgTable(
  `email_deliveries`,
  {
    id: uuidPk(),
    // Nullable: external widget reporters have no users row (2.5).
    userId: text(`user_id`).references(() => users.id, { onDelete: `cascade` }),
    toEmail: varchar(`to_email`, { length: 320 }).notNull(),
    // Ties a delivery to the notification/event that spawned it (idempotency key).
    notificationId: uuid(`notification_id`).references(() => notifications.id, { onDelete: `set null` }),
    issueId: uuid(`issue_id`).references(() => issues.id, { onDelete: `set null` }),
    kind: varchar({ length: 32 }).notNull(), // notification|digest|widget_resolution
    status: varchar({ length: 16 }).notNull().default(`queued`), // queued|sent|failed
    provider: varchar({ length: 16 }), // resend|smtp
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

Neither table is synced (delivery/audit concerns, not client state). The email fan-out hooks the existing notification pipeline (`apps/web/src/lib/integrations/notifications.ts` + `fcm.ts`): after a notification row is created, if the recipient's `user_email_prefs.emailEnabled` and `digest='off'`, enqueue an `email_deliveries` row and send. Digest modes batch. Unsubscribe route resolves `unsubscribeToken` → flips `emailEnabled=false`.

---

### 2.7 Issue-to-issue links + duplicate (Decision 10) **[SYNCED, partly]**

Two things wanted: (a) clickable **issue references** inside descriptions/comments, and (b) a **duplicate-of** resolution.

**(a) References — no new table.** Issue references are authored inline in the GFM markdown (mirroring the `@<email>` mention contract), so they round-trip as plain text and need **no schema change**. The interchange form is the issue identifier token `#MET-1153` (or `[[MET-1153]]`); the server resolves it against workspace issues at save time (a new `apps/web/src/lib/integrations/issue-refs.ts`, mirroring `mentions.ts`), fires no notification in v1 (out of scope) but records nothing extra. Clients render a known identifier as an issue pill — same rendering hook the `@email` pill uses. This keeps references inside the single markdown interchange contract with zero sync surface.

**(b) Duplicate — DECIDED: dedicated columns on `issues`, not a generic `issue_links` table.**

Add to `issues`:
```ts
duplicateOfIssueId: uuid(`duplicate_of_issue_id`).references(
  (): AnyPgColumn => issues.id, { onDelete: `set null` }
),
```
and add `duplicate` to `issueStatusValues` in `domain.ts` (→ `contract.json` `issueStatus.values` + `displayOrder`, regenerate; and `ALTER TYPE issue_status ADD VALUE 'duplicate'`).

**Justification (vs. a generic `issue_links` relation table):** Decision 9 explicitly cuts sub-issues/dependencies/relations — a generic link table is exactly the relation-graph surface we are removing, and it would re-open bulk relation UX. "Duplicate" is a *resolution* (a terminal-ish status pointing at one canonical issue), not a relation graph: it is 1:1 (`duplicateOfIssueId`), it changes the issue's lifecycle (hidden from active lists like `done`/`cancelled`), and it fits the existing `matchesFilters()`/status-group machinery for free. A `status='duplicate'` + self-FK is the minimal model, stays within the "simpler than Linear" moat, and both fields ride the **already-synced `issues` shape** — so the only per-client work is the self-heal ALTER for the new column and rendering the "Duplicate of MET-x" pill. Both changes are **[SYNCED]** on the existing `issues` shape (no new proxy); native clients need the `tableColumnSet` self-heal ALTER for `duplicate_of_issue_id` and enum-decode tolerance for the new `duplicate` status.

---

### 2.8 Cut Google Calendar (Decision 6) **[SYNCED — column removal]**

Drop from `issues` (`schema.ts:261-267`): `googleCalendarEventId`, `googleCalendarLastSyncedAt`, `googleCalendarLastSyncError`. Delete `apps/web/src/lib/integrations/google-calendar.ts` and every `fireAndForgetSync`/`fireAndForgetDelete` call in `apps/web/src/lib/trpc/issues.ts`; remove the `googleCalendarEnabled` flag + the Google-Calendar linking UI in `apps/web/src/routes/_authenticated/account/integrations.tsx` and the calendar branch of `apps/web/src/lib/trpc/integrations.ts`; drop the calendar scope from the Better Auth `linkSocial` config (`apps/web/src/lib/auth/config.ts`, `index.ts`). Note: **Google OAuth login stays** — only Calendar is cut. Update `GOOGLE_CALENDAR_ENABLED` references and the CLAUDE.md integrations section. Because these are synced columns on the `issues` shape, native clients must **not** choke on their disappearance — the generic sync + `tableColumnSet` self-heal already tolerates *extra* server columns; dropping columns the client still lists is safe (client just stops populating them), but remove the fields from iOS `Entities.swift` / Android `Entities.kt` in the same change to keep the mappers clean.

---

### 2.9 Enum additions — consolidated

| Enum (`domain.ts` array → `contract.json`) | Storage | Change |
| --- | --- | --- |
| `subscriberSourceValues` (`subscriber_source` **pg enum**) | pg enum | **+`widget_reporter`** — needs `ALTER TYPE … ADD VALUE` |
| `issueStatusValues` (`issue_status` **pg enum**) | pg enum | **+`duplicate`** — needs `ALTER TYPE … ADD VALUE` + add to `displayOrder` |
| `runModeValues` (`run_mode` pg enum) | pg enum | **reused unchanged** by `agent_run_history.mode` — no new value |
| `platformValues` | varchar (contract-only) | **NOT extended.** `platform` describes preview run-target device backends (`web`/`android`/`ios`), *not* the desktop host OS. Desktop host OS (`macos`/`linux`) is NOT modeled as a domain enum — it lives as free `hostDeviceId` text on `agent_run_history`/`remote_steer_sessions` and in `agent_registrations`. Do **not** add `macos`/`linux` to `platformValues`. |
| notification **delivery channel** | — | **NOT an enum in the DB / not a `notification_type`.** Modeled as `user_email_prefs` flags + the `email_deliveries.kind` varchar. Do not add a channel enum. |

`merge_strategy`, `agent_run_history.status`, `remote_steer_sessions.permission`, `user_email_prefs.digest`, `email_deliveries.kind`/`status` are all documented **varchars** (web/host-only display or logic), deliberately NOT pg enums — matching the existing precedent of `agentPlanState`/`agentPipeline` statuses which are varchars kept in sync via `contract.json` prose, avoiding an `ALTER TYPE` per tweak and cross-client constant regeneration for values no native picker consumes.

---

### Definition of done

- [ ] `repositories` + `project_repositories` tables added, `projects.github_repo` data migrated into rows then the column dropped; partial unique index on primary repo; `populate_project_repository_workspace_id` trigger in `0001_triggers.sql`.
- [ ] Both repo tables run the full 2.0 lockstep (collections.ts, `/api/shapes/repositories` + `/api/shapes/project-repositories`, Zig `specs`+test+migrations+self-heal, iOS/Android entity+DAO, CLAUDE.md); synced-shape count updated 14→17, proxies 15→18.
- [ ] Agent clone-target resolution reads the repo registry; unlinked project → `needs_human` + `agent_error:repo_not_linked`; no reads of `projects.githubRepo` remain.
- [ ] `.exponential/config.json` schema (`ProjectPreviewConfig`) extended with `runConfigs`; `ProjectPreviewMirror` + `projectPreviewMirrorSchema` extended with id/name-only `runConfigs`; no new table/shape for run configs.
- [ ] `agent_run_history` table added (run-UUID PK, append-only) + `agent_runs.currentRunId` FK; both synced; `agent_runs` current-state read path NOT re-keyed; workspace-id trigger added.
- [ ] `remote_steer_sessions`, `user_email_prefs`, `email_deliveries` added as **server-only** (not synced), with the single-steer-claim invariant and idempotent per-notification email delivery.
- [ ] `subscriber_source` gains `widget_reporter` (enum ALTER + contract + regenerate); widget resolution email path keyed off `widget_submissions.reporterEmail`; resolution enqueue on `done`/`cancelled`.
- [ ] Email delivery channel wired into the existing notification fan-out with per-user prefs + unsubscribe tokens; degrades gracefully with no Resend/SMTP creds.
- [ ] Issue references resolved inline in markdown (`issue-refs.ts`, no schema); `duplicate` status value + `issues.duplicateOfIssueId` added and synced on the `issues` shape (self-heal ALTER on native clients).
- [ ] Google Calendar columns dropped from `issues`; `google-calendar.ts` + all `fireAndForgetSync` calls + the flag + the linking UI removed; native `Entities` cleaned; Google login left intact.
- [ ] `contract.json` regenerated (`bun run --filter @exp/domain-contract generate`); `bun run typecheck` + `bun run test` green; migration applied and `0001_triggers.sql` re-run.

---

## 3. Remote agent steering (the outbound relay subsystem)

The killer flow: an issue arrives while I'm away, my agent-capable desktop is running at home, and I trigger clone→AI→PR **and fully steer it from my phone** — watching the live agent terminal and typing into it. This is the single biggest net-new subsystem in the refactor. Electric syncs rows and cannot carry a live PTY byte stream, so remote steering needs a **new standalone relay service** that the desktop connects to **outbound** and that web/mobile viewers subscribe to.

### 3.0 Why a new service, and why outbound-from-desktop

- **Electric can't carry the PTY.** Electric is a Postgres shape-log replicator: it delivers ordered row snapshots + change deltas. A terminal is a high-frequency, ephemeral, ordered byte stream (thousands of small frames/sec at their peak, worthless once consumed) with a **reverse input channel**. Persisting every frame as a Postgres row would trash the WAL and the shape log, and Electric has no viewer→writer RPC path. Terminal transport must be a separate, non-persisted, bidirectional channel. `agent_runs` (the 14th synced shape) stays the **control-plane** record (who's steering, session liveness, PR/plan state); the relay is the **data-plane** for bytes only.
- **Outbound-from-desktop is mandatory for self-hosted / NAT.** The desktop runs on a laptop at home behind NAT with no inbound reachability — exactly the constraint that already forced the push-relay (`apps/push-relay`) and the GitHub App (outbound App-JWT → installation token, only the webhook is inbound + optional) to be outbound-only. The desktop therefore **dials out** to the relay and holds a persistent socket; the relay fans bytes to viewers. Viewers (web/mobile) also dial the relay (a public/reachable host). Nothing ever connects *into* the desktop. Self-hosted deploys the relay next to the web app (or LAN-only); if no relay is configured, steering degrades gracefully to non-existent (see 3.6) while the agent still runs headlessly and syncs plan/PR state over Electric as today.

### 3.1 New service: `apps/steer-relay`

Model it **exactly** on `apps/push-relay` — standalone Hono/Bun, separately deployed, its own Coolify app and `Dockerfile.steer-relay` (context `.`). Same shape as `apps/push-relay/src/index.ts`: a lazy-singleton, `/healthz` unauth check for the Docker HEALTHCHECK, per-IP token-bucket rate limiting, `MAX_BODY_BYTES` guard, `PORT` from env. Add it as a bun workspace `@exp/steer-relay`.

Differences from push-relay: push-relay is a stateless request/response FCM forwarder; steer-relay is a **stateful WebSocket hub** (Bun's native `Bun.serve` `websocket` handler, not Hono's fetch-only path — export `{ port, fetch, websocket }`). It holds an in-memory registry of **rooms** keyed by `runId` (== `agent_runs.issueId`, since one issue = one run = one PR). Each room has:

- exactly one **publisher** socket (the desktop hosting that run),
- zero-or-more **viewer** sockets (web/mobile),
- a small **ring buffer** of the most recent N KB of terminal output (the "scrollback replay" so a viewer that connects mid-session sees current screen state, not a blank pane).

The relay is a **dumb pipe with auth + a session ledger**. It does not parse terminal escape codes, does not persist bytes, and holds no DB connection for the byte path. Session bookkeeping that must survive a relay restart lives in Postgres (`remote_steer_sessions`, 3.3) written by the web app over tRPC, not by the relay.

Env vars (mirror push-relay naming):
```
STEER_RELAY_URL        # public relay base (wss://steer.exponential.at) — desktop + clients dial this
STEER_RELAY_SECRET     # shared secret: web app ↔ relay for the token-mint/verify path
PORT                   # default 4002
```
Add `STEER_RELAY_URL` to the web env and to the desktop `CoreConfigDto`-adjacent host config (the relay URL is a **host-app** concern, not agent-core — see 3.4).

### 3.2 Transport & wire protocol

One WebSocket per participant. **Auth happens on connect**, before joining a room, via a short-lived **relay ticket** minted by the web app (never send raw `expk_`/session tokens to the relay). Framing: length-prefixed binary frames on the wire; the control channel is JSON, the terminal byte channel is raw binary to avoid base64 bloat.

**Connect handshake (both roles).** Client opens `wss://<relay>/v1/steer?ticket=<jwt>`. The ticket is a compact signed token minted by the web app tRPC (`steer.mintTicket`, `authedProcedure`) after it verifies the caller's session/`expk_` AND their permission (3.5). Ticket claims: `{ runId, workspaceId, userId, role: "publisher"|"viewer", perm: "view"|"steer", exp }`, signed with `STEER_RELAY_SECRET` (HS256). The relay verifies signature + `exp` only — all authorization was already decided by the web app at mint time. Publisher tickets are minted by the **desktop** calling `steer.mintTicket` with its `expk_` key (agent-gated, like `companion.repoToken`); it always gets `role: "publisher"`.

**Message types (JSON control frames, `{t, ...}`):**

| `t` | dir | payload | meaning |
| --- | --- | --- | --- |
| `hello` | pub→relay | `{runId, cols, rows, sessionId}` | desktop registers as publisher; relay creates/attaches room |
| `join` | view→relay | `{runId}` | viewer subscribes; relay replays ring buffer then live-tails |
| `resize` | pub→relay→view | `{cols, rows}` | terminal geometry changed (viewers reflow) |
| `input` | view→relay→pub | `{bytes}` (utf8) | keystrokes from a **steer**-perm viewer, injected into the PTY |
| `presence` | relay→all | `{viewers:[{userId,name,perm}], steererId}` | who is watching/steering (drives the UI avatars) |
| `claim` | view→relay→pub | `{until}` | request the exclusive steer token (see 3.3 claim model) |
| `release` | view→relay | `{}` | give up steer |
| `kill` | view→relay→pub | `{}` | **kill-switch**: owner force-terminates the session (3.5) |
| `bye` | pub→relay | `{outcome}` | session ended; relay closes room, evicts viewers |

**Terminal output** is a raw **binary** frame (opcode byte `0x01` + payload) pub→relay→all-viewers — the ghostty surface's byte output (3.4). Keeping it binary and out of the JSON envelope is what keeps the hot path cheap.

**Backpressure.** The desktop is the fast producer; a phone on cellular is the slow consumer. The relay MUST NOT buffer unboundedly per viewer. Policy: each viewer socket has a bounded send queue; on overflow the relay **coalesces** by dropping intermediate output frames and forcing a **full-screen resync** (the desktop, on a `resync` request the relay sends when it evicts a viewer's backlog, re-emits current screen state — ghostty can dump the full visible grid). Control frames (`input`, `claim`, `resize`, `kill`) are **never** dropped. Bun's `ws.send()` returns backpressure signals; when a viewer stays saturated past a timeout the relay drops that viewer with a `slow_consumer` close code (the client shows "reconnecting"). The publisher is never throttled by a slow viewer.

**Reconnect.** Viewers reconnect with a fresh ticket and re-`join` (ring-buffer replay covers the gap). If the **publisher** socket drops, the relay marks the room `stale` and starts a grace timer; the web app sees `agent_runs.interactiveClaimedExpiresAt` lapse via Electric and shows "desktop disconnected." The desktop re-`hello`s on reconnect and resumes the same room.

### 3.3 Session model: `remote_steer_sessions` + the claim/permission/until model

Reuse and extend the interactive bookkeeping that already exists on `agent_runs` (`packages/db-schema/src/schema.ts`): `sessionId`, `runMode` (`run_mode` enum `background|interactive`), `interactiveClaimedAt`, `interactiveClaimedExpiresAt`, `lastError`. Those already model "a desktop interactive session owns this issue, bounded by an expiry." The remote-steer layer sits **on top** of that: the desktop owning the PTY is the publisher; the steer session governs **who among the humans may type**.

Add a new server-only table (NOT Electric-synced — it changes too often and viewers learn presence over the relay `presence` frame, not sync):

```
remote_steer_sessions
  id            uuid pk
  run_id / issue_id  uuid  → agent_runs.issue_id (cascade)
  workspace_id  uuid  → workspaces (cascade)      # for permission scoping
  viewer_id     text  → users.id                  # the human
  perm          steer_perm enum: 'view' | 'steer'
  claimed_at    timestamptz                        # when steer was granted
  claim_until   timestamptz                        # bounded exclusive-steer window
  released_at   timestamptz null
  created_at / updated_at
```
Add enum `steer_perm` to `packages/db-schema/src/domain.ts` (`steerPermValues = ['view','steer']`) alongside `subscriberSourceValues`, mirror into `packages/domain-contract/contract.json`, and run `bun run --filter @exp/domain-contract generate`.

**Claim model (single-steerer, cooperative).** Multiple viewers may `view` concurrently; **at most one** holds `steer` at a time (a terminal has one input cursor — two people typing is chaos). Steer is a **claim with an `until` window** (default 10 min, renewable): the first requester gets it; others queue and see "X is steering." The claim auto-expires at `claim_until` (so an idle steerer doesn't lock everyone out); a heartbeat from the active steerer's client renews it via `steer.renewClaim`. The **owner can always preempt** (`steer.forceClaim`) — owner intent beats a stranger's claim. The relay enforces the runtime rule (only forward `input` frames from the socket whose `userId == steererId`); Postgres is the durable ledger (`steer.claim`/`renewClaim`/`release` tRPC procs write `remote_steer_sessions` + mirror the active steerer's window into `agent_runs.interactiveClaimedExpiresAt` so Electric-synced clients show a "steered by" indicator without joining the relay).

**Local user coexistence.** The desktop's local human is *always* able to type into the terminal directly (it's their machine) — local input is never gated by the relay. The relay-forwarded remote `input` and local keystrokes both feed the same ghostty PTY (3.4). To avoid a fight, the desktop shows a small "remote steering active — <name>" banner and the local user can hit **Take over** (a local button that calls `steer.forceClaim` on their own behalf via the desktop's key, revoking the remote claim).

### 3.4 Desktop host changes (Zig + Swift): publish the surface, inject remote input, per-session slot pool

Today the terminal dock is grounded in `apps/linux/src/ui/terminal.zig` (a `GtkGLArea` wrapping a lazily-created `ghostty_surface`) and `apps/linux/src/core/agent/agent_manager.zig` (spawns the CLI in that surface on a `run_request`, reads back the exit, calls `agent_core_submit_run_result`). macOS mirrors this with `MacAgentTerminalRunner`. The steer publisher is a **host-app** component (agent-core stays "only the agent loop" — the locked rule), sitting beside `agent_manager.zig`:

- **New `SteerPublisher` (per run).** On `run_request` with `interactive:true`, after the terminal mounts, the host opens a publisher WebSocket to `STEER_RELAY_URL` (ticket from `steer.mintTicket` over the existing agent `expk_`), sends `hello{runId, cols, rows, sessionId}`, and **tees the ghostty surface output** into the relay socket. ghostty already invokes the host on output/render; add a byte tap. Two viable taps, pick per platform:
  1. **Byte tee at the PTY** — the cleanest: the CLI child's PTY master is already host-owned in the run wrapper (`agent_manager.zig` writes the per-run bash wrapper); tee the PTY read stream → (a) ghostty `ghostty_surface_*` feed for local display AND (b) the relay binary frame. This is verbatim terminal bytes, so xterm.js on the web renders identically.
  2. **Screen-grid snapshot** — for `resync`/`join` replay, dump ghostty's current visible grid as an ANSI-reconstruction (or a compact cell snapshot) so a late viewer gets current state. Emit this on `join`/`resync`, then switch to live PTY bytes.
- **Inject remote input.** The relay forwards `input{bytes}` (only from the active steerer). The host feeds those bytes into the **same PTY master write** the local keyboard writes to (in `terminal.zig` the input controllers already forward to `ghostty_surface_*`; remote input takes the identical path — write to the PTY, not the ghostty input API, so the CLI sees it as normal stdin). This is the point where local + remote input merge on one stream (3.3).
- **Kill-switch.** On a `kill` control frame (owner), the host tears down the run's terminal exactly like `run_cancelled` today (`destroyTerminal` in `agent_manager.zig` — destroying the surface kills the CLI child) and calls `agent_core_cancel_issue`.

**Per-session terminal-slot pool (the multi-window rework).** agent-core today has a **process-global** `InteractiveSlot` (`crates/agent-core/src/run_pipeline.rs`): `try_claim(issue_id)` allows exactly **one** interactive session process-wide; the FFI header says so (`at most ONE interactive session is live at a time`), and `agent_core_request_interactive` refuses a second with `interactive_session_active`. Multi-window v1 requires **concurrent** agent sessions, so this must become a **per-run slot pool keyed by runId**:

- Replace the single `interactive_slot: Arc<InteractiveSlot>` in `ffi.rs` / `run_pipeline.rs` with an `InteractiveSlotPool` (a `Mutex<HashMap<runId, SlotGuard>>` with a `maxConcurrent` cap, default 2 → make it configurable, matching `CoreConfigDto.maxConcurrent`). `try_claim(runId)` succeeds unless *that run* is already live or the pool is at cap (returns `Retry`/`run_already_in_flight` as today). `build_pipeline`'s `slot.try_claim(&issue.id)` becomes a pool claim.
- The `run_request`/`run_cancelled`/`run_finished` events already carry `runId` — the host already keys its terminal registry by run (`agent_manager.zig` runs registry). Each `SteerPublisher` is likewise keyed by `runId`. So one relay room per run maps 1:1 to one terminal-dock tab / detached window.
- This is a **frozen-ABI-adjacent** change: the C ABI signatures (`agent_core_request_interactive(issue_id)`, `submit_run_result(run_id,…)`) don't change; only the internal slot semantics and the informational `interactive_session_active` gate (now per-run, not global) do. Update the header comment block accordingly and the `ffi.rs` tests (`agent_core_approve_interactive` test around `run_pipeline` lines 550+).

### 3.5 Security, permissions, audit

- **Who can view / steer.** Minting a ticket (`steer.mintTicket`) checks: caller is authenticated (session or `expk_`), is a member of the run's `workspace_id`, and — the default rule — **only the run owner (the agent's `owner_user_id`) and workspace members with role `owner`/`admin` may `steer`; other members get `view`; non-members and `role=agent`/`role=member` on a public workspace get nothing.** Reuse the workspace-permission helper (`apps/web/src/hooks/use-workspace-permissions.ts` semantics; server-side membership in `apps/web/src/lib/auth/membership.ts`/`policies.ts`). The desktop publisher is implicitly the owner's machine.
- **No raw creds to the relay.** The relay only ever sees signed tickets; it verifies the HS256 signature with `STEER_RELAY_SECRET` and enforces `exp`. Compromising the relay leaks live terminal bytes of *active* sessions only (no persisted data, no DB), and cannot mint new access.
- **Kill-switch.** Owner can `kill` any session from any client → relay forwards → host tears down the terminal + `agent_core_cancel_issue`. Also a hard server path: `steer.killSession` sets `agent_runs.interactiveClaimedExpiresAt = now()` and the desktop, watching that over Electric, aborts even if the relay is unreachable.
- **Audit.** Every claim/steer/kill writes an `issue_events` row so it shows in the Linear-style timeline. Add event types to `issueEventTypeValues` in `domain.ts`: `steer_started`, `steer_ended`, `steer_killed` (mirror to contract.json + regenerate). The `remote_steer_sessions` rows are the durable ledger (who steered, when, until when).
- **Rate-limit + origin.** Relay applies the push-relay-style per-IP token bucket on connects; ticket `exp` is short (60s to connect, then the socket lives on its own). Reject `input` frames larger than a small cap.

### 3.6 Self-hosted & graceful degradation

- **Config.** `STEER_RELAY_URL` unset → the whole subsystem is **off**: `steer.mintTicket` returns a `disabled` result, the desktop never opens a publisher socket, and the web/mobile UI shows "Live steering unavailable on this instance" (the agent still runs, plans and PRs still sync over Electric — steering is additive, never load-bearing). This mirrors how `PUSH_RELAY_URL` unset disables push in `fcm.ts` without breaking anything.
- **LAN-only.** Self-hosters can point `STEER_RELAY_URL` at a LAN address (e.g. `ws://relay.lan:4002`); because both desktop and clients dial *out* to it, it works with zero inbound firewall rules on the desktop. Ship the relay in the same `docker-compose.yaml` as an optional service and document it beside the push relay in CLAUDE.md's infra list.
- **Cloud.** New Coolify app `exponential-steer-relay` cloning the repo and building `Dockerfile.steer-relay` (context `.`), holding `STEER_RELAY_SECRET`; `/healthz` gates the HEALTHCHECK. Same manual-deploy posture as the other Coolify apps.

### 3.7 Web & mobile viewer UI

- **Web:** add `@xterm/xterm` (+ `@xterm/addon-fit`) to `apps/web` (not currently a dep). A new `<SteerTerminal>` component on the issue / agent-run screen (beside the agent Plan Panel and `diff-view.tsx`): when `agent_runs.runMode === 'interactive'` and steering is enabled, show a **"Watch live" / "Take steering"** button. It calls `trpc.steer.mintTicket`, opens the viewer WebSocket, `join`s, pipes binary output frames into the xterm write path, and (if `perm === 'steer'` and it holds the claim) forwards xterm `onData` keystrokes as `input` frames. Show a presence bar (avatars from `presence`) and the claim/until countdown.
- **Mobile:** iOS renders bytes into a lightweight native terminal view (SwiftTerm or a minimal VT parser fed from the WebSocket); Android a Compose terminal surface. Read-only "watch" is the primary mobile use; "steer" sends keystrokes from the soft keyboard. Both reuse the same ticket-mint tRPC and the same relay wire protocol. The connection UI lives on the issue/agent-run detail screen next to the plan/diff, gated on the same permission + `STEER_RELAY_URL`-enabled flag delivered via a small `steer.config` proc.
- **No local runtime.** Per the locked platform roles, web/mobile have **no** local terminal/agent runtime — they are pure remote viewers/steerers of a desktop-hosted PTY.

### Definition of done

- [ ] `apps/steer-relay` (Hono/Bun, `@exp/steer-relay`) ships with `/healthz`, per-IP rate limit, `MAX_BODY_BYTES`, room registry keyed by `runId`, ring-buffer replay, and Bun-native WebSocket hub; `Dockerfile.steer-relay` + Coolify `exponential-steer-relay` app + `docker-compose.yaml` service.
- [ ] Wire protocol implemented on all four surfaces: `hello/join/resize/input/presence/claim/release/kill/bye` control frames + binary output frames, with drop-non-input backpressure and slow-consumer eviction.
- [ ] `remote_steer_sessions` table + `steer_perm` enum in `domain.ts`; `steer_started/steer_ended/steer_killed` added to `issueEventTypeValues`; mirrored into `contract.json` and regenerated (Swift/Kotlin); migration generated + applied.
- [ ] `steer` tRPC router: `mintTicket` (session/`expk_`, permission-checked, HS256 ticket via `STEER_RELAY_SECRET`), `claim`/`renewClaim`/`release`/`forceClaim`, `killSession`, `config`. Steer window mirrored into `agent_runs.interactiveClaimedExpiresAt`.
- [ ] agent-core: process-global `InteractiveSlot` replaced by per-`runId` `InteractiveSlotPool` (cap = `maxConcurrent`); `interactive_session_active` gate is now per-run; C ABI signatures unchanged; header comment + `ffi.rs` tests updated; concurrent interactive runs verified green.
- [ ] Desktop hosts (Zig `agent_manager.zig`/`terminal.zig`; macOS `MacAgentTerminalRunner`): `SteerPublisher` per run tees the ghostty PTY bytes to the relay, injects remote `input` into the same PTY write as local keys, honors `kill`, and is keyed by `runId` alongside the terminal registry. Local user can always type + "Take over."
- [ ] Web `<SteerTerminal>` (xterm.js) + iOS/Android native terminal viewers on the issue/agent-run screen, with watch/steer buttons, presence bar, claim countdown, gated on permission + relay-enabled config.
- [ ] `STEER_RELAY_URL` unset degrades cleanly everywhere (no publisher socket, UI shows "unavailable", agent + Electric sync unaffected); LAN-only outbound config documented in CLAUDE.md.

---

## 4. Desktop IDE workstream (Linux + macOS)

The two desktops are the IDE surface: embedded libghostty terminal, JetBrains-style run configs with a play button, coding-agent launch, PR diff review, and — new in this refactor — multi-window operation with concurrent agent sessions. Linux (`apps/linux`, Zig + GTK4) must reach web 1:1 pixel parity; macOS (`apps/ios/ExponentialMac`, SwiftUI) keeps its glass aesthetic but is fundamentally a **verify-and-polish** track on a real Mac. All four deliverables below respect the frozen agent-core C ABI (`crates/agent-core/include/agent_core.h`) and the libghostty gotchas.

### 4a. Run configs + play button (host-side arbitrary process launch)

**Decision (locked): run configs are HOST-SIDE.** The desktop app (Zig / Swift) spawns build/test/dev commands directly into a terminal-dock tab, bypassing agent-core entirely — the Rust core stays "only the agent loop." This mirrors the existing preview-config path, which already spawns preview backends host-side.

**Canonical config = `.exponential/config.json`, extended.** The committed repo file is already the canonical source for run commands (`apps/linux/src/ui/preview/preview_config.zig`; schema in `packages/db-schema/src/domain.ts` — `RunTarget` discriminated union `webTargetSchema`/`androidTargetSchema`/`iosTargetSchema`, `projectPreviewConfigSchema`, DB mirror `ProjectPreviewMirror`). The current union is *preview-shaped* (web/android/ios launch semantics). Extend it with a fourth, generic run-config kind for arbitrary commands:

1. **Schema (`packages/db-schema/src/domain.ts`).** Add a `command` platform to `platformValues` and a `commandTargetSchema` in the discriminated union:
   - `{ platform: 'command', id, name, argv: string[], cwd?: string (repo-relative, reject `..`), env?: Record<string,string> (strip PATH/LD_PRELOAD/DYLD_* server-side like the others) }`.
   - Add it to `runTargetSchema` and to the `RunTarget` TS union. The DB mirror `ProjectPreviewMirror.targets` already carries only `{id,name,platform}`, so command configs surface in the web settings list and cross-client display for free.
2. **Linux parser (`preview_config.zig`).** Add `command` to the `Platform` enum + `fromString`/`label`, add `argv: ?[]const []const u8` and reuse `root_dir`/`env` on `RunTarget`, and a `parseTarget` arm reading `argv` (array of strings) + `cwd`. Fold `argv`/`cwd` into `commandSetHash` so the existing trust gate re-prompts when an agent edits a launch command (the `.exponential/config.json` file is agent-editable and travels with the repo — the trust prompt is the security boundary; do NOT weaken it).
3. **Host spawn (Zig).** Add a `run_launcher.zig` next to `terminal.zig` that, given a parsed command target and the repo clone dir (`preview_config.repoCloneDir`), spawns `argv` with `cwd`+`env` into a **new terminal-dock tab** (see 4b for the per-tab dock). Reuse the per-run bash-wrapper pattern from `apps/linux/src/core/agent/agent_manager.zig` (writes a wrapper, runs it in an embedded ghostty terminal) but WITHOUT any agent-core round-trip: capture the child exit code directly and record it in an in-memory run-history ring per target.
4. **Play-button menu (top bar).** In `apps/linux/src/ui/app.zig`, add a play button to the content header (`adw_header_bar`). Clicking opens a `GtkPopoverMenu` with two groups: **Agent runs** (existing "AI" / plan actions that call `agent_core_request_interactive`) and **Run configs** (the parsed command + preview targets). Selecting a config launches it (or re-prompts trust). Show the last exit code + a spinner while running; a "Stop" entry destroys the tab's ghostty surface (which kills the child). Persist last-selected target id per repo (the trust store already keys per repo — add a sibling `last-run.json`).
5. **macOS mirror.** `MacShell.swift` / `MacTerminalDock.swift` / `MacPreviewBackends.swift` already spawn preview backends. Add the same play-menu (a SwiftUI toolbar `Menu` grouping agent runs + run configs) and a `Process`-based launcher into a `MacGhosttyTerminal` tab; reuse the existing `.exponential/config.json` read path on that side.

**Output/history:** exit code + captured tail is host-side state only (not synced) for v1 — the play menu shows "last run: exit 0 · 2m ago" per config. No new Electric shape.

### 4b. Multi-window + concurrent sessions (v1 requirement)

Two coupled changes: (i) the core's single interactive slot becomes a per-run pool, and (ii) both desktops gain detached windows.

**Core ABI implication — replace the process-global `InteractiveSlot` with a per-`runId` slot pool.** Today `crates/agent-core/src/run_pipeline.rs` defines `InteractiveSlot { owner: Mutex<Option<String>> }` with `try_claim` that admits exactly ONE interactive session process-wide; `ffi.rs` holds one `Arc<InteractiveSlot>` in `Runtime` and every entry point (`agent_core_request_interactive`, `agent_core_approve_interactive`, the dispatcher pipeline in `build_pipeline`) claims it, rejecting the second with `agent_error(interactive_session_active)`. That single-slot gate is exactly what blocks concurrency.

Rework it into a bounded pool:

1. **`run_pipeline.rs`:** replace `InteractiveSlot`/`InteractiveSlotGuard` with a `TerminalSlotPool { active: Mutex<HashMap<String /*issueId*/, ()>>, cap: usize }`. `try_claim(issue_id)` succeeds unless (a) that issue already has a live session (keeps the double-press race guard) or (b) `active.len() >= cap`. The guard's `Drop` removes the map entry. Keep `interactive_owned` per-issue semantics (the `IssuePatch { interactive_owned }` writes at run_pipeline.rs:306/361/411/456 are already per-issue — unchanged). The startup sweep `clear_interactive_owned_all()` in `ffi.rs::agent_core_start` stays.
2. **`ffi.rs`:** `Runtime.interactive_slot: Arc<InteractiveSlot>` → `terminal_pool: Arc<TerminalSlotPool>`; build it with `cap = config.max_concurrent` (already parsed from `CoreConfigDto.max_concurrent`, default 2). The dispatcher already runs `max_concurrent` pipeline threads, so the pool cap and the dispatcher concurrency must be the same number. On a full pool the dispatcher pipeline returns `PipelineOutcome::Retry` (requeue), exactly as today.
3. **ABI stays frozen — no header change.** `agent_core_request_interactive`/`approve_interactive`/`submit_run_result`/`cancel_run`/`cancel_issue` all already carry `run_id`/`issue_id`; the pool is an internal implementation change. **This is a pure Rust-internal rework — `include/agent_core.h` does not change**, so macOS (clang module map) and Linux (hand-declared externs in `apps/linux/src/ui/ghostty_ffi.zig` / core FFI) need NO re-binding. The only behavioral contract change: `interactive_session_active` now fires only when the pool is full or the same issue is re-triggered, not on the second distinct issue. Update the header comment near line 88–91 (single-terminal wording) and the `ffi.rs` doc comments accordingly, and update the FFI test `second_interactive_request_fails_fast_without_clobbering` to assert pool-cap behavior (2 distinct issues both mount; a 3rd at cap=2 is rejected).

**Linux detached windows.** Today `apps/linux/src/ui/app.zig` mounts one `adw_application_window` (`app.zig:244`, content set at 261/927) and a single bottom `TerminalDock` (`apps/linux/src/ui/terminal_dock.zig`) that mounts exactly one terminal (`current_term`, `mountTerminal` replaces the prior run). Rework:

1. **Terminal dock → tabbed, per-run.** Replace `TerminalDock`'s single `term_slot`/`current_term` with an `AdwTabView`/`GtkNotebook` of terminal tabs keyed by `runId` (agent sessions) or run-config id. `mountForManager`/`unmountForManager` (the C-style hooks the agent manager calls) become add-tab / close-tab-by-key. Honor the ghostty gotchas: a surface inits lazily only at NONZERO size, so only realize a tab's ghostty GLArea when the dock is expanded at a real height (keep the `set_size_request(-1, 200)` floor and the paned-position logic in `expand()`).
2. **Detach-to-window.** Add a "pop out" affordance on a tab that reparents its ghostty terminal into a new `adw_application_window` (a second top-level; `gtk_application` supports many windows — no single-window assumption is required by GTK, only by the current code). Because destroying a ghostty surface kills the CLI child, reparent (not recreate) the widget. Do the same for the **diff view** (4c) and the **preview webview** so users get detached terminal / diff / preview windows.
3. Concurrent agent sessions now coexist because the core pool admits `max_concurrent` and each gets its own tab; the dispatcher already parallelizes.

**macOS detached windows.** `apps/ios/ExponentialMac/ExponentialMac/` has a single main scene (`ExponentialMacApp.swift`) and a single `MacTerminalDock.swift` bound to one `MacGhosttyTerminal`. Add a `WindowGroup(id:for:)` (or `Window`) scene for detached terminal/diff/preview windows keyed by `runId`, and make `MacTerminalDock` a tabbed host over `MacAgentService`'s now-multiple concurrent runs. Same rule: reparent the `GhosttyKit` surface into the detached window rather than tearing it down.

### 4c. Linux 1:1 web parity (the #1 UI-quality gap)

Linux is GTK/Adwaita-styled and visibly diverges from the web app. This is the biggest open UI-quality item. The fix is to **hand-roll native GTK widgets sized to the web's pixel dimensions** rather than accept Adwaita defaults. Concrete debt catalogue (read `apps/linux/src/ui/{app,gtk,widgets}.zig` + `format.zig`, styling in the app's CSS classes like `exp-sidebar`, `card`, `diff-line`):

1. **Buttons too big / wrong metrics.** Adwaita buttons are taller and more padded than the web's shadcn buttons. Define an `exp-btn` CSS class matching shadcn's heights (default `h-9`=36px, sm `h-8`=32px, icon `h-5 w-5`), font-size, radius, and horizontal padding; apply it everywhere instead of bare `gtk_button_new_*`. Sidebar/nav rows must match the web sidebar row height and the 260px sidebar width is already pinned (`app.zig:698`).
2. **Wrong components.** Audit `widgets.zig` for places using stock Adwaita rows/lists where the web uses a specific shadcn primitive (status/priority dropdowns, label pills, filter pills, the issue-row grid `grid-cols-[24px_72px_24px_1fr_auto]`). Rebuild the issue row as a fixed-column `GtkGrid`/`GtkBox` matching that template (priority icon · identifier · status · title · labels+due) with the same gaps.
3. **Spacing / fonts.** Establish a token layer in CSS (spacing scale, the Inter font stack, OKLCH zinc colors, `--radius`) mirroring `apps/web/src/styles.css`, and replace ad-hoc `set_margin_*` calls with consistent tokens.
4. **Plaintext-only diff → syntax-highlighted side-by-side.** Current `diffFileWidget` (`app.zig:2024`) renders each patch line as a `GtkLabel` with `diff-add`/`diff-del`/`diff-hunk` CSS — unified, plaintext, no syntax highlighting. The web `diff-view.tsx` is also only line-colored unified today, so **this deliverable pushes Linux AHEAD of web**: build a real side-by-side view. Use `GtkSourceView` (GtkSourceLanguageManager guesses language from the filename in `PullFile.filename`) with a two-column layout (old / new) driven by parsing the unified `patch` into hunks. Keep the `+N -N` header. The data source is unchanged (`issues.prFiles` via the same tRPC query the Zig side already calls in `prDiffWorker`). Read-only for v1 (locked), but structure the hunk model so line-anchored comments can attach later.
5. **No row virtualization.** The issue list is a `gtk_list_box` inside a `gtk_scrolled_window` (`app.zig:867–873`) that materializes every row — janky on large workspaces. Move to `GtkListView` + `GtkSignalListItemFactory` backed by a `GListModel` (recycling), or `GtkColumnView` for the multi-column issue row. This is a structural rewrite of `refreshIssues`/`onIssueActivated`; keep the status-group collapse behavior (`state.collapsed[idx]`).

Deliver 4c as an incremental parity pass: (1) token+button CSS layer, (2) issue-row grid, (3) list virtualization, (4) side-by-side diff, verifying each against a side-by-side screenshot of the web app.

### 4d. macOS: keep glass, verify-and-polish on a real Mac

The macOS app design divergence is LOCKED: keep the SwiftUI `.ultraThinMaterial` glass aesthetic, aligning only the semantic status/priority tokens with web — do NOT chase pixel parity here (that's Linux's job). The real work is **runtime verification**: per `docs/native-desktop-roadmap.md` (§6, §9 ledger), phases A1–A5 and the review-fix pass are "build + launch green but never exercised on a real Mac / display." Inherit this as a verification track, not a rebuild. Verify, against `next.exponential.at` on a real Mac:

- **A2 — login + read-only live sync:** sign in (Better Auth session), confirm all 14 Electric shapes populate workspaces/projects/issues (the one part §A2 flags as unverified).
- **A3 — CRUD:** create/edit/status/priority/assignee/label toggle/comment mutations round-trip via tRPC + `generateTxId`, appear over Electric.
- **A4 — markdown editor:** the NSTextView WYSIWYG description/comment editor round-trips the GFM markdown contract (bold/italic/strike/code/H1–H3/lists/task-lists/blockquote/code blocks/links/images/@mentions) byte-identically to web/iOS; attachment upload works.
- **A5 M5 — agent identity:** `agent.register` from the Mac creates the synthetic device user and it appears in web `agents-section.tsx`.
- **A5 M6 — agent loop:** link `libagent_core.dylib`, assign an issue to the Mac device user, confirm the pipeline runs, opens a PR, and `agent_runs` populates the native Plan Panel.
- **A5 M7 — libghostty terminal:** GhosttyKit.xcframework surface actually renders on a display and accepts input; honor the ghostty gotchas — surface inits only at nonzero size (mount only when the dock is expanded), handle `GHOSTTY_ACTION_RENDER` in the action callback, and NEVER build libghostty from source on macOS (link the prebuilt `GhosttyKit.xcframework`).
- **Concurrency + play button (this refactor):** after 4a/4b land, verify the pool admits concurrent sessions and the play menu launches run configs into `MacGhosttyTerminal` tabs; verify detached windows reparent surfaces without killing children.
- **"Approve & continue here" is a HUMAN action:** confirm the host approves with the human session, THEN calls `agent_core_approve_interactive` (resume only) — the agent credential can't self-approve.

Ad-hoc-signing of the bundled `libagent_core.dylib` + hardened-runtime entitlements already ship; release-time notarization (Developer ID cert, real codesign, `notarytool submit`) stays a release-checklist item, not this workstream.

### Definition of done

- [ ] `.exponential/config.json` schema + Linux parser extended with a generic `command` run target (`argv`/`cwd`/`env`); `commandSetHash` covers the new fields so the trust gate re-prompts on command edits.
- [ ] Host-side launcher spawns arbitrary run configs into a terminal-dock tab on both Linux and macOS (NO agent-core round-trip); exit code + last-run history captured per config.
- [ ] Top-bar play button on both desktops opens a menu grouping **Agent runs** + **Run configs**, with per-repo last-selected memory and a Stop action.
- [ ] `crates/agent-core/src/run_pipeline.rs` `InteractiveSlot` replaced by a per-`runId` `TerminalSlotPool` (cap = `max_concurrent`); `ffi.rs` `Runtime` updated; `include/agent_core.h` UNCHANGED (verified — no re-binding on either client); FFI concurrency test updated and green; `cargo test` passes with 0 warnings.
- [ ] Linux terminal dock is tabbed per run; concurrent agent sessions coexist; detached terminal / diff / preview windows work by reparenting (not recreating) ghostty surfaces.
- [ ] macOS gains detached windows + tabbed dock over concurrent `MacAgentService` runs.
- [ ] Linux parity: shadcn-sized `exp-btn` CSS layer, issue-row fixed-column grid, `GtkListView`/`GtkColumnView` row virtualization, and a `GtkSourceView` side-by-side syntax-highlighted diff replacing `diffFileWidget`'s plaintext labels — each verified against a web screenshot.
- [ ] macOS A2–A5 + M5–M7 runtime-verified on a real Mac against `next.exponential.at` (login/sync/CRUD/editor/agent identity/agent loop/ghostty render), respecting all ghostty gotchas; glass aesthetic preserved.

---

## 5. Coordination clients workstream (web + iOS + Android)

These are the three **non-IDE** surfaces. They create/triage issues, comment, reassign, review agent progress, and **remotely watch + steer** a live desktop agent session — but they run **no local terminal and no agent runtime**. All of Section 2's Rust `agent-core`, run configs, and `claude`/`codex` spawning are desktop-only; web/mobile only read Electric rows and speak to the relay (Section 3) and tRPC. This decision is load-bearing and store-policy safe: nothing here shells out.

Ground-truth files this workstream touches:
- Web list/detail: `apps/web/src/components/issue-list.tsx`, `issue-detail-view.tsx`, `agent-panel.tsx`, `diff-view.tsx`, `mention-textarea.tsx`; sidebar `apps/web/src/components/workspace/sidebar.tsx`; routes under `apps/web/src/routes/w/$workspaceSlug/`.
- iOS: `apps/ios/Exponential/UI/Issue/{IssueListView,IssueDetailView,DiffView,CommentThreadView}.swift`, `UI/Home/HomeView.swift`, `UI/Navigation/MobileTabBar.swift`, `UI/Markdown/MarkdownEditor.swift`.
- Android: `apps/android/app/src/main/java/com/exponential/app/ui/issue/{IssueListScreen,IssueDetailScreen,PrDiffSection,CommentThread}.kt`, `ui/home/HomeScreen.kt`, `ui/markdown/`.

### 5a. "My Issues" — first-class cross-project view (assignee = me)

Decided: a top-level, cross-project view filtered to `assigneeId == currentUser`, present on **all three** coordination clients, above the per-project lists. No new column and no new shape — `issues.assigneeId` already exists and is indexed (`idx_issues_assignee` in `packages/db-schema/src/schema.ts`); the `issues` Electric shape already syncs everything needed. This is pure client work.

**Web**
1. New route `apps/web/src/routes/w/$workspaceSlug/my-issues/index.tsx`. Query the `issueCollection` (`apps/web/src/lib/collections.ts`) with `useLiveQuery`, `where eq(issue.assigneeId, session.user.id)` across the whole workspace (join issues→projects to scope by workspace; projects are already synced). Reuse `IssueList` + `IssueGroup` grouping from `lib/project-board.ts`; reuse `matchesFilters`/tab presets from `apps/web/src/lib/filters.ts`. Group by status like the project board, but prefix each row's identifier with its project (rows span projects). Row click → existing full-page detail route `projects/$projectSlug/issues/$issueIdentifier`.
2. Sidebar entry: add a `SidebarMenuItem` in `workspace/sidebar.tsx` in the same nav group as Search/Inbox (around the `Inbox` item, gated on `isAuthed`), icon `User`/`CircleUser` from `lucide-react`, `Link to="/w/$workspaceSlug/my-issues"`. Place it above Inbox.
3. Mobile: add "My Issues" to `WorkspaceMobileTopbar` (`workspace/mobile-topbar.tsx`) navigation.
- Reminders: use `and()`/`or()` from `@tanstack/react-db` (never JS `&&`), return `undefined` (not `false`) to skip the query while `session` is loading, and rely on `snakeCamelMapper` (already set on the collection) so `assigneeId` resolves.

**iOS**
1. New `apps/ios/Exponential/UI/MyIssues/{MyIssuesView,MyIssuesViewModel}.swift`. The view model queries the local GRDB store for `assignee_id = activeUserId` across all projects in the active account (mirror `IssueListViewModel`'s fetch but drop the project predicate). Reuse the existing row cell from `IssueListView.swift`; show project prefix per row.
2. Add a **My Issues** destination to `UI/Navigation/MobileTabBar.swift`. The pill currently holds Projects + Inbox; add a third tab (icon `person.crop.circle` / SF Symbol) so the order is Projects · My Issues · Inbox. Wire routing in `AppNavigator.swift`.

**Android**
1. New `ui/myissues/{MyIssuesScreen,MyIssuesViewModel}.kt`. View model observes the Room DAO with `assigneeId = currentUserId` across projects (mirror `IssueListViewModel`); reuse `SwipeableIssueRow`.
2. Add the destination to the bottom navigation used by `HomeScreen.kt` (the app's nav host), matching the Projects/Inbox pattern with a person icon.

Cross-client parity note: keep the "assignee = me, all projects, grouped by status" semantics identical on the three clients; there is no saved-view or custom-filter machinery here (cut list, Decision 9) — My Issues is a fixed built-in view, not a saved filter.

### 5b. Remote steer UI (watch + type into a live desktop agent session)

Decided: the coordination clients render a **live terminal** for a running agent session and can type into it, driven entirely through the relay + remote-steer-session model from **Section 3**. Electric carries only rows; the PTY stream and input RPC ride the relay's WebSocket/SSE. Web/mobile are pure relay **clients** — they never spawn a PTY.

**The "start agent on my desktop" action is nearly free.** Triggering clone⇒AI⇒PR from a phone is just **assigning the issue to the desktop-agent user** — the same synthetic `users.is_agent` user surfaced in the assignee picker (`issue-properties/assignee-dropdown.tsx`, and the iOS/Android `PickerSheet`/`IssuePickerSheet`). The owner's running desktop already watches the `assigned-issues` Electric feed and its dispatcher picks the issue up (carry-forward architecture, unchanged). So the coordination clients need **no new "run" endpoint**; the only genuinely new surface is the **steering terminal**. Add a small affordance: when the assignee is the desktop-agent user, the assignee menu / agent panel shows "Run on my desktop" copy that just performs the assign (and, if no desktop is currently connected to the relay for this owner, a hint that the run will start when the desktop comes online).

**Relay session contract (consumed here, defined in Section 3):** a remote-steer session row keyed to a `runId` (from `agent_runs`), with `permission ∈ {view, steer}` and a claim/until window governing who may type. Coordination clients:
- discover the active session for an issue via `agent_runs` (the 14th synced shape — read it directly, no `agentPlan.getState` round-trip) plus a relay "is-live" lookup;
- open a viewer that streams desktop→relay→client terminal frames;
- when the user holds a `steer` claim, send keystrokes client→relay→desktop as input RPC.

**Web** — new `apps/web/src/components/remote-terminal.tsx` using **xterm.js** (`@xterm/xterm` + `@xterm/addon-fit`; add to `apps/web/package.json`). Mount it inside `agent-panel.tsx` beneath the PR/diff row when a live session exists (gate on `agent_runs` state + relay liveness). A "Take control" button requests a `steer` claim from the relay; while held, forward `onData` to the relay socket; render incoming frames via `term.write`. Read-only viewers get frames but no input. Also expose it on the full-page issue route so it survives navigation.

**iOS** — new `apps/ios/Exponential/UI/Agent/RemoteTerminalView.swift`. There is no SwiftUI terminal primitive; render frames into a monospaced, scrollback text surface (a lightweight VT100-subset renderer is sufficient for v1 — the desktop side already runs the real terminal; mobile only mirrors output). A "Take control" button acquires the `steer` claim; a bottom input row sends keystrokes as input RPC over the relay socket. Surface it from `IssueDetailView.swift`'s agent section (next to `AgentPlanPanel`).

**Android** — new `ui/agent/RemoteTerminalScreen.kt`, same shape: a monospaced scrollback `Text`/`LazyColumn` fed by relay frames, a claim toggle, and an input field wired to the relay socket. Surface from `IssueDetailScreen.kt`.

Permissions: honor `WorkspacePermissions` (mirror of `apps/web/src/hooks/use-workspace-permissions.ts`) — only members who can act on the issue may request `steer`; public/read-only viewers get view-only or nothing. Enforcement is server/relay-side (Section 3); the UI just reflects the granted permission.

### 5c. Read-only PR diff review on all platforms

Decided: **read-only, syntax-highlighted, side-by-side** diff on every client for v1, but the data model stays **write-back-ready** so inline comments + approve/request-changes can be added later without a rewrite. All three clients already fetch patches via `trpc.issues.prFiles` → `issues.prFiles`; today they render a flat `<pre>` with +/−/@@ line coloring (web `diff-view.tsx`, iOS `DiffView.swift`, Android `PrDiffSection.kt`). Bring them to parity and upgrade rendering.

1. **Schema-ready for write-back (do now, render later):** ensure the `prFiles` payload carries, per hunk/line, the stable anchors GitHub needs for future review comments — `path`, `sha`/`blob` refs, and per-line `side` (LEFT/RIGHT) + `position`/`line` — even though v1 discards them. This is a payload/shape concern owned with Section 4 (GitHub/PR); the clients must **thread these fields through their `PullFile`/`PrFile` types** (`diff-view.tsx` `PullFile`, iOS `PrFile`, Android `data/api/PullFile`) so a later inline-comment layer has anchors without a data migration. No comment UI in v1.
2. **Web**: replace the flat `<pre>` in `diff-view.tsx` with a real side-by-side, syntax-highlighted view. Parse the unified patch into hunks (left/right columns), highlight with a lightweight tokenizer (`shiki` or `prism`, lazy-loaded), keep the existing add/del/context/hunk color language. Preserve the `agent-panel.tsx` "View changes" toggle entry point.
3. **iOS**: upgrade `DiffView.swift` from the flat patch text to a two-column (or unified-with-gutter, given phone width) syntax-highlighted view; keep the glass aesthetic tokens. Reuse the same parse-hunks logic conceptually as web.
4. **Android**: upgrade `PrDiffSection.kt` similarly (Compose two-pane on tablet width, unified on phone).
5. Keep the loading/empty/error/binary-fallback states already present in all three (they match — do not regress the "No textual diff (binary or too large)" and "Renamed." messages).

### 5d. Issue-to-issue linking + duplicate-of

Decided: two features, both wanted by the user (Decision 10).

**(1) Clickable issue-reference pills** in descriptions and comments, resolved like `@mentions`. Extend the **existing mentions pipeline**, do not fork it.
- Markdown interchange form: references are written as the **issue identifier** token (e.g. `MET-1153`) — the same "single interchange form, round-trips as plain GFM text" rule the markdown contract uses for `@email` mentions. This keeps byte-parity across web/iOS/Android editors and the GFM contract intact.
- Server resolution: extend `apps/web/src/lib/integrations/mentions.ts` with an `extractIssueRefs(text)` + `resolveIssueRefs(tx, text, workspaceId)` pair mirroring `extractMentionEmails`/`resolveMentions` — regex for the `{PREFIX}-{number}` identifier, resolved against `issues` joined to `projects` in the same workspace (so a ref only pills when the target is visible). Call these from `lib/trpc/comments.ts` (alongside `resolveMentions`, line ~84) and from `lib/trpc/issues.ts` on description save.
- Client rendering: the markdown renderers already pill known `@email` mentions; add an issue-identifier pill renderer in the same place — web editor `apps/web/src/components/issue-editor/` (TipTap node/decoration, mirroring the mention pill) and the iOS/Android markdown renderers (`apps/ios/.../UI/Markdown/`, `apps/android/.../ui/markdown/`). A resolved pill is clickable → navigate to that issue's detail route; an unresolved token renders as plain text.
- Editor autocomplete (web): mirror `mention-textarea.tsx` — trigger on a `#`-style or bare-identifier prefix, offer workspace issues (title + identifier), insert the `{PREFIX}-{number}` token. Mobile can ship pill **rendering** first and add the autocomplete affordance as a fast-follow.
- No new notification type is required for a plain reference in v1 (keep it a link). If a "referenced-in" signal is wanted later it reuses the existing notification delivery (Section 6) — do not add it speculatively.

**(2) Duplicate-of** — mark an issue as a duplicate of a canonical issue (a resolution).
- Schema: add a self-referential nullable FK `issues.duplicateOfId text references issues(id) on delete set null` in `packages/db-schema/src/schema.ts`, migrate (`bun run migrate:generate && bun run migrate`), and **mirror it in lockstep** across every synced surface: the `issues` shape proxy passes it through automatically, but add the column to web `Issue` type usage, the Zig `sync_manager.zig` specs[] (+ its expectEqual test), and the iOS/Android issue entity + DAO column lists. Guard native clients with the `tableColumnSet` self-heal ALTER (gotcha a) so no row is dropped.
- Behavior: marking duplicate sets `duplicateOfId` and moves status to `cancelled` (the existing "not active" terminal state — no new enum value; cut list forbids custom fields/new lifecycle). This is a resolution, so the update mutation in `lib/trpc/issues.ts` handles both fields atomically.
- UX (all three clients): a "Mark as duplicate…" action in the issue row/overflow menu (`issue-row-menu/` web; iOS `PickerSheet`/context action; Android `SwipeableIssueRow`/overflow) opening an issue picker (reuse the ref-autocomplete list). On the detail view, when `duplicateOfId` is set, show a banner "Duplicate of {IDENTIFIER}" with a clickable pill to the canonical issue (reuse the 5d-(1) pill component). Provide an "unmark" affordance clearing the FK.

### 5e. No local terminal / agent runtime on web or mobile (confirm)

Confirmed and enforced by construction:
- The coordination clients contain **no** process spawning, **no** `claude`/`codex` invocation, **no** `agent-core`/FFI, and **no** run-config execution. Those belong exclusively to the desktop apps (Sections 2 + 4).
- The only "run" action here is **assigning to the desktop-agent user** (5b), which enqueues work the owner's desktop executes.
- The only terminal here is a **remote mirror** of a desktop PTY over the relay (5b) — receive frames, send keystrokes; never a local shell.
- The diff (5c) is **read-only** on all clients in v1.
Any PR that adds a spawn, a bundled CLI, or a local PTY to web/iOS/Android is out of scope and must be rejected.

### Definition of done

- [ ] "My Issues" route + sidebar/tab entry live on web, iOS, and Android; filters issues by `assigneeId == me` across all projects in the workspace, grouped by status, rows clickable to detail. No new column/shape.
- [ ] Remote terminal component on all three clients connects to the Section 3 relay, renders live desktop-agent PTY frames (view), and sends keystrokes when holding a `steer` claim; permission gating honored via `WorkspacePermissions`.
- [ ] "Run on my desktop" surfaces as an assign-to-desktop-agent action (no new run endpoint); reflects relay liveness of the owner's desktop.
- [ ] PR diff upgraded to syntax-highlighted side-by-side (web/iOS/Android), read-only, with loading/empty/error/binary states preserved; `PullFile`/`PrFile` types carry write-back anchors (path/sha/side/line) though no comment UI ships.
- [ ] Issue-reference pills render and resolve in descriptions + comments on all clients via the extended `mentions.ts` pipeline; identifier token is the GFM interchange form; web editor autocomplete inserts it.
- [ ] `issues.duplicateOfId` added, migrated, and mirrored across web/Zig/iOS/Android sync (with tableColumnSet self-heal); "mark as duplicate" UX + canonical-issue banner on all clients; marking sets `cancelled`.
- [ ] Zero local terminal / agent runtime / CLI spawn on web or mobile; diff stays read-only in v1.

---

## 6. Notifications, email & built-in helpdesk

This workstream turns notifications into a **multi-channel delivery layer** (in-app + push + **email**), fixes agent-notification mis-routing, wires the away/phone killer flow end-to-end, and ships a **one-way helpdesk**: widget reporters get a resolution email when their issue is closed. Email and push are **table-stakes and FREE** — the moat is agents/seats/repos/tier, never "nothing gets lost." Google Calendar (`google-calendar.ts`, the `fireAndForgetSync` calls, and the `issues.googleCalendar*` columns) is being deleted in a separate workstream; do not add email paths to any calendar code.

### 6.1 The email primitive (delivery channel, NOT a new notification type)

The core rule: notifications keep their existing `notification_type` enum (`packages/db-schema/src/domain.ts` → `notificationTypeValues`; do not add `email_*` variants). **Email is a third fan-out target inside the existing `deliver()` function**, sitting beside the in-app row write and the push call in `apps/web/src/lib/integrations/notifications.ts`.

**Extend `deliver()` (`notifications.ts:104`).** Today it (1) writes `notifications` rows, then (2) fires push gated on `canUsePush()`. Add a third leg after the row write:

- Add an `emailBody` (or reuse `body`) and a stable `deepLinkPath` to the `deliver` args so the email can render a real "Open in Exponential" button (e.g. `/w/{slug}/projects/{slug}/issues/{identifier}`). Resolve the workspace/project slugs in `loadIssueMeta` (extend its select — it already joins `projects`).
- For each delivered recipient (the `deliveredIds` set returned by the dedupe insert — reuse it so email honors the same 30s dedupe window), resolve email eligibility and send. Fan email out with `Promise.all`, fully independent of the push branch. **Never let an email failure throw** — these are fire-and-forget; wrap per-recipient sends in try/catch and `console.error` only (mirror the existing `[notify]` logging).

**Decouple push AND email from plan gating.** Per decision 8, push and email are free. In `apps/web/src/lib/billing.ts`, `canUsePush()` (line 263) currently returns `limits.push`. Change the delivery layer so **push is no longer plan-gated**: either make `canUsePush` always return `true` (and drop the `limits.push` read) or, cleaner, stop calling it from `deliver()` entirely and always attempt push/email. Do **not** add a `canUseEmail`. Remove `push` from the plan-limits shape if nothing else reads it (grep `limits.push`); leave billing gating for seats/agents/repos/storage untouched.

**Send path — `apps/web/src/lib/email.ts` is the single sender.** It already implements the graceful-degradation contract: `sendEmail()` no-ops with a stderr log when `RESEND_API_KEY` is unset, and exports `emailEnabled`. Extend it, do not fork it:

- Add an **SMTP transport** alongside Resend for self-host (decision 8: cloud=Resend, self-host=SMTP or off). Introduce a small transport switch in `email.ts`: if `RESEND_API_KEY` set → Resend (existing fetch path); else if `SMTP_URL`/`SMTP_HOST` set → SMTP (use `nodemailer`); else → the existing logged no-op. Update `emailEnabled` to `Boolean(RESEND_API_KEY || SMTP_HOST)`. New env: `SMTP_URL` (or `SMTP_HOST`/`SMTP_PORT`/`SMTP_USER`/`SMTP_PASS`/`SMTP_SECURE`) + reuse `EMAIL_FROM`. Document all in `.env.example` and CLAUDE.md's env table.
- Add notification email templates next to `actionEmailHtml`/`sendPasswordResetEmail`/`sendVerificationEmail`. Add `sendNotificationEmail({ to, subject, heading, body, actionLabel: "Open in Exponential", actionUrl, unsubscribeUrl })` reusing `actionEmailHtml` (extend it to append an unsubscribe footer line). The auth emails stay exactly as-is.

**Per-user email prefs + unsubscribe tokens.** The `users` table (`packages/db-schema/src/auth-schema.ts:10`) is a Better Auth table; **do not add non-Better-Auth columns there** — create a dedicated server-only table instead:

- New table `user_notification_prefs` (server-only, NOT Electric-synced): `userId text pk → users.id (cascade)`, one boolean per channel/type as needed — minimum `emailEnabled boolean default true`, plus optional per-type opt-outs (`emailOnComment`, `emailOnStatus`, `emailOnMention`, `emailOnAssigned`, `emailOnAgentAction`) and `digest` enum (`off`|`daily`, default `off`). Add `unsubscribeToken text unique` (opaque, `randomUUID`/`nanoid`, minted lazily on first send). Missing row ⇒ treat as all-defaults (email on).
- Add a helper `emailRecipients(userIds)` in `notifications.ts` that joins `users` (to get `email`, skip agents/null email) and `user_notification_prefs`, returns `{ userId, email, unsubscribeToken }[]` filtered by the relevant per-type flag. `deliver()` calls this to build its email fan-out set.
- **Unsubscribe route:** a public file route `apps/web/src/routes/api/email/unsubscribe.ts` (GET `?token=…`) that flips `emailEnabled=false` for the matching token and returns a small confirmation page. Every notification email's `unsubscribeUrl` points here (`${BETTER_AUTH_URL}/api/email/unsubscribe?token=…`). This is the CAN-SPAM/one-click requirement — non-optional.
- **Settings UI (web-only, later polish acceptable):** a small "Email notifications" panel under account settings that toggles the same prefs via a tRPC `notifications.updateEmailPrefs` mutation. Native clients need no email-prefs UI for v1 (link to web, mirroring the billing/calendar web-only pattern).

**Optional digest.** Model-only for v1 wiring: when `digest='daily'`, `deliver()` skips the immediate email and instead the events accumulate as `notifications` rows; a cron (self-host `GITHUB_POLLING`-style env `EMAIL_DIGEST=true`, cloud a scheduled task) batches each user's unread rows into one daily digest email via `sendNotificationEmail`. Ship the pref + the skip-immediate branch now; the cron can land last without a schema change.

### 6.2 Fix agent-notification routing (decision 12)

`fireAndForgetAgentActionNotify()` (`notifications.ts:212`) currently calls `workspaceOwnerRecipients()` (line 194) — it hardcodes workspace **owners** as the recipients of `agent_plan_review` / `agent_question`. That is wrong for any multi-member or non-owner-assigned issue.

- Replace the `workspaceOwnerRecipients(issue.workspaceId)` call with **assignee + active subscribers**: `subscriberRecipients(issueId, actorUserId=<system/null>)` (line 68 — already filters `unsubscribed=false` and de-dupes) **unioned with the issue's current `assigneeId`**. The assignee is auto-subscribed in most flows, but union explicitly so a just-reassigned issue still routes. Extend `loadIssueMeta` to also select `issues.assigneeId`, or read it inline.
- Keep the agent-filter: `deliver()` already drops `isAgent` users via `withoutAgents()`, so the desktop-agent synthetic user never gets its own alert.
- **Fallback:** if that set is empty (unassigned, no subscribers — a widget-created issue with no human yet), fall back to `workspaceOwnerRecipients()` so plan-ready/questions are never silently dropped. Keep `workspaceOwnerRecipients` for exactly this fallback; do not delete it.
- No caller change needed (`agent-plan.ts:111` and the comments router keep calling the same function); the routing fix is entirely inside `notifications.ts`.

### 6.3 The away/phone killer flow — end to end

This is the north-star loop; this workstream owns only its **notification + email edges**, the rest is cross-referenced. Sequence:

1. **Issue arrives** (created, assigned, or a comment/mention). Existing `fireAndForget*Notify` callers fire from the tRPC mutations (`issues.ts:186/444/451`, `comments.ts:104`, `agent-plan.ts:111`).
2. **`deliver()` fans out in-app + push + email** (6.1). On a phone that's asleep, the **email + push** are what actually reach the user while away — this is why they must be free and reliable.
3. **Open on phone** → deep link (email button / push `data.issueId`+`identifier`) opens the native issue detail. Push already carries `type`/`issueId`/`identifier` (`notifications.ts:148`); ensure the email `actionUrl` uses the same identifier deep link.
4. **Assign to the desktop agent** from the phone (assigning to the desktop-agent user enqueues the run — carry-forward architecture). This fires `fireAndForgetAssignmentNotify` to the agent user, which `withoutAgents()` correctly drops.
5. **Watch/steer via the relay** — the live PTY stream is the **remote-steer relay** workstream (§ remote steer), NOT Electric and NOT this workstream. When the agent reaches plan-ready or a question, `fireAndForgetAgentActionNotify` (now correctly routed to the human assignee, 6.2) emails+pushes the phone so the user knows to jump into the relay session.
6. **Review the diff** (read-only side-by-side — PR-review workstream) and **merge** → `applyPrMergeState` (`apps/web/src/lib/integrations/pr-sync.ts`) flips state and fires the `pr_merged` push type; add a `pr_merged` email here too so the away user gets the "it's merged" confirmation.

This workstream's deliverable for the flow: **every step that should reach an away phone also emails** — assignment, mention, agent plan-ready, agent question, PR opened, PR merged. Verify each `fireAndForget*` path now produces an email when the recipient has `emailEnabled`.

### 6.4 Helpdesk — one-way v1 (decision 7)

A widget reporter is an end user who is **not** a workspace member. v1 is one-way: they get a **resolution email** when their reported issue is closed. Model it thread-ready so a two-way reporter thread + public status page can be added later without a rewrite.

**Capture the reporter as a subscriber on submit.** `createWidgetSubmission()` (`apps/web/src/lib/widget/service.ts:113`) already captures `reporterEmail` (from the widget form OR host `identify()`; schema `submitFieldsSchema.email`, line 60) and writes it to `widget_submissions.reporterEmail` (schema `packages/db-schema/src/schema.ts:603`). The submit transaction (line 209) today writes issue + attachment + `widget_submissions` row and explicitly does **not** create a subscriber. Change:

- **Add subscriber source `widget_reporter`** to `subscriberSourceValues` (`packages/db-schema/src/domain.ts:82`). This is an enum change — follow the enum protocol: update `packages/domain-contract/contract.json` (`subscriberSource`, line 55) and run `bun run --filter @exp/domain-contract generate` to refresh Swift/Kotlin constants. Generate + apply a Drizzle migration for the `subscriber_source` pgenum (`ALTER TYPE … ADD VALUE`).
- In the submit transaction, when `reporterEmail` is present, insert an `issue_subscribers` row with `source='widget_reporter'`, `unsubscribed=false`, `workspaceId=config.workspaceId`, `issueId`, and a `userId` — **but the reporter has no `users` row**. Two options; **chosen default: store the reporter email on the subscriber row, not a synthetic user.** Add a nullable `email varchar(320)` column to `issue_subscribers` used only for external (`widget_reporter`) rows, and make `userId` nullable **for that source only** (keep the existing FK for member subscribers). This avoids minting throwaway `users` rows and keeps the reporter out of member/agent fan-out. Reflect the nullable `userId`/new `email` column across the native mirrors per the shape-discipline rule (this table is the `issue_subscribers` synced shape — update Zig `sync_manager.zig` specs[], iOS/Android entity + DAO, and add the self-heal ALTER guard).
- Reporter identity precedence: form email (`fields.data.email`) wins if provided; else host `identify()` email if the widget forwards it (currently `identify()` sets name/userId — extend the widget config/submit to also forward an identified email when present). Reuse the already-persisted `widget_submissions.reporterEmail`.

**Send the resolution email on close.** When an issue's status transitions to `done` (or `cancelled` = "closed"), notify external `widget_reporter` subscribers:

- Hook the issue-status-change path in `apps/web/src/lib/trpc/issues.ts` (the same place `fireAndForgetStatusChangeNotify` fires, line 451). Add `fireAndForgetReporterResolution({ issueId, toStatus })` in `notifications.ts` that: guards on `toStatus ∈ {done, cancelled}`; loads external subscribers (`issue_subscribers.source='widget_reporter'`, `unsubscribed=false`, non-null `email`); and for each sends a **plain reporter-facing email** via `email.ts` — a new `sendReporterResolutionEmail({ to, issueTitle, resolutionNote?, unsubscribeUrl })`. **Do not** create in-app/push rows for reporters (they have no inbox and no account). Copy: "Your report '{title}' has been resolved." No internal metadata, no assignee names — reporters must not see the sensitive workspace context (see dogfood note).
- **Idempotency:** a reopen→re-close must not double-email. Add a `resolvedNotifiedAt timestamptz` to `widget_submissions` (or a per-subscriber flag) and skip if already set for the current close; clear it on reopen if you want re-notification. Chosen default: set-once per submission (`resolvedNotifiedAt`), don't re-notify on reopen churn.
- **Thread-ready modeling** (build the shape, not the feature): the resolution email already carries a stable issue reference. Reserve a per-reporter reply token on the subscriber/submission row (unused in v1) and keep the reporter's contact on the durable `widget_submissions` row so a future inbound reply route can attach a reporter comment. Do **not** build inbound parsing, a public status page, or reporter auth now.

### 6.5 Dogfood (decision e)

Exponential embeds **its own** feedback widget on a public feedback workspace with GitHub preconfigured. This already exists — extend, don't rebuild:

- `apps/web/src/lib/bootstrap-cloud.ts` creates the public `feedback` workspace (`isPublic:true`, `publicWritePolicy:everyone`, slug `feedback`), the `Exponential App` widget config (`FEEDBACK_WIDGET_NAME`), and `ensureDogfoodProject()` links the dogfood project to `DOGFOOD_REPO` via `projects.githubRepo` + `previewConfig`. `findDogfoodWidgetKey()` (`apps/web/src/lib/widget/dogfood.ts`) resolves the key for the in-app FeedbackButton.
- **GitHub preconfigured:** the repositories-as-first-class-entity workstream migrates `projects.githubRepo` into repository rows via a join table. When it lands, update `ensureDogfoodProject` to link the dogfood project to a **repository row** (not the TEXT `githubRepo`) so the agent can resolve a clone target and coding-first dogfooding works end-to-end. Cross-ref that workstream.
- **Owner-only sensitive data:** widget submissions carry reporter email/page/UA/customData (`widget_submissions`, server-only, not Electric-synced, read via the `widgets` tRPC router). Keep it that way — reporter PII must stay owner-visible only. The resolution email to the reporter (6.4) must **never** leak the internal metadata block that `buildWidgetDescription` embeds in the issue description; render a clean reporter-facing template.

### 6.6 Self-hosted email/relay optionality (decision f)

- **Email degrades gracefully.** With neither `RESEND_API_KEY` nor `SMTP_*` set, `email.ts` no-ops with a log (existing contract) — in-app + push still work, and the helpdesk resolution email is simply skipped. Never throw from a fire-and-forget notify path on a self-host box without email. The web UI hides email-only affordances via `emailEnabled` (extend beyond the current forgot-password gating to the email-prefs panel).
- **Push is optional too:** `sendToUser` (`fcm.ts`) already no-ops when `PUSH_RELAY_URL` is unset. Keep that.
- **The remote-steer relay** (live PTY) is a separate service and a separate workstream; its self-host LAN-only/outbound-friendly requirement is documented there. This workstream only guarantees that the **notification edges** (email/push telling you to go steer) degrade cleanly when a self-hoster runs without email or without the relay.

### Definition of done

- [ ] `deliver()` in `notifications.ts` fans out to a **third email channel** off the same deduped recipient set; push and email are **no longer plan-gated** (`canUsePush`/`limits.push` removed from the delivery path).
- [ ] `email.ts` supports Resend (cloud) **and** SMTP (self-host) with a graceful logged no-op fallback; `emailEnabled` reflects both; new `SMTP_*` env documented in `.env.example` + CLAUDE.md.
- [ ] `user_notification_prefs` table + `notifications.updateEmailPrefs` mutation + one-click unsubscribe route (`/api/email/unsubscribe`) shipped; missing-row defaults to email-on; digest pref modeled (immediate-skip branch in place).
- [ ] `fireAndForgetAgentActionNotify` routes to **assignee + subscribers** (owners only as empty-set fallback); verified on a non-owner-assigned issue.
- [ ] Away/phone flow emails on assignment, mention, agent plan-ready, agent question, PR opened, **and** PR merged (`applyPrMergeState`).
- [ ] `subscriber_source` gains `widget_reporter` (enum + `contract.json` + generated Swift/Kotlin + migration); widget submit records the reporter as an external subscriber (nullable `userId` + `email` column, mirrored across native shapes with the self-heal ALTER guard).
- [ ] Closing a widget-sourced issue emails the reporter a clean resolution notice (no internal metadata leak), idempotent via `resolvedNotifiedAt`; no in-app/push rows created for reporters.
- [ ] Reporter thread-readiness reserved (durable contact + reply token) without building inbound/public-status.
- [ ] Dogfood widget still resolves via `findDogfoodWidgetKey`; dogfood project relinked to a **repository row** once repos-as-entity lands; reporter PII stays owner-only.
- [ ] All paths no-op cleanly on a self-host instance with no email transport and no push relay; no fire-and-forget path throws.

---

## 7. GitHub, repositories & coding-first flow

This workstream makes repositories a first-class workspace entity, hard-wires the "no repo => needs-human" rule so Exponential is coding-first, and ships a real syntax-highlighted diff on every client while keeping the door open for PR write-back later. Today a repo is a single `projects.github_repo TEXT` column (`packages/db-schema/src/schema.ts:222`), the agent resolves it per-project via `mcp::get_project` (`crates/agent-core/src/run_pipeline.rs:190`), and the Linux diff is plaintext. All three change here.

### 7a. Repositories as a first-class workspace entity (many-to-many with projects)

**New tables.** Add to `packages/db-schema/src/schema.ts`:

- `repositories` — one row per connected GitHub repo, workspace-scoped:
  - `id uuid pk`, `workspaceId uuid → workspaces.id (cascade)`, `fullName text notnull` (`owner/name`), `defaultBranch text notnull default 'main'`, `private boolean notnull default false`, `installationId bigint` (nullable; mirror of the GitHub App installation that grants access, from `listInstallationRepos`), `sortOrder doublePrecision`, `archivedAt timestamptz`, `...timestamps`.
  - `unique().on(workspaceId, fullName)`; `index` on `workspaceId`.
- `projectRepositories` — the join table (composite PK), so one repo can back many projects and one project can list many repos:
  - `projectId uuid → projects.id (cascade)`, `repositoryId uuid → repositories.id (cascade)`, `workspaceId uuid → workspaces.id (cascade)` (denormalized for the shape filter — see the trigger note), `isPrimary boolean notnull default false` (the default clone target when an issue's project has more than one repo).
  - `primaryKey({ columns: [projectId, repositoryId] })`; index on `repositoryId`, index on `workspaceId`; a partial unique index enforcing at most one `isPrimary=true` per project.
- Add `Repository` / `ProjectRepository` `InferSelectModel` type exports and `selectRepositorySchema` / `selectProjectRepositorySchema`.

**Migration + backfill (decided).** Generate with `bun run migrate:generate`, then hand-author the data migration in the generated SQL file (Drizzle won't write it):
1. For each `projects` row with a non-null `github_repo`, upsert a `repositories` row (`workspaceId` from the project, `fullName = github_repo`, `defaultBranch`/`private`/`installationId` best-effort from `listInstallationRepos` at migration time, else `'main'`/`false`/`null`), then insert a `project_repositories` row with `isPrimary = true`.
2. **Keep `projects.github_repo` for one release as a read-through fallback**, then drop it in the following migration once every client reads the registry. This avoids a flag-day break of the running agent. Update the merge/diff read sites in the interim so they prefer the registry and fall back to `github_repo` (`agent-plan.ts:346`, `issues.ts:474`, `companion/identity.ts:34`, `companion/setup.ts:78`, `bootstrap-cloud.ts`, `bootstrap-self-hosted.ts`).
3. Add the `workspace_id` denormalization trigger for `project_repositories` mirroring the existing `populate_issue_*_workspace_id` triggers in `apps/web/src/db/out/custom/0001_triggers.sql` (join `project → workspace`). Remember: custom triggers are **not** auto-applied — document the manual `docker exec … psql < 0001_triggers.sql` step in the release checklist.

**tRPC.** New `repositories` router in `apps/web/src/lib/trpc/repositories.ts`, mounted in `api/trpc/$.ts`:
- `list({ workspaceId })`, `add({ workspaceId, fullName, defaultBranch, private, installationId })` (owner/admin only; validates the App is installed on that repo via `installationIdForRepo`), `remove({ repositoryId })`, `linkProject({ projectId, repositoryId, isPrimary })`, `unlinkProject`, `setPrimary`.
- Reuse `integrations.github.repos` (`github-app.ts` `listInstallationRepos`) to populate the picker; the registry `add` mutation persists the chosen repo as a `repositories` row.

**Repositories management UI (all clients).** New workspace-settings section "Repositories" (owner/admin), parallel to the existing `projects-section.tsx`:
- **Web**: `apps/web/src/components/workspace/repositories-section.tsx` — lists workspace repos, "Connect repository" opens the existing `GithubRepoPicker` (`apps/web/src/components/github-repo-picker.tsx`), and a per-project repo-link editor (multi-select of workspace repos with a primary star). Register it in the settings nav.
- **iOS / macOS / Android / Linux**: a read + link surface in workspace settings backed by the new synced shape. Native clients don't need the GitHub-App install flow (link to web for install, mirroring the billing/calendar web-only pattern), but they must render the repo list and let an owner pick which repo backs a project. Linux must reach web 1:1 per the parity mandate.

**New Electric shape (the 15th synced shape) — full lockstep checklist.** `repositories` becomes synced (the join `project_repositories` is small and client-derivable, so **sync `repositories` only**; expose the links through the projects/repositories join in queries, or add a 16th shape only if a client needs it live — default: don't). For `repositories`:
1. `CLAUDE.md` + `vision.md` client-parity list: bump "fourteen shapes" → fifteen synced (+ `assigned-issues` = 16 proxies).
2. **Web collections** — add `repositoryCollection` in `apps/web/src/lib/collections.ts` with `columnMapper: snakeCamelMapper()` (line 31 pattern) and `getShapeUrl('/api/shapes/repositories')`.
3. **Web shape proxy** — `apps/web/src/routes/api/shapes/repositories.ts` copied from `projects.ts` (workspace-scoped `getWhere` via `getReadableWorkspaceIds` + `buildWhereClause('workspace_id', …)`).
4. **Linux** — add `.{ .name = "repositories", .url_path = "/api/shapes/repositories", .table = "repositories" }` to `specs` in `apps/linux/src/core/electric/sync_manager.zig:19` (and its `expectEqual` count test), plus the `repositories` table DDL in `database.zig` and a self-heal `ALTER` (guard against the drop-whole-row-on-unknown-column gotcha via `tableColumnSet`).
5. **iOS/macOS** — add the `Repository` entity + its DAO to the synced-table lists in the Swift sync layer (ExpCore).
6. **Android** — add the `Repository` Room entity + DAO to the synced list.
7. **Fixtures** — extend `packages/electric-protocol/fixtures` with a `repositories` shape fixture so cross-client alignment tests cover it.

### 7b. Coding-first / GitHub effectively mandatory

**Formalize the null behavior into a first-class needs-human resolution.** The wiring already exists — `run_pipeline.rs` calls `ctx.needs_human(issue, ERROR_CODE_REPO_NOT_LINKED, …)` (`crates/agent-core/src/run_pipeline.rs:79,193`) and sets `status = needs_human`. This workstream makes it deterministic and registry-driven:

- **Change the agent's clone resolution to the workspace registry, per-issue.** In `resolve_handle` (`run_pipeline.rs:187`), replace `project.and_then(|p| p.github_repo)` with a repo resolved from the registry for the issue's project:
  - Server side, add `agent.resolveIssueRepo({ issueId })` (or extend the MCP `get_project` response) returning the project's **primary** `project_repositories` repo (`isPrimary=true`, else the sole link, else `null`), including `fullName` + `defaultBranch`. Update `crates/agent-core/src/mcp.rs` `struct Project` (line 147) / `get_project` (line 154) — or add a dedicated `resolve_repo` call — so the core reads `fullName`/`defaultBranch` from the registry, not `githubRepo`.
  - `null` repo => `needs_human` with `ERROR_CODE_REPO_NOT_LINKED` (unchanged code string — it's a frozen `agent_error` code). App installed-but-missing stays `ERROR_CODE_REPO_TOKEN_UNAVAILABLE`.
- **Retarget `companion.repoToken`** (`apps/web/src/lib/trpc/companion/identity.ts:34`): the gate currently joins `projects.githubRepo = input.repo`. Change it to authorize `input.repo` if it matches any `repositories.fullName` in a workspace the agent device is a member of (join `repositories` ↔ `workspace_members` on `workspaceId`). Keep the `owner/name` regex and the `resolveRepoInstallationToken` mint. Bump `defaultBranch` sourcing (`github::get_repo`) to prefer the registry value, falling back to the live GitHub lookup.
- **Surface "needs a repo" in the UI**: the agent-panel (`apps/web/src/components/agent-panel.tsx:75`, currently `project.githubRepo &&`) and the setup checklist (`companion/setup.ts:78` `repoLinked` signal, currently `isNotNull(projects.githubRepo)`) both switch to "does this issue's project have a linked repository?" Show a clear "Link a repository to let the agent code" CTA that deep-links to the new Repositories settings section. This is the coding-first funnel: an issue can't reach clone => AI => PR without a repo, and the product says so up front.

### 7c. PR review — read-only diff, schema-ready for write-back

**Keep the storage-free serving path.** The GitHub App (`apps/web/src/lib/integrations/github-app.ts`: App JWT RS256 → JIT per-repo installation token) and the diff endpoint (`issues.prFiles` → `fetchPullFiles` in `github-pr.ts:104`) stay as-is. `pr-sync.ts` remains **merge-detection only** (`applyPrMergeState`); it does not gain review state. No new inbound surface beyond the existing optional webhook.

**Ship syntax-highlighted side-by-side on every platform.** Web's current `diff-view.tsx` is a single-column colored `<pre>` (`lineClass` on `+`/`-`/`@@`). Upgrade to a real side-by-side, syntax-highlighted view:
- **Web (decided)**: render with a lightweight unified→split parser over `PullFile.patch` plus a token highlighter (Shiki or highlight.js keyed off the file extension). Two gutters (old/new line numbers), aligned hunks, intra-line add/remove background. Keep the existing "no textual diff (binary/too large)" fallback and the `+adds/-dels` header.
- **Linux (the gap — priority)**: the Linux app has only a plaintext diff. Build a native GTK side-by-side diff widget in `apps/linux/src/ui/` reading the same `issues.prFiles` payload (fetched via the Linux tRPC client), with syntax highlighting (GtkSourceView language guessing by filename, or a hand-rolled tokenizer to hit web 1:1). This is part of the Linux pixel-parity mandate.
- **iOS/macOS/Android**: side-by-side syntax-highlighted diff from the same `prFiles` payload; macOS keeps the glass aesthetic, others match web.

**Design the model so write-back can land later without a rewrite (v1 = read-only, no tables built yet).** Do not build review tables now, but lock the shape:
- A future `pr_review_comments` table (`issueId`, `prNumber`, `filename`, `side` (`old`|`new`), `line`, `body`, `authorId`, `githubCommentId` nullable, `state`) maps 1:1 onto GitHub's review-comment API. The diff parser must therefore key hunks by **`(filename, side, new/old line number)`** now, so inline anchors already exist when comments arrive.
- A future `pr_reviews` row (`event: approve|request_changes|comment`, `submittedBy`, `githubReviewId`) will POST to `/repos/{repo}/pulls/{n}/reviews` using the same JIT installation token from `resolveRepoInstallationToken` — the auth path is already correct and outbound-only.
- Extend `PullFile` return shape (`github-pr.ts`) to also carry `sha`/`previousFilename` (already available from the GitHub files API) so line-anchoring survives force-pushes. `pr-sync.ts` gains a review-state writer **later**; note in-code that it is intentionally merge-only today.

### Definition of done

- [ ] `repositories` + `project_repositories` tables added to `packages/db-schema/src/schema.ts` with types/zod schemas; migration generated and the `projects.github_repo` → `repositories` backfill written and run.
- [ ] `workspace_id` denormalization trigger added for `project_repositories` in `0001_triggers.sql` and applied manually to local + deployed DBs.
- [ ] `repositories` synced as the 15th shape: web collection + shape proxy, Linux `specs[]` (+ count test) + `database.zig` DDL/self-heal, iOS/macOS + Android entity+DAO, `electric-protocol` fixture, and `CLAUDE.md`/`vision.md` counts updated.
- [ ] `repositories` tRPC router (list/add/remove/linkProject/setPrimary) mounted; Repositories management UI shipped on web (owner/admin) and readable on all native clients.
- [ ] Agent clone resolution reads the workspace registry per-issue (primary repo of the issue's project); `crates/agent-core` `resolve_handle`/`mcp.rs` no longer keys off `project.github_repo`.
- [ ] `companion.repoToken` gate + `agent-panel` + setup-checklist `repoLinked` all switch to the registry; `null` repo deterministically yields `needs_human` / `repo_not_linked`.
- [ ] Syntax-highlighted side-by-side diff shipped on web, Linux (was plaintext), iOS, macOS, Android — all reading `issues.prFiles`.
- [ ] Diff parser anchors hunks by `(filename, side, line)` and `PullFile` carries `sha`/`previousFilename`, so inline-comment + approve/request-changes write-back can be added later without reworking the diff or auth path.

---

## 8. Billing moat, self-hosted parity & the cut list

This workstream is where the product's positioning becomes code: **simpler and cheaper than Linear**, self-hostable to full parity, and ruthless about what it refuses to build. It touches billing (web-only), the self-hosted gating of every new feature this refactor adds, and the removal of everything that would make Exponential "just another Linear."

### 8a. The billing moat — workspace-flat-rate, value-based

**Decided model: flat rate per workspace, not per seat.** Keep the existing three-tier shape (`free` / `pro` / `business` / `unlimited`) defined in `apps/web/src/lib/billing.ts` (`PLAN_LIMITS`, `PlanTier`) and priced in `apps/web/src/components/workspace/plan-comparison.tsx` (`TIERS`: $18/yr Pro, $60/yr Business, annual-only, FOUNDING 50%-off code). This is already flat-per-workspace — the moat is to **keep it that way** and never drift into per-seat metering.

**Monetize on value, never on notifications.** Charge on the axes that scale with how much a team gets out of the coding-agent superpower: **agents / concurrent runs, seats (member cap), linked repositories, and workspace tier**. Do **not** monetize on delivery reliability — email and push are **both free / table-stakes** on every tier.

Concrete change required in `PLAN_LIMITS` (`apps/web/src/lib/billing.ts`):

- The `free` tier currently has `push: false`; `pro`/`business` have `push: true`. **Flip `push` on for every tier** (or remove the `push` limit entirely) — push is no longer a paywalled feature. `getWorkspacePlan().limits.push` is consumed by `isPushEnabledForWorkspace` (billing.ts) and by the notification path (`apps/web/src/lib/integrations/notifications.ts` gates a "plan-gated push"); both callers must stop treating push as paid.
- **Email (the new delivery channel from §Email) is likewise never plan-gated.** Do not add an `email` boolean to `PlanLimits`.
- Add the value axes instead as needed: a repositories cap (per §Repositories) and/or a concurrent-agent-runs cap belong in `PlanLimits` if a paid axis is wanted there — this is where new limits go, not on notifications.
- Update `plan-comparison.tsx` `TIERS`: drop the "Push notifications" `FeatureRow` gate (show it enabled on all tiers, or move it out of the comparison as a baseline feature), and surface the real differentiators (members, projects, repositories, storage, AI agents / concurrent runs).

**Make limits non-opaque + nudge on hit.** The billing surface already shows usage bars (`WorkspaceBillingSection` → `UsageBar` for members/projects/storage in `billing-section.tsx`) and a full `PlanComparison` with per-tier feature rows. Extend, don't rebuild:

- Every server-side limit throw in `billing.ts` (`assertCanCreateWorkspace`, the project-count guard, `assertStorageWithinLimit`, etc.) already returns a `PRECONDITION_FAILED` (note: the tRPC code is `PRECONDITION_FAILED`, not `FAILED_PRECONDITION`) with a human message like "Your plan allows up to N …. Upgrade to …". **Standardize these** so the client can catch them and render an inline upgrade nudge (a small "Upgrade" CTA that deep-links to the workspace settings → billing section / `PlanComparison`) rather than a bare toast.
- In `plan-comparison.tsx`, ensure each tier row states **what unlocks** at the next tier (it already lists members/projects/storage/agents per tier — keep that concrete, not "contact us"). The point of the moat is that a user always sees exactly what they get and what the next $ buys.
- Add a repositories usage bar to `WorkspaceBillingSection` once the repositories entity lands (§Repositories), reading a new `usage.repositories` from `getWorkspaceUsage`.

**Billing stays strictly WEB-ONLY.** No native client (iOS / Android / macOS / Linux) shows any billing UI — store-policy safe. The `billingRouter` (`apps/web/src/lib/trpc/billing.ts`) and Creem checkout/portal routes (`/api/auth/creem/*`) are web-only by construction; keep them out of the desktop agent surface. Native clients that hit a paid limit link to the web app, they do not render `PlanComparison`.

### 8b. Self-hosted parity — every new feature has a self-hosted path

**Rule: self-hosted must fully support every feature.** Billing is the *only* thing that degrades on self-host — and it degrades to *unlimited*, not to disabled. The single gate is `process.env.SELF_HOSTED !== 'true'` via `isCloudInstance()` (`apps/web/src/lib/bootstrap-cloud.ts:264`); when self-hosted, `billingRouter.workspacePlan` / `.userPlan` short-circuit to `plan: 'unlimited'` with `Infinity` limits (`billing.ts` early returns), and `buildRuntimeConfig()` (`apps/web/src/lib/runtime-config.ts`) nulls the Creem product IDs so no checkout UI renders.

The three features this refactor adds each need an explicit self-hosted path:

- **Repositories (§Repositories):** the workspace repo registry + GitHub App integration is already storage-free and outbound-only (`apps/web/src/lib/integrations/github-app.ts` mints per-repo installation tokens JIT). Self-hosted works unchanged — it just needs a GitHub App configured via the existing `GITHUB_APP_ID` / `GITHUB_APP_SLUG` / `GITHUB_APP_PRIVATE_KEY` / `GITHUB_WEBHOOK_SECRET` env vars. Merge detection on self-host uses the outbound cron (`GITHUB_POLLING=true`) since the inbound webhook may be NAT-blocked. **No repositories cap is enforced on self-host** (unlimited plan).
- **Email delivery (§Email):** must degrade gracefully. Cloud uses Resend (existing `RESEND_API_KEY` / `EMAIL_FROM`). Self-hosted email is **optional** — configured via SMTP env (add e.g. `SMTP_HOST` / `SMTP_PORT` / `SMTP_USER` / `SMTP_PASS` / `EMAIL_FROM`) **or left unset**, in which case the email delivery channel is simply skipped and in-app + push still work. The email sender must probe config at startup and no-op (log, never throw) when neither Resend nor SMTP is configured — same fire-and-forget discipline the notification path already uses. Per-user email prefs + unsubscribe tokens + optional digest all work on self-host when email is configured.
- **Remote-steer relay (§Remote-steer):** the relay must be **LAN-only / outbound-friendly** — mirror the push-relay + GitHub-App outbound-only insight. The desktop connects *outbound* to the relay; viewers connect to the relay; nothing requires an inbound port on the desktop. Self-hosted deploys the relay alongside the stack (like the existing standalone `apps/push-relay`), pointed at via a `REMOTE_STEER_RELAY_URL` env with a shared `REMOTE_STEER_RELAY_SECRET` (mirror `PUSH_RELAY_URL` / `PUSH_RELAY_SECRET`). If unset on self-host, remote-steer degrades to "watch on the desktop only" — the agent still runs, you just can't steer it from your phone.
- **Feedback widget dogfood** already has both paths (`runtime-config.ts`): cloud resolves the dogfood `expw_` key from the DB (`findDogfoodWidgetKey`), self-hosted points at the cloud via `FEEDBACK_WIDGET_SCRIPT_URL` + `FEEDBACK_WIDGET_KEY`, or runs its own config. No change needed — just don't regress it.

**Enumerated `SELF_HOSTED` gating touch-points** (verify each still holds after this refactor):

- `apps/web/src/lib/bootstrap-cloud.ts` — `isCloudInstance()` (`SELF_HOSTED !== 'true'`), the canonical gate.
- `apps/web/src/lib/bootstrap-self-hosted.ts` — self-host bootstrap (feedback widget, etc.).
- `apps/web/src/lib/runtime-config.ts` — `buildRuntimeConfig()` nulls Creem IDs + resolves feedback widget per deployment.
- `apps/web/src/lib/billing.ts` — every `getUserPlan` / `getWorkspacePlan` early-returns `unlimited` when not cloud.
- `apps/web/src/lib/trpc/billing.ts` — `workspacePlan` / `userPlan` short-circuit when `!isCloudInstance()`.
- New: the email sender's config probe (Resend vs SMTP vs none) and the relay URL resolution both belong to this list once built.

### 8c. The cut list — the competitive edge

**The refusal to build these IS the moat.** Every item below is something Linear (or a Linear clone) has; each one is deliberately *not* in Exponential because simplicity is the product. Do **not** build, and **remove any half-present remnants**, of:

- Kanban drag-drop board (there is a `project-board.test.ts` / board-adjacent code — audit and remove any board view; the issue list is grid-only per the existing UX conventions)
- Saved filters / custom saved views (keep only the fixed tab presets in `apps/web/src/lib/filters.ts` — all/active/backlog; do not add a "save this filter" concept)
- Cycles / sprints
- Sub-issues / dependencies (already cut per prior rebuild — keep it cut; no `issue_relations` parent/child, no dependency graph)
- Time tracking
- Estimates / story points
- Custom fields
- Bulk edit (multi-select-and-mutate)
- Issue templates
- Agent marketplace
- MCP server browser (the agent uses a fixed MCP config; no in-app MCP discovery UI)
- Presence / typing indicators
- Timeline / Gantt
- Public roadmap share
- Linear import

**Dead code to delete now** (this refactor removes it, not "later"):

- **Google Calendar — cut entirely** (locked §Calendar). Delete: `apps/web/src/lib/integrations/google-calendar.ts`; the `fireAndForgetSync` / `fireAndForgetDelete` calls in `apps/web/src/lib/trpc/issues.ts`; the `googleCalendarEnabled` flag / `GOOGLE_CALENDAR_ENABLED` env handling in `apps/web/src/lib/auth/config.ts` and `apps/web/src/lib/trpc/integrations.ts`; the Calendar connect UI in `apps/web/src/routes/_authenticated/account/integrations.tsx`; and the schema columns `issues.googleCalendarEventId` / `googleCalendarLastSyncedAt` / `googleCalendarLastSyncError` (drop-column migration via `bun run migrate:generate && bun run migrate`, then mirror the column removal in every native client's table spec per the drop-a-row-on-unknown-column gotcha). Also scrub the `SELF_HOSTED` calendar mentions in `apps/web/src/lib/project-board.test.ts` / `context-menu.test.tsx` if they reference calendar.
- **Legacy agent-auth C entry points.** Remove from `crates/agent-core/include/agent_core.h` and `crates/agent-core/src/ffi.rs`: `agent_core_claim_setup` (header line 64 / ffi.rs:312), `agent_core_github_device_login` (header 65 / ffi.rs:324, the device-flow login — superseded by web `linkSocial`), and `agent_core_uninstall` (header 66 / ffi.rs:332). Delete the associated **setupToken model** and its server route. Update the ffi test at `crates/agent-core/src/ffi.rs:503` that calls `agent_core_uninstall`. Because the C ABI is FROZEN, treat this as a coordinated ABI change: remove the symbols, bump/regenerate the macOS module map + Swift wrapper and the Linux hand-declared externs in lockstep so nothing links against the removed symbols.
- **`companion.*` tRPC alias.** In `apps/web/src/routes/api/trpc/$.ts:44`, remove `companion: companionRouter` (keep `agent: companionRouter`). The router file stays at `lib/trpc/companion/` and is mounted as `agent.*`; only the temporary alias is dropped. Confirm no native client still calls `companion.*` before removing (grep the Zig/Swift/Kotlin agent code).
- **`agentPlan.getState` fallback + comment-based agent-plan path.** The synced `agent_runs` shape is the source of truth; native Plan Panels read it directly. Remove the drainable `getState` procedure (`apps/web/src/lib/trpc/agent-plan.ts:558`) and any comment-based plan/question rendering path, once every native client reads `agent_runs` directly. This is a "migrate clients first, then delete" removal — gate it on client-parity confirmation.

**KEEPs — explicitly wanted, do build** (out of scope for *this* section's implementation but must survive the cut):

- **Issue-to-issue linking** — clickable issue pills referencing other workspace issues inside descriptions/comments, resolved like `@mentions` (mirror `apps/web/src/lib/integrations/mentions.ts`).
- **Duplicate concept** — mark an issue as a duplicate of a canonical issue (a resolution), with a reference to the canonical.
- **My Issues** cross-project view (assignee = me) with a sidebar entry on web + mobile.

These three are the *counter-signal* to the cut list: Exponential cuts complexity, but keeps the small number of high-leverage relational features (linking, duplicates, My Issues) that users genuinely loved in Linear.

### Definition of done

- [ ] `PLAN_LIMITS` in `apps/web/src/lib/billing.ts` has push free on **all** tiers; no `email` limit added; push/email callers no longer treat delivery as paid.
- [ ] Any new paid axis (repositories and/or concurrent agent runs) lives in `PlanLimits`, not on notifications; `plan-comparison.tsx` reflects the real differentiators and drops the push paywall row.
- [ ] Limit-hit `PRECONDITION_FAILED` throws are standardized and the client renders an inline upgrade nudge deep-linking to `PlanComparison`; billing UI stays web-only (no native billing surface).
- [ ] Self-hosted returns `unlimited` for all plan checks; Creem IDs nulled off-cloud; email degrades gracefully (Resend / SMTP / none, never throws); relay is LAN-outbound-friendly with a graceful "watch-only" fallback; feedback-widget dogfood path unregressed.
- [ ] All `SELF_HOSTED` touch-points (bootstrap-cloud, bootstrap-self-hosted, runtime-config, billing lib + router, new email/relay probes) verified consistent.
- [ ] Google Calendar fully removed: lib, `fireAndForget*` calls, flag + env, integrations UI, and the three `issues.googleCalendar*` columns (with native-client column-spec mirroring).
- [ ] Legacy agent-auth C symbols (`agent_core_claim_setup` / `_github_device_login` / `_uninstall`) + setupToken model removed across header, ffi.rs, ffi test, macOS module map/Swift wrapper, and Linux externs; `companion.*` alias dropped; `agentPlan.getState` + comment-based plan path removed after client-parity confirmation.
- [ ] No cut-list feature (Kanban board, saved filters, cycles, sub-issues/deps, time tracking, estimates, custom fields, bulk edit, templates, agent marketplace, MCP browser, presence, timeline/Gantt, roadmap share, Linear import) exists in the codebase; issue linking + duplicate + My Issues KEEPs are preserved.

---

## 9. Sequenced execution plan for Fable

This turns Sections 1–8 into an ordered set of phases Fable executes top-to-bottom. The ordering rule is **foundations before consumers**: every synced-shape/column, enum, and server primitive lands (and is mirrored across the five clients) before any UI or subsystem reads it; the outbound relay lands before the desktop publisher and the coordination viewers that ride it; agent-core's slot-pool rework lands before multi-window; Linux parity and the diff upgrade land before the coordination clients that must match them. Section 1 (invariants) is not a phase — it is the standing "do not regress" contract every phase below is checked against.

Hard forks already resolved (do not re-litigate mid-execution):
- **Run configs are HOST-SIDE**, not an agent-core event vocabulary (§1.1, §2.2, §4a). Canonical file is the extended `.exponential/config.json`; DB mirror is `ProjectPreviewMirror` on the existing `projects` shape — no new shape.
- **`agent_runs` is NOT re-keyed.** Add `agent_run_history` (run-UUID PK, append-only) alongside it + an `agent_runs.currentRunId` FK (§2.3). The four-client Plan Panel read path is untouched.
- **Duplicate = `status='duplicate'` + self-FK `issues.duplicateOfId`**, not a generic `issue_links` relation table (§2.7, §5d). Note the two drafts diverge on the resolution status (§2.7 adds a new `duplicate` enum value; §5d reuses `cancelled`) — **resolve to §2.7's dedicated `duplicate` status value** during Phase 1 (it is the more explicit model and both fields ride the existing `issues` shape either way); this is the one open decision Fable must nail down before writing the migration.
- **Email is a delivery channel, NOT a `notification_type`** (§2.6, §6.1). Server-only tables (`user_notification_prefs`/`user_email_prefs`, `email_deliveries`), never synced. Push + email are both un-gated/free (§6.1, §8a).
- **The `remote_steer_sessions` table shape differs slightly between §2.4 and §3.3** (column names `viewer_user_id`/`viewer_id`, `claimed_until`/`claim_until`, and §3.3 adds a `steer_perm` pg enum where §2.4 uses a varchar). **Resolve to §3.3** (it owns the relay runtime): use the `steer_perm` pg enum and §3.3's column names. Server-only, not synced.
- **Repositories sync `repositories` only**; `project_repositories` is workspace-derivable — §2 lists it as [SYNCED] (17 shapes) while §7 syncs only `repositories` (15 shapes). **Resolve to §7's stance for v1: sync `repositories`, do NOT sync `project_repositories`** unless a client needs the links live; keep the count at **15 synced shapes / 16 proxies** after Phase 1. Revisit only if a native client can't derive links.
- **The C ABI stays frozen except the internal `InteractiveSlot`→pool change** (§1.3, §3.4, §4b): no header signature change, so neither host re-binds. The legacy agent-auth symbol removal (§8c) IS a deliberate ABI break, handled in the cleanup phase with both hosts updated in lockstep.

---

### Phase 0 — Cut/delete-first cleanup (clear the deck)

**Goal:** Remove dead weight and cut-list remnants before building, so no new code is written against doomed surfaces and the migration in Phase 1 isn't fighting soon-to-be-deleted columns.

**Deliverables (§8c, §2.8, §6):**
- Delete Google Calendar end-to-end (§2.8, §8c): `google-calendar.ts`, the `fireAndForgetSync`/`fireAndForgetDelete` calls in `issues.ts`, the `googleCalendarEnabled` flag + `GOOGLE_CALENDAR_ENABLED` handling in `auth/config.ts` + `integrations.ts`, the Calendar connect UI in `account/integrations.tsx`, and the calendar scope from `linkSocial`. **Leave Google login intact.** (The three `issues.googleCalendar*` column drops are folded into the Phase 1 migration so there is exactly one schema migration pass — but delete all the *code* now.)
- Audit + remove any cut-list remnants (§8c): Kanban/board view code (`project-board.test.ts` board-adjacent code), saved-filter concepts (keep only fixed tab presets in `filters.ts`), and confirm sub-issues/relations stay cut. Scrub calendar references in `project-board.test.ts` / `context-menu.test.tsx`.
- Drop the `companion.*` tRPC alias (`api/trpc/$.ts`) **after** grepping Zig/Swift/Kotlin for `companion.*` callers (keep `agent.*`).
- **Defer** the legacy agent-auth C-symbol removal and the `agentPlan.getState`/comment-plan removal — those are ABI-break / migrate-clients-first removals; they live in Phase 9 (cleanup-after-parity), not here.

**Acceptance gate:** `bun run typecheck` + `bun run test` green with zero calendar references; no board/saved-view code paths remain; `companion.*` alias gone and no client references it. Native builds (Zig/iOS/Android) still compile (calendar columns not yet dropped, so no mirror break yet).

**Platforms:** web (primary); grep-only pass on Linux/iOS/Android/macOS to confirm no `companion.*` callers.

---

### Phase 1 — Data-model & sync foundation (the load-bearing migration)

**Goal:** Land every schema change, enum, trigger, and the full five-client shape lockstep in ONE coordinated migration pass, so every later phase reads a stable model. This is the single biggest correctness-risk phase — the shape-discipline checklist (§1.7, §2.0) is its acceptance gate.

**Deliverables (§2, §7a):**
- **Repositories entity (§2.1, §7a):** `repositories` + `project_repositories` tables; partial unique index on primary repo; data migration off `projects.github_repo` (upsert repo rows + primary join rows). **Keep `projects.github_repo` for one release as a read-through fallback** (§7a) — do NOT drop it in this migration; its drop is a later migration once all clients read the registry. Add `populate_project_repository_workspace_id` trigger to `0001_triggers.sql`.
- **`repositories` synced as a shape** (resolve to 15 synced shapes / 16 proxies): web collection + `/api/shapes/repositories` proxy, Zig `specs[]` + count test + `database.zig` DDL/self-heal, iOS/macOS + Android entity+DAO, `electric-protocol` fixture, CLAUDE.md/vision.md counts. **Do NOT sync `project_repositories`** for v1.
- **`agent_run_history` table** (run-UUID PK, append-only) + `agent_runs.currentRunId` FK (§2.3); both [SYNCED] (history gets its own proxy; `currentRunId` rides the existing `agent-runs` shape). `agent_runs` current-state read path untouched. Add `populate_agent_run_history_workspace_id` trigger.
- **Server-only tables (not synced):** `remote_steer_sessions` (§3.3 shape — `steer_perm` pg enum, §3.3 column names), `user_notification_prefs`/`user_email_prefs` + `email_deliveries` (§2.6/§6.1). These skip the client-mirror steps.
- **Enum additions:** `subscriber_source += widget_reporter` (pg enum ALTER), `issue_status += duplicate` (pg enum ALTER + `displayOrder`), `steer_perm = [view,steer]` (new pg enum), `issueEventTypeValues += steer_started/steer_ended/steer_killed` — all mirrored into `contract.json` and regenerated via `bun run --filter @exp/domain-contract generate`. Verify the generated migration splits `ALTER TYPE … ADD VALUE` out of any transactional DDL.
- **Issue columns (ride existing `issues` shape):** `duplicateOfId` self-FK (§2.7/§5d); **drop** `googleCalendarEventId`/`googleCalendarLastSyncedAt`/`googleCalendarLastSyncError` (§2.8). Mirror both the add and the drops in iOS `Entities.swift` / Android `Entities.kt` / Zig specs with the `tableColumnSet` self-heal ALTER guard (§1.8a) so no native row is dropped.
- **`issue_subscribers` change for helpdesk (§6.4):** nullable `email varchar(320)` + make `userId` nullable for `widget_reporter` rows; mirror across the `issue_subscribers` synced shape (Zig/iOS/Android + self-heal ALTER). Add `resolvedNotifiedAt` to `widget_submissions` (server-only table, no mirror).
- **Run-config schema (no shape):** extend `ProjectPreviewConfig` with `runConfigs` and `ProjectPreviewMirror` with id/name-only `runConfigs` in `domain.ts` (§2.2/§4a). Rides the existing `projects` shape.

**Acceptance gate:** `bun run migrate:generate && bun run migrate` clean; `0001_triggers.sql` re-applied (documented manual step); `bun run typecheck` + `bun run test` + `bun run test:widget` green; the Zig `"shape registry: N shapes"` `expectEqual` test passes at the new count; iOS + Android compile and self-heal ALTERs run on an older local DB without dropping rows; `contract.json` regenerated and committed. **iOS stays green** (build + launch).

**Platforms:** all five (web schema/proxy/collection; Zig sync+db+test; iOS/macOS ExpCore entities+DAO; Android Room entity+DAO+version bump) + domain-contract regen.

---

### Phase 2 — Email primitive + notification routing fix (server-only, no new shape)

**Goal:** Turn notifications into a three-channel fan-out (in-app + push + email), un-gate push/email from billing, and fix agent-notification mis-routing — all server-side, riding the tables from Phase 1.

**Deliverables (§6.1, §6.2, §8a):**
- Extend `email.ts` with an SMTP transport (nodemailer) beside Resend; `emailEnabled = Boolean(RESEND_API_KEY || SMTP_HOST)`; graceful logged no-op when neither is set. Add `sendNotificationEmail(...)` + unsubscribe-footer. New `SMTP_*` env documented in `.env.example` + CLAUDE.md.
- Extend `deliver()` (`notifications.ts`) with a third email leg off the same deduped recipient set + `emailRecipients()` helper joining `user_notification_prefs`; fire-and-forget, never throws. Add `deepLinkPath` resolution in `loadIssueMeta`.
- **Un-gate push AND email** (§8a): flip `push` free on all `PLAN_LIMITS` tiers (or remove the limit); stop `deliver()` treating push as paid; add no `canUseEmail`.
- One-click unsubscribe route `/api/email/unsubscribe`; `notifications.updateEmailPrefs` mutation + web-only email-prefs panel (polish acceptable later). Digest pref modeled with the immediate-skip branch (cron lands in Phase 8).
- **Routing fix (§6.2):** `fireAndForgetAgentActionNotify` routes to **assignee + subscribers**, with `workspaceOwnerRecipients()` kept only as the empty-set fallback.

**Acceptance gate:** unit tests for the fan-out + routing; a manual/e2e check that a non-owner-assigned issue's agent-action notification reaches the assignee, and that email sends when `emailEnabled` (and cleanly no-ops when not); `plan-comparison.tsx` no longer shows push as a paywalled row. `bun run test` green.

**Platforms:** web only (native clients read notification rows over the existing `notifications` shape unchanged; email prefs are web-only UI).

---

### Phase 3 — GitHub repositories: agent resolution + tRPC + coding-first funnel

**Goal:** Make the agent resolve its clone target from the workspace repo registry (not `projects.github_repo`), enforce the coding-first "no repo => needs-human" rule deterministically, and ship the repositories management surface.

**Deliverables (§7a, §7b):**
- `repositories` tRPC router (`list/add/remove/linkProject/unlinkProject/setPrimary`), owner/admin-gated, reusing `integrations.github.repos`. Mount in `api/trpc/$.ts`.
- Agent clone resolution: `resolve_handle` (`run_pipeline.rs`) + `mcp.rs` `get_project`/a new `resolve_repo` read the registry (project → primary `project_repositories` → `repositories`), not `github_repo`. `null` repo → `needs_human` + `repo_not_linked` (frozen code, unchanged). Keep `github_repo` fallback read-through for this one release.
- Retarget `companion.repoToken` gate (`companion/identity.ts`) to authorize against `repositories.fullName` in a workspace the agent device belongs to.
- Coding-first funnel: `agent-panel.tsx` + setup-checklist `repoLinked` switch to "does this issue's project have a linked repo?" with a "Link a repository" CTA deep-linking to the new Repositories settings.
- Web `repositories-section.tsx` (owner/admin) using `GithubRepoPicker`; native clients render a read+link surface over the synced `repositories` shape (Linux at 1:1 — but the polished parity pass is Phase 6; a functional surface is fine here).

**Acceptance gate:** assigning an issue whose project has a linked repo runs the agent to a PR against the registry-resolved repo; a project with no repo deterministically yields `needs_human`/`repo_not_linked` and the UI shows the CTA; `companion.repoToken` mints against a registry repo. `cargo test` green (agent-core), `bun run typecheck`+`test` green.

**Platforms:** web (router + settings UI + funnel), agent-core (Rust resolution), all native clients (read/link surface + agent-panel repoLinked signal).

---

### Phase 4 — agent-core per-runId slot pool (concurrency foundation, ABI-frozen)

**Goal:** Replace the process-global `InteractiveSlot` with a bounded per-`runId` `TerminalSlotPool` so concurrent agent sessions are possible — the prerequisite for both multi-window (Phase 5) and remote-steer-per-run (Phase 5b relay). No C ABI signature change.

**Deliverables (§3.4, §4b):**
- `run_pipeline.rs`: `InteractiveSlot`/`InteractiveSlotGuard` → `TerminalSlotPool { active: Mutex<HashMap<issueId,()>>, cap }`; `try_claim(issueId)` succeeds unless that issue is live or the pool is at `cap`; guard `Drop` frees. Claim-before-slow-I/O ordering preserved per-slot. Startup `clear_interactive_owned_all()` retained.
- `ffi.rs`: `Runtime.interactive_slot` → `terminal_pool`, `cap = config.max_concurrent` (default 2, == dispatcher concurrency). Full pool → `PipelineOutcome::Retry` (requeue).
- `interactive_session_active` now fires only on same-issue re-trigger or full pool, never a blanket global refusal. Update the header comment block (single-terminal wording) and `ffi.rs` doc comments — **no signature change**, so neither host re-binds. Update the FFI test (`second_interactive_request_fails_fast_without_clobbering`) to assert pool-cap behavior (2 distinct issues mount, a 3rd at cap=2 is rejected).

**Acceptance gate:** `cargo test` green with 0 warnings; the concurrency test proves two distinct issues both claim slots and a third is rejected at cap; `agent_core.h` byte-unchanged (verified diff); both hosts build against the unchanged header without re-binding.

**Platforms:** agent-core (Rust) only; Linux + macOS re-build against the unchanged ABI to confirm no break.

---

### Phase 5 — Outbound remote-steer relay service (the data-plane)

**Goal:** Stand up the standalone outbound relay (`apps/steer-relay`) and its tRPC ticket/claim control plane, so a desktop can publish a live PTY and viewers can subscribe — before any desktop publisher or client viewer is wired.

**Deliverables (§3.1, §3.2, §3.3, §3.5, §3.6):**
- `apps/steer-relay` (`@exp/steer-relay`, Hono/Bun, Bun-native WebSocket hub): room registry keyed by `runId`, one publisher + N viewers, ring-buffer replay, `/healthz`, per-IP token bucket, `MAX_BODY_BYTES`, drop-non-input backpressure + slow-consumer eviction. `Dockerfile.steer-relay`, `docker-compose.yaml` optional service, Coolify `exponential-steer-relay` app.
- Wire protocol: `hello/join/resize/input/presence/claim/release/kill/bye` JSON control frames + binary output frames.
- `steer` tRPC router: `mintTicket` (session/`expk_`, permission-checked via `WorkspacePermissions`/membership, HS256 ticket signed with `STEER_RELAY_SECRET`), `claim/renewClaim/release/forceClaim`, `killSession`, `config`. Writes `remote_steer_sessions`; mirrors the active steer window into `agent_runs.interactiveClaimedExpiresAt`; writes `steer_started/steer_ended/steer_killed` `issue_events`.
- Env: `STEER_RELAY_URL`, `STEER_RELAY_SECRET`, `PORT` (default 4002) in web env + `.env.example` + CLAUDE.md infra list. Unset → subsystem disabled cleanly (`mintTicket` returns `disabled`).

**Acceptance gate:** relay `/healthz` green; a synthetic publisher + viewer over `wss` exchange output + input frames through the room with ring-buffer replay and single-steerer claim enforcement; ticket signature/`exp` verified relay-side; `STEER_RELAY_URL` unset yields a clean `disabled` result from `mintTicket`. Runs LAN-only outbound.

**Platforms:** new `apps/steer-relay` service + web (tRPC router, env). No client UI yet.

---

### Phase 6 — Desktop IDE workstream: run configs, multi-window, publisher, Linux parity + diff

**Goal:** Build the desktop IDE surface on top of the Phase-4 pool and Phase-5 relay: host-side run configs with a play button, tabbed/detached multi-window terminals, the `SteerPublisher`, Linux 1:1 parity, and the syntax-highlighted side-by-side diff. This is the largest desktop phase and can run its Linux and macOS sub-tracks in parallel.

**Deliverables (§4a, §4b, §4c, §7c, §3.4):**
- **Run configs + play button (§4a):** Linux `preview_config.zig` parses the new `command` target (`argv`/`cwd`/`env`, folded into `commandSetHash` trust gate); `run_launcher.zig` spawns into a terminal-dock tab (no agent-core round-trip); top-bar play button menu grouping Agent runs + Run configs with last-selected memory + Stop + exit-code display. macOS mirror in `MacShell`/`MacTerminalDock`/`MacPreviewBackends` + SwiftUI toolbar `Menu`.
- **Multi-window (§4b):** Linux terminal dock → `AdwTabView`/`GtkNotebook` of per-`runId` tabs; detach-to-window by **reparenting** (not recreating) ghostty surfaces for terminal/diff/preview; honor the nonzero-size + `GHOSTTY_ACTION_RENDER` gotchas per detached window. macOS `WindowGroup`/`Window` detached scenes + tabbed `MacTerminalDock` over concurrent `MacAgentService` runs.
- **SteerPublisher (§3.4):** per-run host component beside `agent_manager.zig`; on interactive `run_request`, opens a publisher socket (ticket via `steer.mintTicket` over the agent `expk_`), tees the PTY bytes to the relay (byte-tee at the PTY master), injects remote `input` into the same PTY write as local keys, honors `kill` (teardown like `run_cancelled` + `agent_core_cancel_issue`), keyed by `runId`. Local user can always type + "Take over" (`forceClaim`). macOS mirror in `MacAgentTerminalRunner`.
- **Linux 1:1 parity (§4c):** `exp-btn` shadcn-sized CSS token layer, issue-row fixed-column `grid-cols-[24px_72px_24px_1fr_auto]`, `GtkListView`/`GtkColumnView` row virtualization — each verified against a web screenshot.
- **Syntax-highlighted side-by-side diff (§4c #4, §7c):** replace Linux's plaintext `diffFileWidget` with a `GtkSourceView` two-column view; the diff **hunk model anchors by `(filename, side, line)`** and `PullFile` carries `sha`/`previousFilename` (§7c) so write-back can attach later. macOS diff upgraded (glass-preserving).

**Acceptance gate:** Linux — play menu launches a run config into a new tab; two concurrent agent sessions coexist in tabs; a tab detaches into its own window without killing the child; side-by-side diff renders syntax-highlighted; issue list virtualizes and matches a web screenshot at the button/row-metric level. Publisher tees bytes to a running relay and injects input. `cargo`/Zig build green. macOS deliverables build green (real-Mac verification is Phase 7).

**Platforms:** Linux (primary), macOS (mirror; runtime-verified in Phase 7), agent-core (SteerPublisher is host-side — no core change beyond Phase 4).

---

### Phase 7 — Verification-debt track: macOS + iOS on real hardware (runs alongside 6→8)

**Goal:** Discharge the standing verification debt (§1.10, §4d): the entire macOS app and much of the blind-written iOS Swift are "green build, never exercised." This is a **verify-and-polish** track on a real Mac/display against `next.exponential.at`, not a rebuild. It is scheduled here because Phases 1–6 have by now added shapes, the pool, the relay, and the desktop publisher/diff that macOS must exercise — but it should be treated as a continuous track that also re-checks each earlier phase's macOS/iOS deliverables as they land.

**Deliverables (§4d):**
- macOS A2 (login + all synced shapes populate), A3 (CRUD round-trip via tRPC+`generateTxId`), A4 (GFM markdown editor byte-parity + attachment upload), A5/M5 (agent identity register → appears in web `agents-section`), A5/M6 (link `libagent_core.dylib`, assign issue, pipeline → PR → Plan Panel from `agent_runs`), A5/M7 (GhosttyKit surface renders + accepts input, honoring nonzero-size + `ACTION_RENDER` + never-build-from-source gotchas).
- Verify this refactor's macOS additions on hardware: the per-runId pool admits concurrency; the play menu launches run configs into `MacGhosttyTerminal` tabs; detached windows reparent surfaces; the SteerPublisher streams to the relay and injects steer input; "Approve & continue here" uses the human session then `agent_core_approve_interactive` (resume-only, §1.8g).
- iOS: re-verify the shape/entity/DAO additions from Phase 1 and the coordination features from Phase 8 as they land (My Issues, remote terminal viewer, diff, links/duplicate) on a real device — keeping iOS green throughout.

**Acceptance gate:** each A2–A5/M5–M7 item observed working on a real Mac; concurrency + play button + detached windows + steer publisher exercised on hardware; glass aesthetic preserved (no pixel-parity chase on macOS); iOS builds+launches green after every phase that touches its entities/DAOs. Findings that can't be fixed on-device are logged as follow-ups, not silently passed.

**Platforms:** macOS + iOS (real hardware).

---

### Phase 8 — Coordination clients: My Issues, remote-steer UI, diff parity, links/duplicate, helpdesk

**Goal:** Ship the web + iOS + Android coordination surfaces that consume everything built so far — the relay viewer/steer terminal, the syntax-highlighted diff, My Issues, issue linking + duplicate, and the one-way helpdesk resolution email. No local terminal/agent runtime on these clients (§5e, enforced by construction).

**Deliverables (§5a–5d, §6.4, §7c):**
- **My Issues (§5a):** cross-project `assigneeId == me` view + sidebar/tab entry on web, iOS, Android. No new column/shape (rides `issues`). Fixed built-in view (no saved-filter machinery — cut list).
- **Remote-steer UI (§5b, §3.7):** web `<SteerTerminal>`/`remote-terminal.tsx` (xterm.js + addon-fit) in `agent-panel.tsx`; iOS `RemoteTerminalView.swift` + Android `RemoteTerminalScreen.kt` (lightweight VT/scrollback). Connects to the Phase-5 relay via `steer.mintTicket`, renders frames, sends keystrokes only while holding a `steer` claim; presence bar + claim countdown; permission gating via `WorkspacePermissions`; gated on `steer.config` relay-enabled flag. "Run on my desktop" = assign-to-desktop-agent (no new run endpoint) + relay-liveness hint.
- **Read-only diff parity (§5c, §7c):** upgrade web `diff-view.tsx`, iOS `DiffView.swift`, Android `PrDiffSection.kt` to syntax-highlighted side-by-side; thread `PullFile`/`PrFile` write-back anchors (path/sha/side/line) through the types though no comment UI ships; preserve loading/empty/error/binary states.
- **Issue linking + duplicate (§5d, §2.7):** extend `mentions.ts` with `extractIssueRefs`/`resolveIssueRefs` (identifier token is the GFM interchange form); pill rendering + web autocomplete; mobile pill-render first, autocomplete fast-follow. Duplicate UX (mark-as-duplicate action + canonical banner) on all clients, setting `status='duplicate'` + `duplicateOfId` (per the resolved fork).
- **Helpdesk one-way (§6.4):** widget submit records the reporter as a `widget_reporter` `issue_subscribers` row (nullable user + `email`); closing a widget-sourced issue (`done`/`cancelled`) sends a clean reporter resolution email via `sendReporterResolutionEmail` (no internal metadata leak), idempotent via `resolvedNotifiedAt`; no in-app/push rows for reporters. Dogfood project relinked to a **repository row** (§6.5) now that repos-as-entity exists.

**Acceptance gate:** My Issues works on all three clients; a phone viewer watches + steers a live desktop agent session end-to-end through the relay with claim enforcement; diff renders side-by-side syntax-highlighted on all three with anchors threaded; issue pills resolve + navigate and duplicate banners show; closing a widget issue emails the reporter exactly once with no PII leak; **zero** local terminal/CLI/agent-core on web/iOS/Android (rejected by construction). `bun run test`+`typecheck` green; iOS/Android build green.

**Platforms:** web, iOS, Android (macOS/iOS runtime re-verified via Phase 7).

---

### Phase 9 — Billing moat finalize + ABI/legacy cleanup-after-parity

**Goal:** Land the value-based billing polish and the removals that were gated on all clients having migrated (the "migrate-clients-first, then delete" items), now that Phase 8 has every client reading the new sources of truth.

**Deliverables (§8a, §8b, §8c):**
- **Billing (§8a):** add any new paid axis (repositories cap and/or concurrent-agent-runs cap) to `PlanLimits`; standardize `PRECONDITION_FAILED` limit throws so clients render inline upgrade nudges deep-linking to `PlanComparison`; add a repositories usage bar to `WorkspaceBillingSection`; `plan-comparison.tsx` reflects real differentiators (members/projects/repositories/storage/agents) and drops the push paywall row. Billing stays strictly web-only.
- **Self-hosted parity verification (§8b):** verify all `SELF_HOSTED` touch-points (bootstrap-cloud/-self-hosted, runtime-config, billing lib+router, new email + relay probes) return `unlimited`/degrade-gracefully; relay LAN-outbound "watch-only" fallback confirmed; email Resend/SMTP/none confirmed.
- **Drop `projects.github_repo`** (the deferred second migration from §7a) now that every client reads the registry.
- **Legacy agent-auth C-symbol removal (§8c) — the deliberate ABI break:** remove `agent_core_claim_setup`/`_github_device_login`/`_uninstall` from `agent_core.h` + `ffi.rs` + the ffi test + the macOS module map/Swift wrapper + Linux hand-declared externs, in lockstep; delete the setupToken model + server route.
- **Remove `agentPlan.getState` + comment-based plan path (§8c)** now that all native Plan Panels read `agent_runs` directly (parity-confirmed in Phases 7–8).

**Acceptance gate:** self-hosted instance returns unlimited + degrades cleanly with no email/relay/creem; hitting a paid limit shows an inline upgrade nudge; `projects.github_repo` gone with no reader left; `cargo test` green after the ABI symbol removal with both hosts rebuilt against the new header; `agentPlan.getState` gone with no client caller; full `bun run test`+`typecheck`+native builds green; iOS still green.

**Platforms:** web (billing), agent-core + Linux + macOS (ABI removal lockstep), all clients (getState removal parity), self-hosted config verification.

---

### How Fable should work this plan

Execute the phases in order — 0→9 — treating **Phase 1's five-client shape lockstep as the gate everything else stands on**: for every synced shape or column touched, walk the full §2.0 checklist (schema + zod/type export → `contract.json` + `bun run --filter @exp/domain-contract generate` for any enum → web collection with `snakeCamelMapper` + a `createShapeRouteHandler` proxy → Zig `specs[]` + count test + `database.zig` DDL and a self-heal `ALTER` → iOS/macOS + Android entity+DAO → CLAUDE.md/memory counts), and remember the **15 synced shapes / 16 proxies** resolution (sync `repositories`, not `project_repositories`). After any schema change run `bun run migrate:generate && bun run migrate` and then **manually re-apply `apps/web/src/db/out/custom/0001_triggers.sql`** (never auto-applied) including the two new denormalization triggers. Respect the frozen C ABI — the only sanctioned runtime change is the internal `InteractiveSlot`→`TerminalSlotPool` (no header signature change; the legacy-symbol removal in Phase 9 is the one deliberate, lockstep ABI break) — and honor all eight gotchas in new code (esp. row-drop guard for new columns, `PRECONDITION_FAILED` not `FAILED_PRECONDITION`, ghostty nonzero-size/`ACTION_RENDER`, and human-session approval under remote-steer). Keep **iOS green** after every phase that touches its entities/DAOs, run the **Phase 7 macOS/iOS real-hardware verification as a continuous track** rather than a one-shot at the end, and resolve the three flagged forks (duplicate status = `duplicate`; `remote_steer_sessions` = §3.3 shape; repositories sync = `repositories` only) before writing the Phase 1 migration so the foundation never has to be re-cut.
