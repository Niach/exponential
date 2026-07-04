# Exponential — Masterplan (v4, consolidated)

*2026-07-04. This is the single plan of record. It consolidates and supersedes
`docs/archive/masterplan-v2.md` (server/data/relay refactor — shipped),
`docs/archive/masterplan-v3.md` (gpui desktop IDE — shipped through Phase 7), and
`docs/archive/handoff-mac.md` (status delta). Those files are history, not spec: where this
document is silent about an inherited system, **the code is the spec** (see §2 "Where truth
lives"). `docs/vision.md` stays the product narrative and gets refreshed in Phase R5.*

**What v4 adds** — the "Project = Repository" iteration: every project **must** connect exactly
one GitHub repo; the desktop auto-clones it and becomes a real IDE around it (git top bar with
pull/push, a full source-control panel with staging/commit/history/diff, live per-issue
worktree diffs with Claude-driven conflict fixing, a read-only file tree + viewer, terminals
that open *in the repo*). Issues keep spawning Claude in their own worktrees. Run configurations become actually usable because the
working directory they need is now guaranteed to exist.

---

## Table of contents

- §1 Feature inventory — what the product is today
- §2 Locked decisions & where truth lives
- §3 Project = Repository: data model & server contract
- §4 Desktop IDE: clone lifecycle, git contexts, source control, files, terminals
- §5 Web: onboarding, project creation, settings
- §6 Mobile: iOS & Android adjustments
- §7 Migration & rollout
- §8 Execution plan (phases R0–R5, packetized for an Opus subagent workflow)
- §9 Do-not-regress contract

---

## §1 Feature inventory — what the product is today

This is the featureset we wanted and (except v4 scope) have. It is the checklist any future
"what does Exponential do?" question resolves against.

### 1.1 Platform core

- **Real-time sync**: ElectricSQL, exactly **14 synced shapes** (workspaces, projects, issues,
  labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments,
  notifications, issue_events, issue_subscribers, coding_sessions) behind auth-gated shape
  proxies (`apps/web/src/routes/api/shapes/`, built by `createShapeRouteHandler`). Server-only
  (tRPC, never synced): repositories, run_configs, github_installations, user_notification_prefs,
  email_deliveries, widget tables. Proxy count == shape count == 14 on **all four clients**.
- **Writes**: tRPC v11 mutations + `generateTxId` so clients await Electric sync.
- **Auth**: Better Auth — email/password, Google, OIDC (`genericOAuth`); personal API keys with
  `expu_` prefix (Better Auth apiKey plugin) power MCP + native clients; sessions for web.
- **Clients**: web (TanStack Start), iOS (SwiftUI + GRDB, self-contained), Android
  (Kotlin/Compose), desktop (Rust: gpui + gpui-component + alacritty_terminal). All honor
  `isPublic`/`publicWritePolicy` via a shared `WorkspacePermissions` mirror.
- **Theming/contract codegen**: `packages/design-tokens` (OKLCH → Compose/SwiftUI/Rust),
  `packages/domain-contract` (`contract.json` → per-language enum constants).

### 1.2 Issue tracking

- Workspaces → projects → issues; identifiers `{PREFIX}-{n}` via DB trigger; fractional
  `sortOrder`; statuses backlog/todo/in_progress/done/cancelled/duplicate (**no `in_review`** —
  "in review" = `prState='open'`); priorities; labels; due dates; recurrence (server spawns the
  next occurrence on completion); `duplicateOfId`; archival.
- Filtering (status/priority/label, tab presets), board + list views, full-page issue detail,
  My Issues, search sheet, issue timeline (`issue_events`), issue-reference `#` pills.
- **Markdown contract**: descriptions/comment bodies are plain GFM `text` — the single
  interchange across TipTap (web), cmark-gfm (iOS), commonmark-java (Android, byte-parity
  tests), pulldown-cmark/comrak (desktop). Mentions are `@<email>`; images stored relative
  `![alt](/api/attachments/{id})`; underline/tables intentionally unsupported.
- Comments, attachments (S3/Garage, probed width/height), subscriptions (auto-subscribe on
  mention/assignment), notifications (in-app inbox + FCM/web push via push-relay + **email** as
  a third free channel with prefs, `email_deliveries` ledger, one-click unsubscribe).

### 1.3 Coding flow (the differentiator)

- **"Start coding" launcher** (desktop, `crates/coding`): resolve issue → repo, mint a
  session-gated JIT GitHub-App installation token (~55 min, never persisted), `git` via argv
  (never `gh`, never a git library) — ensure clone, create worktree + `exp/<IDENTIFIER>` branch
  off `origin/<default>`, token-embedded remote re-set every launch, `.mcp.json` (expu_ key) +
  `PROMPT.md` written into the worktree (git-excluded via `.git/info/exclude`), then spawn
  `claude --model <model> --dangerously-skip-permissions` (model always explicit, default
  `opus`) in an embedded terminal tab, cwd = worktree. One issue = one branch = one worktree =
  one PR = one tab = one steer room.
- **coding_sessions** (14th shape) drives the cross-client "coding now" badge; server enforces
  the concurrent-session plan limit. `codingSessions.start`/`.end`, idempotent end.
- **MCP** (`/api/mcp`): `exponential_pr_open` (server opens + links the PR through the GitHub
  App), `exponential_issues_update_status` (in_progress|done only).
- **GitHub App**: storage-free (App JWT → JIT installation tokens); PR↔issue linking via the
  branch regex `/(?:^|\/)([A-Z0-9]+-\d+)$/`; webhook (cloud) + polling (`GITHUB_POLLING`,
  self-host behind NAT); `github_installations` synced from setup redirect + webhooks +
  empty-table self-heal.
- **Steer**: `apps/steer-relay` (Bun WS hub; device presence + session rooms in memory, ring
  replay, single-steerer claim), HS256 tickets (`packages/steer-ticket`; web mints, relay
  verifies), desktop is publisher + remote-start target (`start_session` enters the same
  `coding::launch` path), web/iOS/Android are viewers (xterm.js / native VT). Wire truth:
  `apps/steer-relay/src/protocol.ts`. `STEER_RELAY_URL` unset ⇒ cleanly off.
- **Run configurations**: `run_configs` table (server-only, per project — argv/cwd/env, env
  strips PATH/LD_PRELOAD/DYLD_*), desktop run bar + per-device **Trust & Run** gate keyed on
  the command-set hash (rusqlite trust store), argv-direct spawn (never a shell), SIGTERM→SIGKILL
  stop, exit-code strip on the tab.

### 1.4 Desktop IDE (gpui)

- App shell: chrome-less sidebar (left dock), screen panel (center: board / issue detail /
  My Issues / inbox / settings / account), terminal dock (bottom) with `TabKind::{Claude, Run,
  Shell}`, run bar; multi-window "reparent, never recreate".
- Terminal: upstream `alacritty_terminal` + `portable-pty` (we own the PTY master; single
  read thread; steer tee; reply-required terminal events), login-shell PATH augmentation.
- Read-only side-by-side **PR diff** (Tree-sitter highlight, `issues.prFiles`).
- Auth: instance picker (Cloud vs self-hosted base URL, cloud button first), `exp://` deep links
  (macOS Info.plist + `LSSetDefaultHandlerForURLScheme`; Linux `.desktop` + single-instance
  socket), file-based `0600` token store (never OS keyring), per-account
  `{data_dir}/accounts/{id}/sync.sqlite`.
- Doctor (`claude --version`, `git --version`) gating Start coding; settings: repos root
  (default `~/Exponential/repos`), claude path, branch prefix, model.
- Channels: compile-time `production`/`staging` features; Linux AppImage + macOS `.app` in CI.

### 1.5 Ecosystem

- **Feedback widget** (`packages/widget`): embeddable Preact snippet, snapDOM screenshots +
  annotation editor, `expw_` keys + domain allowlist, rate-limited public routes, issues created
  by a synthetic `isAgent` user (never delete — `issues.creator_id` cascades), helpdesk reply
  emails to reporters; dogfood config on the public feedback workspace.
- **Billing** (web-only, Creem): plan axes members/projects/storage/repositories/
  concurrentCodingSessions/ownedWorkspaces; push/email never paywalled; `SELF_HOSTED=true` ⇒
  unlimited. Admin console web-only.
- **Deploys**: Coolify (cloud web/staging/marketing/push-relay; steer-relay app still to
  create), Gitea → Portainer self-host, GH Actions desktop pipeline (two channels × two OSes).

---

## §2 Locked decisions & where truth lives

Decisions carried over (still binding) plus the v4 decisions made 2026-07-04.

| # | Decision | Locked |
|---|----------|--------|
| L1 | Exactly **14** Electric shapes; repositories/run_configs stay server-only tRPC | v2 |
| L2 | GFM plain-text markdown contract; `@<email>` mentions; relative image URLs | v2 |
| L3 | Personal keys `expu_`; the coder is the **real user** (no synthetic agents in the coding flow) | v2 |
| L4 | No `in_review` status; "in review" = `prState='open'` | v2 |
| L5 | `git` via argv only — never `gh`, never libgit2; JIT tokens never persisted/logged | v2 |
| L6 | Steer wire truth = `protocol.ts` + `packages/steer-ticket`; input field is `data` | v2 |
| L7 | GPL boundary: Zed terminal/editor code is study-only; ship upstream-licensed deps | v3 |
| L8 | `run_configs` live in the **DB per project**, not in the repo; per-device Trust & Run gate | v3 |
| L9 | Token store file-based `0600`, never OS keyring; model always passed explicitly (default `opus`) | v3 |
| L10 | **Project ↔ repository is 1:1 and mandatory**: `projects.repository_id NOT NULL`; `project_repositories` is deleted; a repo may still back many projects | **v4** |
| L11 | **GitHub App only** — a configured GitHub App is now a prerequisite for any instance where projects get created (cloud and self-host). Plain-git/GitLab/Gitea is out of scope for v4 (documented future work) | **v4** |
| L12 | **Auto-clone on project open** in the desktop (background, with progress); trunk freshness = fetch on open / after transport ops / focus-debounced | **v4** |
| L13 | **Full source-control panel** (trunk): stage/unstage, commit, commit+push, history, working diffs; top-bar Pull/Push auto-rebase (`--autostash`); conflicts → Fix with Claude | **v4** |
| L14 | **Two git worlds**: all IDE git chrome acts on the **trunk only**; issue worktrees have no manual transport — they surface as a live **Changes** tab inside the issue | **v4** |
| L15 | **Read-only file tree + viewer** (Tree-sitter highlight). No editable code editor in v4 | **v4** |
| L16 | Shape count stays 14 in v4 — `projects.repository_id` rides along on the existing `projects` shape | **v4** |
| L17 | **Claude-task primitive**: conflict fixing (trunk rebase, unmergeable PRs) spawns a one-shot interactive `claude` terminal tab with a generated prompt — visible and steerable, never hidden automation; no MCP, no `coding_sessions` row | **v4** |
| L18 | **Web/mobile do no git operations.** Remote visibility per issue is tiered: PR diff → server-side branch compare (`repositories.branchDiff`) → watch/steer of the live session. The branch map (§4.10) is stretch-only (R6), never critical path | **v4** |

**Where truth lives** (for inherited systems this doc summarizes but does not re-spec):

- Steer wire protocol: `apps/steer-relay/src/protocol.ts`, `packages/steer-ticket/src/index.ts`
- Launcher sequence: `apps/desktop/crates/coding/src/launcher.rs` (+ `git_worktree.rs`)
- Enum values: `packages/domain-contract/contract.json` (+ generators)
- Shape/proxy rules: `apps/web/src/lib/shape-route.ts`, `apps/web/src/lib/collections.ts`
- Markdown contract: CLAUDE.md "Description / comment markdown contract" + Android parity tests
- Run-config validation: `apps/web/src/lib/run-configs.ts` ↔ `crates/coding/src/run_launch.rs`

---

## §3 Project = Repository: data model & server contract

### 3.1 Schema (`packages/db-schema/src/schema.ts`)

- `projects` gains `repositoryId: uuid("repository_id").notNull().references(() =>
  repositories.id, { onDelete: "restrict" })` + index. `restrict` (not cascade): you cannot
  delete a repo that still backs a project — retarget or delete the projects first.
- **Delete `project_repositories`** entirely (table, indexes, zod/type exports).
- `repositories` unchanged: it stays the workspace-level registry (`unique(workspaceId,
  fullName)`, cached `installationId`, `sortOrder`, `archivedAt`) so one repo can back several
  projects (monorepo) and plan limits keep counting registry rows.
- `run_configs` unchanged.
- Shape ride-along: `repository_id` is added to the `projects` shape columns; **no new shape**.
  Clients see the uuid; resolving it to `fullName`/`defaultBranch` is a tRPC read
  (`repositories.list` / `repositories.forIssue`) exactly as today.

### 3.2 tRPC contract

`projects` router:

- `create` input becomes `{ workspaceId, name, prefix, color, repository:
  { repositoryId } | { fullName } }`. With `fullName`, the server connects the repo inline
  (same validation as `repositories.add`: owner/admin, plan cap, App-install check via
  `resolveRepoInstallationToken` → `PRECONDITION_FAILED` if the App isn't installed on it,
  upsert + un-archive) and links it — one transaction, so onboarding/create dialogs are a
  single call. With `repositoryId`, validates same-workspace + not archived.
- New `setRepository({ projectId, repositoryId })` (owner/admin) — retargeting. Fires an
  `issue_event`-less workspace-level concern; existing worktrees for old-repo issues keep
  working locally (they're just git), but new launches use the new repo.
- `create` keeps enforcing the projects plan axis; the inline connect path also enforces the
  repositories axis.

`repositories` router:

- **Delete** `linkProject`, `unlinkProject`, `setPrimary`.
- `forIssue` simplifies to issue → project → `repositoryId` join (no primary/sole-link
  ambiguity; never null for a valid issue — return shape keeps `| null` only for dangling
  data safety).
- `list` drops `projectLinks` in favor of `projects: {id, name, slug}[]` computed from
  `projects.repositoryId` (used by settings "in use by" chips and mobile pickers).
- `remove` maps the FK `restrict` violation to `CONFLICT` with "repository backs N projects".
- `installationToken` unchanged (member-gated JIT mint, defaultBranch heal).
- New `branchDiff({issueId})` (member-gated): resolves the issue's repo + `exp/<IDENTIFIER>`
  branch, calls GitHub's **compare** API (`<default>...<branch>`, installation token,
  ~60s in-memory cache per branch) and returns the `prFiles` shape so all clients reuse the
  existing diff rendering. Returns null when the branch was never pushed. This is the middle
  tier of remote Changes visibility (L18) — pushed work is visible before a PR exists.
- New `graph({projectId})` — **stretch, R6 only**: trunk commits + active/merged `exp/*`
  branches with fork points, feeding the web branch map (§4.10). Not built before R6.

`onboarding` router: `complete` unchanged; the wizard change is client-side (§5.1).

### 3.3 Bootstrap & system paths

- `bootstrap-cloud`: creating the public feedback workspace's `Exponential` project now requires
  a repositories row first — bootstrap upserts `{ workspaceId, fullName: "Niach/exponential",
  installationId: null }` directly (no App validation on this internal path; the
  empty-`installationId` self-heal fills it) and passes its id to project creation. The
  pre-collapse heal path repoints widget configs as today.
- Seed/dev scripts and tests that create projects must supply a repository (add a
  `test-repo` factory in vitest helpers).
- **Self-host consequence (accepted)**: instances without `GITHUB_APP_*` configured cannot
  create projects. `.env.example` + README get a loud note; the create-project UI surfaces the
  server's `PRECONDITION_FAILED` as "This instance has no GitHub App configured".

---

## §4 Desktop IDE: clone lifecycle, git contexts, source control, files, terminals

### 4.1 Clone lifecycle (L12)

New `CloneManager` (in `crates/coding`, beside `git_worktree.rs`, reusing `run_git`,
`ensure_clone`, `set_token_remote`, `TokenUrl` redaction):

- **Auto-clone on project open**: navigating to a project's board kicks a background job if
  `<repos_root>/<owner>/<name>/.git` is missing — mint JIT token (`repositories.
  installationToken`), `ensure_clone`, then a fetch. Progress surfaces in the top-bar git chip
  (spinner + "Cloning <name>… 42%" from `git clone --progress` stderr). Failure → chip shows
  an error state with retry; terminals/run configs degrade as before (cwd `$HOME`, run
  disabled) until the clone exists.
- **Freshness**: `git fetch` on project open, after every pull/push, and on window focus with a
  ≥5-minute debounce. Every network op re-mints the token if the cached one is <5 min from
  expiry and re-runs `set_token_remote` first (tokens die in ~55 min; the remote URL is
  disposable). Ahead/behind = local `rev-list --left-right --count <branch>...origin/<branch>`
  after fetch — no network for the counts themselves.
- **Pull = fetch + `git pull --rebase --autostash`** (an explicit `pull.rebase=false` in the
  user's git config is respected → merge instead). **Push = fetch → auto-rebase if behind →
  push.** Conflicts never auto-abort: the trunk enters **conflict mode** (§4.4) with git's
  markers left in place.

### 4.2 Two git worlds (L14)

The IDE chrome and the issues split git cleanly — there is no context switcher and no mode:

- **Trunk** (the project clone on its default branch) is *the* IDE surface. Git bar,
  source-control screen, file tree, `+` shell tabs, and run configs all act on the trunk,
  always. What the top bar shows is what every panel means.
- **Issue worktrees are Claude's.** No manual push/pull/commit UI exists for them; Claude
  commits, pushes, and opens the PR. Humans meet a worktree inside its issue (§4.8): live
  diff, session badge, and two escape hatches — "Open terminal in worktree" (plain shell tab,
  cwd = worktree) and Claude tasks (§4.9).

Consistency rules binding all v4 UI work:

1. Trunk chrome never displays worktree state; issue surfaces never display trunk state.
2. No hidden git: every Claude-driven git operation runs in a visible, interactive terminal tab.
3. Git state shown in the UI is always derived from the repo on disk (`.git/rebase-merge`,
   `MERGE_HEAD`, porcelain status) — never from session bookkeeping — so it survives app
   restarts and out-of-band fixes.

### 4.3 Top bar (git bar, left of the run bar)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ⌂ Web App ▾    ⎇ main  ⟳  ↓2 ↑1  [Pull] [Push]     ▸ dev-server ▾  ⏹  │ … │
└──────────────────────────────────────────────────────────────────────────────┘
```

`ui/src/git_bar.rs`: static branch chip (the trunk branch — nothing to switch), sync spinner
(clone/fetch in flight, clone progress %), behind/ahead counts, ghost Pull/Push buttons
(disabled with tooltip while cloning). During conflict mode the counts are replaced by an
amber `⚠ N conflicts` chip that navigates to Source Control. The run bar is unchanged and
always runs against the trunk.

### 4.4 Source Control screen (L13)

New `Navigation::SourceControl` center screen (`ui/src/source_control.rs`), trunk-only:

```
┌─ Changes ────────────────────┬─ Diff: src/app.rs ──────────────────────────┐
│ Staged (2)                   │  side-by-side, reuses diff.rs renderer      │
│   M src/app.rs           ▣  │                                             │
│   A src/new.rs           ▣  │                                             │
│ Changes (1)                  │                                             │
│   ? notes.md             □  │                                             │
├──────────────────────────────┤                                             │
│ [ commit message…          ] │                                             │
│ [ Commit ]  [ Commit & Push ]│                                             │
├─ History ────────────────────┤                                             │
│ ● fix login   danny · 2h     │                                             │
│ ● add auth    danny · 1d     │  (selecting a commit shows its diff)        │
└──────────────────────────────┴─────────────────────────────────────────────┘
```

- Changes list from `git status --porcelain=v2 --branch`; checkbox = `git add -- <path>` /
  `git restore --staged -- <path>`; file click → working diff (`git diff [--cached] -- <path>`)
  rendered by the existing side-by-side diff element via a new git-unified-diff →
  diff-model adapter (the PR diff and SCM diff share one renderer).
- Commit: message Input + Commit / Commit & Push (`git commit -m`, then push). Empty-staged ⇒
  buttons disabled. Author identity comes from the user's global git config; if unset, a
  one-time inline prompt writes `user.name`/`user.email` to the **clone-local** config.
- History: `git log --format=<NUL-separated>` (hash, subject, author, relative time), paged
  (200 at a time, "Load more"); selecting a commit shows `git show` diffs per file.
- **Conflict mode**: while a rebase/merge sits paused, the screen leads with a banner —

  ```
  ┌─ Rebase paused — 3 conflicted files ───────────────────────────────┐
  │  ⚠ src/app.rs    ⚠ src/lib.rs    ⚠ Cargo.lock                      │
  │  [⚡ Fix conflicts with Claude]   [Open terminal]   [Abort rebase] │
  └────────────────────────────────────────────────────────────────────┘
  ```

  Conflicted files open their marker diff; **Fix conflicts with Claude** runs a Claude task
  (§4.9) in the trunk; Abort = `git rebase --abort` / `git merge --abort`. Mode entry/exit is
  detected from `.git/rebase-merge` / `.git/MERGE_HEAD`, so the banner clears itself no matter
  who finishes the rebase (Claude, a terminal, another tool) and survives app restarts.
- All git invocations go through a new `crates/coding/src/scm.rs` (status/log/diff/stage/
  commit/push/pull/fetch wrappers + parsers) — argv-only per L5, parsers unit-tested against
  fixture repos created in tests.

### 4.5 File tree + viewer (L15)

- The left dock gets a two-icon rail at its top: **Navigator** (existing sidebar) ⟷ **Files**.
  Files shows the trunk working directory as a gpui-component `tree`:
  lazy-loaded directories, `.git` hidden, gitignored entries dimmed, git status dots
  (M/A/?) on changed files, context-menu "Reveal in file manager" / "Open terminal here".
- Clicking a file opens `Navigation::FileViewer { path }` (trunk-relative): read-only, Tree-sitter
  highlighted (reuse the diff view's `highlighter`), virtualized lines, no editing (L15).
  Binary/oversized (>2 MB) files show a placeholder with size + "Open in terminal".

### 4.6 Terminals & run configs

- `+` shell tab: cwd = trunk clone root; `$HOME` only while the clone doesn't exist yet or on
  non-project screens. Tab header shows the directory name.
- Run configs: `run_root` = trunk clone root, always. Running something inside an issue
  worktree is a power move done from a worktree shell tab, not from the run bar. Trust gate
  unchanged.
- Claude tabs unchanged (cwd = their worktree, keyed by `coding_sessions.id`); "Open terminal
  in worktree" lives in the issue Changes tab (§4.8), never in the IDE chrome.

### 4.7 Local repository management

Settings gains a **Local repositories** section: each clone with disk usage (`du`-style scan,
cached), worktree count, actions: "Prune merged worktrees" (worktrees whose issue's
`prState='merged'` — `git worktree remove` + branch delete, skipped if dirty) and "Remove local
copy" (confirm; blocked while sessions run). No auto-GC in v4.

### 4.8 Issue Changes tab (worktree diffs live in the issue)

Desktop issue detail gets a segmented header: **Details · Changes**. Changes is the *only*
place a worktree is visible:

```
┌  Details │ Changes ────────────────────────────────────────────────────────┐
│  ⎇ exp/EXP-42   ● Claude running    5 files  +120 −34                      │
│  [Open terminal in worktree]                  [⋯ Update from main ·        │
│                                                    Clean up worktree]      │
│ ┌─ files ─────────┬─ side-by-side diff (shared renderer) ────────────────┐ │
│ │ M src/app.rs    │  …                                                   │ │
│ │ A src/new.rs    │                                                      │ │
│ └─────────────────┴──────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

- **Source logic (capability-tiered, same tab meaning on every client)**:
  1. local worktree exists (desktop only) → **live local diff** (`git diff
     origin/<default>...HEAD` plus uncommitted; header: "Local — includes uncommitted");
  2. PR exists → PR diff as today;
  3. branch pushed, no PR → `repositories.branchDiff` (header: "Branch exp/EXP-42 — no PR
     yet");
  4. nothing pushed → empty state naming the live machine: "Being coded on
     <deviceLabel> — Watch / Steer" opening the steer viewer.
  Watch/Steer buttons sit in the tab header whenever a session is running, on all clients.
- **Freshness**: refresh on tab focus, when the session's terminal output goes quiet
  (debounced), and a slow poll while visible. No FS watcher in v4.
- **PR unmergeable** (GitHub reports conflicts): the tab shows "PR conflicts with <default>"
  + **Resolve with Claude** → Claude task (§4.9) in the worktree.
- **Update from main**: Claude task in the worktree ("rebase onto origin/<default>…") for
  refreshing a long-lived issue branch on demand.
- **Clean up worktree** appears once `prState` is merged/closed: `git worktree remove` +
  local branch delete (blocked while a session runs or the tree is dirty, with the reason).
- The standalone PR-diff section folds into this tab — one diff surface per issue.

### 4.9 Claude tasks (the reusable "let Claude handle it" primitive)

`coding::claude_task(cwd, prompt, label)` spawns a one-shot **interactive**
`claude --model <model> --dangerously-skip-permissions` in a terminal-dock tab
(`TabKind::ClaudeTask`), prompt passed as the positional argument. Differences from
`coding::launch`: no `.mcp.json`, no `PROMPT.md`, **no `coding_sessions` row** (not
issue-bound: no steer room, no plan-limit charge), no branch/worktree creation — it runs
where it's pointed. Always visible, always steerable by typing — never a hidden background
job. v4 users of the primitive:

| Trigger | cwd | Prompt sketch |
|---|---|---|
| Fix conflicts (trunk conflict mode) | trunk | "A `git pull --rebase` on `<branch>` stopped on conflicts in `<files>`. Resolve them preserving both sides' intent, run `git rebase --continue` (or `git merge --continue`), verify the project still builds, and do NOT push." |
| Resolve PR conflicts / Update from main (issue Changes tab) | issue worktree | "Rebase this branch onto `origin/<default>`, resolve any conflicts, verify the build, then push with `--force-with-lease`." |

### 4.10 Branch map (stretch — phase R6 only)

A faithful JetBrains-style log graph (arbitrary refs, general lane assignment, graph-side git
actions) is overblown here. The **product-native** version is not: in Exponential every
interesting branch *is* an issue (`exp/*`), so the graph collapses to a trunk spine + one lane
per issue branch — a flight map of in-progress work, not a git log viewer.

```
│ ●  fix: partial-unique upserts …           main
│ ●  feat: wave 2 — §4c run-targets …
│ ├─●  ╮                                      ⎇ exp/EXP-42 · In Progress · ● coding
│ ●  │ ●  refactor: extract scm parsers      (chip → navigates to the issue)
│ ●──╯                                        ⎇ exp/EXP-38 · Done · merged ↩
│ ●  docs: masterplan v4
```

- **Desktop**: the Source Control History pane gains a lane gutter — trunk commits plus
  `exp/*` branches (local worktrees + remote-tracking after fetch): fork point, commits ahead,
  merge-back curve once the PR merged. Branch tips carry issue chips (identifier · status ·
  session badge). Data = local git (`log`, `merge-base`, `for-each-ref`). Only trunk +
  issue-prefix branches — never arbitrary refs, and no git actions from the graph.
- **Web**: a **Branches** view on the project screen, same visual model rendered as SVG, fed
  by `repositories.graph` (§3.2; GitHub API, 60s cache). Issue chips link to issues.
- **Mobile**: skipped this iteration.
- Severable by construction: v4 ships whole without R6.

---

## §5 Web: onboarding, project creation, settings

### 5.1 Onboarding (`components/onboarding/`)

Collapse the two steps into **one**: "Create your first project".

```
┌──────────────────────────────────────────────┐
│  Create your first project                   │
│  Name      [ Web App        ]  Prefix [WEB]  │
│  Color     ● ● ● ● ●                         │
│  ── Repository (required) ──────────────────  │
│  [ GithubRepoPicker: installed repos list ]  │
│  [ + Install the GitHub App… ] (if none)     │
│                        [ Create project ]    │
└──────────────────────────────────────────────┘
```

- One `projects.create` call with the inline `repository: { fullName }`.
- **Remove** the per-step Skip and the global "Skip setup entirely" link — repo-less projects
  no longer exist (invited users never hit onboarding; they land in the shared workspace).
- The App-install CTA keeps the existing round-trip (`/integrations/github/installed` return
  refreshes the picker).

### 5.2 Create-project dialog (`create-project-dialog.tsx`)

Add a repository section under the existing fields — same picker as onboarding (connected
repos first, "Connect another repo…" expands the GithubRepoPicker install/add flow). The
Create button stays disabled until a repo is chosen. Empty-state (no App installed) links to
workspace settings → Repositories.

### 5.3 Workspace settings

- **Repositories section** becomes a pure registry: rows show `owner/name`, default branch,
  install status, and "used by" project chips (from `repositories.list().projects`); actions:
  connect (unchanged), archive, remove (disabled with tooltip while in use). Star/primary UI
  and per-project link editing are **deleted**.
- **Projects section**: each project row shows its repo (`owner/name` chip) and an owner-only
  "Change repository…" action → dialog with the same picker calling `projects.setRepository`.

### 5.4 Issue Changes tab (web)

The issue detail's PR-diff section becomes a **Changes** tab, tiers 2–4 of the §4.8 source
logic: PR diff (as today) → `repositories.branchDiff` when the branch is pushed but has no PR
→ "Being coded on <deviceLabel> — Watch / Steer" empty state opening the existing xterm.js
steer viewer. Watch/Steer buttons in the tab header while a session runs. No git operations
of any kind on the web (L18).

---

## §6 Mobile: iOS & Android adjustments

Mobile stays a coordination surface (no cloning, no git). Changes are model-parity + creation
flow:

- **Both platforms**
  - Project creation (wherever it exists) gains a required "Repository" selector listing
    already-connected repos (`repositories.list`). Empty state: "Connect a repository in the
    web app first" (Android; iOS may offer add-by-name below). No skip.
  - Settings repositories section: drop primary-star / per-project link UI; show "used by"
    project chips; remove blocked-in-use handling mirrors the web copy.
  - Issue detail's PR-diff section is titled **Changes** to match desktop, with the §4.8
    tiers 2–4: PR diff → `branchDiff` (pushed, no PR) → "Being coded on <device>" state
    opening the native steer viewer. Mobile never sees local worktrees and does no git ops.
  - Project screens (project header, issue-detail coding section) show the repo name chip —
    the uuid comes off the synced `projects` shape, the name from the repositories API (cache
    per workspace).
  - Sync schema: add `repository_id` to the projects entity/DAO (GRDB migration on iOS, Room
    on Android) — additive column on an existing shape, no shape-count change.
- **iOS**: `RepositoriesApi` drops `linkProject`/`unlinkProject`/`setPrimary`, gains
  `projects.setRepository`; `GithubRepoPicker` reused in create-project.
- **Android**: connecting brand-new repos stays web-only (as today); the selector only offers
  connected repos.

---

## §7 Migration & rollout

Production and staging still run the pre-refactor image; the v2 plan already commits to a
**greenfield DB reset** at release, and v4 folds into that same reset. So the migration burden
is dev/staging only:

1. Drizzle migration: add `projects.repository_id` **nullable** → backfill from
   `project_repositories` (primary link, else sole link) → *(repair)* any project still null
   fails loudly in a listed report → set `NOT NULL` → drop `project_repositories`.
2. Repair script for our own DBs (dev + staging): link repo-less projects to a named repo
   (dogfood: `Niach/exponential`), or delete them. Run before the NOT NULL step.
3. `bun run migrate:generate && bun run migrate`; custom triggers unchanged (no new trigger —
   `repository_id` needs no denormalization, it lives on an already-workspace-scoped table).
4. Shape ride-along: confirm the projects shape proxy has no `columns` pin excluding the new
   column (only `issue-subscribers` pins columns today), and regenerate/extend the three native
   clients' projects row structs.
5. Staging reset per the documented Coolify procedure once R0–R3 are green.

No `contract.json` change (no new enums). No design-token change.

---

## §8 Execution plan — phases R0–R5

Designed for a **dynamic workflow of subagents** (Workflow tool). Orchestration rules:

- **Model**: default every packet agent to `opus`; reserve Fable for adversarial review/judging
  packets only.
- **Contract-first**: R0 lands the schema + server contract before any client work; R1
  (web) ∥ R2 (desktop core) after R0; R3 depends on R2; R4 (mobile) ∥ R2/R3; R5 last.
- Packets are sized for one agent: each names its files, its gate, and must leave
  `bun run typecheck` / `cargo build` green. Agents touching `apps/desktop` run
  `cargo test -p <crate>`; server packets run the vitest suite.
- Every phase ends with a **verify packet** (independent agent): re-run the gate commands,
  grep for leftovers (e.g. `project_repositories`, `linkProject`, `setPrimary`,
  `resolveProjectRepository` old semantics), and exercise the flow per `/verify`.

### R0 — Schema & server contract

| Packet | Scope | Key files |
|--------|-------|-----------|
| R0.a | Schema: `projects.repositoryId`, delete `project_repositories`, types/zod; migration + backfill/repair per §7 | `packages/db-schema/src/schema.ts`, `apps/web/src/db/out/` |
| R0.b | `projects.create` inline-repo + `setRepository`; `repositories` router simplification (`forIssue`, `list.projects`, delete link procs, `remove`→CONFLICT) + `branchDiff` compare proc; vitest coverage | `apps/web/src/lib/trpc/projects.ts`, `repositories.ts` |
| R0.c | Bootstrap + seeds: feedback-workspace repo row, test factories, `.env.example`/README self-host note | `apps/web/src/lib/**bootstrap**`, test helpers |
| R0.d | Shape ride-along audit: projects proxy/collection carry `repository_id`; web `useLiveQuery` typings | `apps/web/src/routes/api/shapes/projects.ts`, `lib/collections.ts` |

**Gate**: fresh-DB migrate green; typecheck + vitest green; creating a project without a repo is
impossible via tRPC; `repositories.remove` on an in-use repo returns CONFLICT.

### R1 — Web surfaces (∥ R2)

| Packet | Scope |
|--------|-------|
| R1.a | Onboarding single-step merge (§5.1), skip removal |
| R1.b | Create-project dialog repo section (§5.2) |
| R1.c | Settings: repositories registry + projects rows "Change repository…" (§5.3) |
| R1.d | Web issue **Changes** tab: PR → branchDiff → watch/steer tiers (§5.4) |

**Gate**: e2e — fresh user onboards through project+repo; create dialog blocks without repo;
settings retarget works; Changes tab renders each source tier.

### R2 — Desktop git core (Rust, ∥ R1)

| Packet | Scope | Key files |
|--------|-------|-----------|
| R2.a | `scm.rs`: status/log/diff/show/stage/commit/push/pull/fetch wrappers + porcelain-v2/log/unified-diff parsers, fixture-repo tests | `crates/coding/src/scm.rs` |
| R2.b | `CloneManager`: auto-clone job + progress events, fetch policy, token re-mint/re-set, ahead/behind | `crates/coding/src/clone_manager.rs`, reuse `git_worktree.rs` |
| R2.c | Trunk state model (ahead/behind, conflict-mode detection from `.git/rebase-merge`/`MERGE_HEAD`) + `claude_task` primitive; `run_root`/shell-cwd rewiring to trunk | `crates/coding/src/`, `crates/ui/src/state` |
| R2.d | git-diff → diff-model adapter so `diff.rs` renders working/commit diffs | `crates/ui/src/diff.rs` (+adapter) |

**Gate**: `cargo test` green incl. fixture-repo parser tests; headless: open project →
clone appears under repos root; ahead/behind + conflict-mode detection correct against a
fixture remote.

### R3 — Desktop UI (after R2)

| Packet | Scope |
|--------|-------|
| R3.a | Git bar (trunk chip, counts, Pull/Push, clone progress, conflict chip) next to the run bar |
| R3.b | Source Control screen (changes/stage/commit/history/diff) + conflict-mode banner + Fix-with-Claude wiring |
| R3.c | Files rail: left-dock toggle, tree with status dots, read-only viewer screen |
| R3.d | Terminal `+`/run-bar trunk cwd, settings "Local repositories" (§4.7) |
| R3.e | Issue **Changes** tab: segmented header, local-diff/PR-diff source logic, Open-terminal-in-worktree, Resolve-PR-conflicts/Update-from-main + cleanup actions; fold the standalone PR-diff section in |

**Gate**: build green both OSes; manual checklist — clone→terminal-in-repo→edit→stage→commit→
push→pull round trip; manufactured rebase conflict → Fix with Claude resolves it and the banner
clears; issue Changes tab shows the live worktree diff while Claude codes.

### R4 — Mobile parity (∥ R3)

| Packet | Scope |
|--------|-------|
| R4.a | iOS: entity migration, create-project repo selector, settings simplification, repo chips |
| R4.b | Android: same, selector limited to connected repos |

**Gate**: both build; project creation without repo impossible; settings render 1:1 model.

### R5 — Polish & release alignment

Prune-merged-worktrees action, `vision.md` refresh (four-surface + IDE story), CLAUDE.md sync,
staging reset + smoke, steer-relay Coolify app creation (carried over from v2 Phase 4 remains),
then the v2 Phase-10-style release gate (greenfield reset, tags, channels).

### R6 — Branch map (optional stretch)

Desktop history lane gutter + web project Branches view + `repositories.graph` (§4.10).
Explicitly severable: v4 ships whole without it; start only when R0–R5 are green.

**Carried-over open items** (pre-v4 debts that ride along in R5 unless picked earlier):
wss:// TLS in the desktop WS client + publisher auto-reconnect + relay-unreachable kill
honoring; live relay-presence device pickers; native `#`-autocomplete; digest-batching cron;
Android release signing; macOS notarization.

---

## §9 Do-not-regress contract

Inherited invariants that every v4 packet must respect:

1. **Shape lockstep**: 14 shapes, 14 proxies, all four clients — adding a column to a synced
   shape means updating web collections + iOS GRDB + Android Room + desktop sync structs in the
   same phase. Never widen a proxy's pinned `columns` allowlist from the client.
2. **Proxy hardening**: `cache-control: private, no-store` + vary; explicit 401 on bad token
   creds; sorted-id where clauses (shape-identity stability).
3. **JIT tokens**: never persisted beyond the remote URL, never logged (`TokenUrl` redaction);
   session-gated mint only.
4. **argv git only** (L5); `.mcp.json`/`PROMPT.md` stay git-excluded; worktree layout
   `<clone>.worktrees/<branch,'/'→'-'>` is load-bearing (launcher reuse + prune logic).
5. **One entry point**: local Start coding and relay `start_session` both go through
   `coding::launch`. Claude tasks (§4.9) are a *separate* primitive — they must never create
   `coding_sessions` rows, steer rooms, or `.mcp.json` files.
6. **Terminal**: single PTY read thread, steer tee ordering (sink → processor → wake),
   reply-required terminal events, drop-`slave` after spawn.
7. **Run-config security**: server strips env keys; desktop mirrors validation; Trust & Run
   re-prompts on command-set-hash change; argv-direct spawn.
8. **Markdown byte-parity** (Android tests are the lock); mention/image forms unchanged.
9. **Widget users**: never delete a config's `isAgent` user (`issues.creator_id` cascades).
10. **Model explicitness**: spawned `claude` always gets `--model` (default `opus`).
11. **Deploy realities**: Coolify is home-LAN-only (manual redeploys); staging/prod DBs are
    not redeployed until the release-phase greenfield reset; use `git pushsync`, never bare push.
