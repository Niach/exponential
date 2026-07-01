# Handoff — switching to the MacBook (2026-07-01)

Continuation state for the **masterplan v2 execution** (see `docs/masterplan.md` — the spec; `docs/vision.md` — the north star). This doc is the delta: what's done, what's in flight, exactly where the Mac session picks up. Written at the end of the Linux session that executed Phases 0–3.

## Where we are (phase map)

| Phase | Status | Notes |
| --- | --- | --- |
| 0+1 Delete + greenfield schema | ✅ **committed** | Commit `refactor!: v2 hard cut…` — agent-core/FFI/companion/agent-plan/calendar all deleted (−83k LOC); 14 synced shapes with `coding_sessions`; fresh `0000` migration; all validation green (web typecheck + 83 tests + build; Zig 43/43 + full build; Android assembleProductionDebug). Swift compile deferred to this Mac session. |
| 2 Server contract | ⬜ **briefed, NOT started** — full brief below | repositories router + codingSessions router + personal-API-key procs + MCP `exponential_pr_open`/`update_status` + repositories settings UI + webhook branch-parse linking. |
| 3 Launcher (Linux + macOS) | ⬜ **briefed, NOT started** — full briefs below | Zig + Swift "Start coding" launchers, desktop settings (CLI path / repos root / branch prefix / personal key). |
| 4 Steer relay | ⬜ not started | Design pinned in masterplan §3. **No files written yet** (`apps/steer-relay` does not exist). |
| 5 Desktop IDE (run configs, multi-window, Linux parity) | ⬜ | Carried-over TODO recorded in task list: Linux still has `projects.github_repo` remnants in local SQLite + repo-banner/preview infra to swap to the repositories registry. |
| 6 macOS/iOS real-hardware verify | ⬜ **← THE MAC SESSION STARTS HERE** (Phase 0+1 Swift compile-fix + checklist; Phases 2/3 can run in parallel from the briefs below) | Checklist below. |
| 7 Coordination clients | ⬜ | Carried-over: Android repo-connect screens still read the deleted `projects.githubRepo` (WorkspaceSettingsScreen:187/248, IssueListScreen:160) — replace with repositories tRPC here. |
| 8 Notifications/email/helpdesk | ⬜ | Schema already in place (`user_notification_prefs`, `email_deliveries`, `issue_subscribers.email`, `widget_submissions.resolvedNotifiedAt`, `pr_opened`/`pr_merged` notification types). |
| 9 Billing/self-host finalize | ⬜ | |
| 10 Final green + release | ⬜ | |

Task list in the session tracks the same phases (#1–#10).

## Mac session — do this first

1. `git pull` (everything is pushed to `origin/master`; pushsync was run — verify tags with `git ls-remote --tags origin` per the standing rule).
2. Bootstrap ghostty + project: `cd apps/ios && ./scripts/setup-ghostty-macos.sh && tuist generate` (libghostty is NEVER built from source on macOS — the script fetches the prebuilt `GhosttyKit.xcframework` into `vendor/`).
3. **Expect compile errors.** All Swift for Phases 0–3 was written on Linux without a compiler (grep/brace-verified only). Build `Exponential` (iOS) and `Exponential-macOS` schemes; fix what falls out. The riskiest areas, in order:
   - `Project.swift`: agent-core build scripts removed; `agentCoreSettings` renamed → `macLinkSettings`; both mac targets now `scripts: [ghosttyBootstrapScript]`. If `tuist generate` fails, start here.
   - New files: `MacCodingLauncher.swift`, `GitWorktree.swift`, `RepositoriesApi.swift`, `CodingSessionsApi.swift`, `MacDiffView.swift`, `MacAppSupport.swift`, `MacToasts.swift`, `MacEventPhrases.swift`, `EventPhrases.swift` (iOS), plus settings additions.
   - `ExpCore` entities: `CodingSessionEntity` (replaces AgentRunEntity), `IssueEntity` (+`duplicateOfId`, −googleCalendar*/−agentPlanState), `IssueSubscriberEntity` (optional `userId`, +`email`), DatabaseManager cache-key bumped `-v3`→`-v4`.
4. Then run the **Phase 6 checklist** (below).

## Phase 6 real-Mac verification checklist (masterplan §4f + agent additions)

Dev backend: this repo's compose stack, or point at `next.exponential.at`. On a fresh local DB: `bun run backend:up && bun run migrate` — custom triggers are ALSO auto-applied by the web app at boot (`bootstrap-cloud.applyCustomSql`), and `0002_public_workspace.sql` no longer exists (deleted; greenfield schema has no singleton-public index).

- [ ] `tuist generate` succeeds (validates GhosttyKit path); both schemes compile; macOS links with `macLinkSettings`.
- [ ] Login + all **14 shapes** populate (incl. `coding_sessions`); `-v4` local DB purges v3 and re-syncs cleanly.
- [ ] CRUD round-trips (create/edit/status/priority/assignee/labels/comment) via tRPC + Electric; `duplicate` status renders (icon doc.on.doc, grouped after cancelled).
- [ ] GFM editor round-trips byte-identically incl. images/@mentions.
- [ ] PR section renders on issue detail (both platforms) when `prUrl` set; MacDiffView/DiffView load `issues.prFiles`.
- [ ] **Launcher end-to-end** (needs GitHub App configured + a repo linked in workspace settings → Repositories): play button → repo resolve → JIT token → clone/worktree `exp/<IDENT>` + token remote → `.mcp.json` with personal key (mint in settings → Coding) → `claude --dangerously-skip-permissions` in MacTerminalRunner ghostty window → Claude plans, implements, pushes, calls `exponential_pr_open` → PR fields land on the issue → `coding_sessions` row goes running→ended.
- [ ] Preview/run path still works after the rename (`MacAgentTerminalRunner`→`MacTerminalRunner`) and scratch-dir move (MacAppSupport.dir).
- [ ] ghostty gotchas hold: surface only mounts at nonzero size; `GHOSTTY_ACTION_RENDER` handled; app runs with `disable-library-validation` kept — then try REMOVING it (it existed for the deleted agent-core dylib; GhosttyKit is static — if the app still runs, delete the entitlement).
- [ ] iOS: build green, 14-shape sync, editor parity, PR diff section.
- [ ] Glass aesthetic unregressed.

## Key decisions taken (deviations/pins vs masterplan text)

- **Personal API keys**: Better Auth apiKey plugin KEPT; prefix `expk_` → **`expu_`**; mint/list/revoke procs on the users router (Phase 2). The deleted thing is only the desktop-agent expk_ flow.
- **Widget bot user**: `users.isAgent` column KEPT as the bot marker (sanctioned by §8e); bot's workspace membership is now `role='member'` (the `agent` role is deleted); all filtering (member lists, fan-out, mentions, seat counts) keys off `isAgent`.
- **No `in_review` status** (masterplan §7c mentions it but §2.8 is canonical): "in review" = `prState='open'`. MCP `exponential_issues_update_status` accepts `in_progress` | `done` only.
- **repositories table** follows §7a shape (`fullName`, `defaultBranch`, `private`, `installationId`, `sortOrder`, `archivedAt`), not §2.3's owner/name/branchPrefix/mergeStrategy variant. Branch prefix is a desktop setting.
- **`issues.description` / `comments.body`** are plain `text` GFM (the jsonb `{text}` unwrap had already landed pre-refactor; CLAUDE.md now reflects it).
- **`users.setupChecklistDismissedAt` dropped** with the setup-checklist feature.
- **`0002_public_workspace.sql` deleted** (obsolete on greenfield); `bootstrap-cloud.ts` now applies only `0001_triggers.sql`.
- **Branch→issue webhook linking** matches `/(?:^|\/)([A-Z0-9]+-\d+)$/` against the head branch so custom prefixes work.

## Phase 2 + 3 implementation briefs (written, NOT started — execute these next)

The locked cross-cutting tRPC contract both phases share:
- `repositories.forIssue({issueId})` → `{repositoryId, fullName, defaultBranch} | null` (query)
- `repositories.installationToken({repositoryId})` → `{token, fullName, defaultBranch, expiresAt}` (mutation, session-gated, never persisted)
- `codingSessions.start({issueId, deviceLabel?})` → row; `codingSessions.end({id})` (idempotent)
- `users.mintPersonalApiKey({name?})` → `{key, id, start}` raw once; `users.listPersonalApiKeys()`; `users.revokePersonalApiKey({id})` (Better Auth apiKey plugin, `expu_` prefix)

### Phase 2 (apps/web) — masterplan §7
1. `lib/trpc/repositories.ts` mounted as `repositories`: `list({workspaceId})` member-readable with project links; `add({workspaceId, fullName, defaultBranch?, private?, installationId?})` owner/admin, validate App installed via `resolveRepoInstallationToken` else PRECONDITION_FAILED; `remove`; `linkProject`/`unlinkProject`/`setPrimary` (setPrimary clears previous primary in one tx — partial unique index exists); `forIssue` (primary link, else sole link, else null); `installationToken` (member of repo's workspace; expiresAt ≈55min).
2. `codingSessions` router: `start` verifies issue access, sets userId from session + workspaceId explicitly (trigger also populates); `end` only by owner, idempotent.
3. Personal-API-key procs on the users router (see contract above; raw key returned exactly once).
4. MCP (`lib/mcp/tools.ts`): `exponential_issues_get` resolves by human identifier too; ADD `exponential_issues_update_status` (allowed: `in_progress` | `done`; "in review" = prState, document in description); ADD `exponential_pr_open({issueId|identifier, title, body, head?, base?})` — resolve repo via forIssue logic, head defaults `issue.branch` or `exp/<IDENTIFIER>`, base defaults repo defaultBranch, `createPullRequest` (github-pr.ts) with JIT token, ONE tx writes prUrl/prNumber/prState='open'/branch + `pr_opened` issue event (recordIssueEvent); no-repo error: "No repository linked to this project — link one in workspace settings".
5. `components/workspace/repositories-section.tsx` (owner/admin, in settings nav where agents-section was): repo list (fullName/branch/private badge/remove), Connect via existing `GithubRepoPicker` → `repositories.add`, per-project link editor (multi-select + primary star).
6. Webhook (routes/api/webhooks/github.ts + pr-sync.ts): fallback linking by head-branch parse `/(?:^|\/)([A-Z0-9]+-\d+)$/` → repositories row by full_name → linked projects → issue by identifier → `applyPrMergeState`; ALSO on `pull_request.opened`, link un-linked issues (prUrl/prNumber/prState/branch + pr_opened event). Extract parser as pure fn + vitest.
7. Dogfood: `ensureDogfoodProject` (bootstrap-cloud.ts, DOGFOOD_REPO) upserts a repositories row + primary project link.
Gate: typecheck + test + build:web green. Live-PR verification deferred to staging.

### Phase 3 Linux (apps/linux) — masterplan §4a/§4b/§7d
1. `src/core/credentials.zig` — JSON store (account_store.zig pattern): personal API key, claude path override, repos root override, branch prefix (default `exp/`).
2. `src/core/git_worktree.zig` — spawn `git` via argv (never gh): ensureClone(reposRoot, fullName, tokenUrl) clone-or-fetch; createWorktree(clonePath, branch, baseRef) REUSING existing branch/worktree (one issue = one worktree); setTokenRemote → `https://x-access-token:<token>@github.com/<fullName>.git`.
3. Launcher (e.g. `src/ui/coding_launcher.zig`), single fn taking issueId (relay-callable later): forIssue → null ⇒ "Link a repository in workspace settings" state; else installationToken → git ops → write `.mcp.json` `{"mcpServers":{"exponential":{"type":"http","url":"<base>/api/mcp","headers":{"Authorization":"Bearer <personal-key>"}}}}` → write PROMPT.md (plan-first: propose a concise plan and WAIT for go-ahead; then implement; commit, push branch, call `exponential_pr_open`; may use `exponential_issues_update_status`) → spawn `claude --dangerously-skip-permissions "Read PROMPT.md in this directory, then follow it."` cwd=worktree in an embedded ghostty tab (nonzero-size gotcha) → `codingSessions.start` before spawn, `.end` on child/terminal close.
4. Issue-detail "Start coding" play button: enabled iff forIssue resolves (lazy query); running indicator while a session is live.
5. settings.zig "Coding" section: claude path + doctor (`claude --version` + `git --version`), repos root (default `~/Exponential/repos`), branch prefix, personal-key Generate/Regenerate/copy via the users procs.
Gate: zig build check/test/build green; headless unit tests for branch composition, .mcp.json serialization, prompt, token URL. Carried-over: swap the `projects.github_repo` remnants (ProjectRow/repo-banner/preview clone) to the repositories registry while here.

### Phase 3 macOS (apps/ios) — same sequence in Swift
1. ExpCore API clients: RepositoriesApi.swift + CodingSessionsApi.swift + apikey procs (follow IssuesApi/TrpcClient patterns).
2. `ExponentialMac/GitWorktree.swift` — Foundation.Process argv git plumbing (as Linux #2).
3. `ExponentialMac/MacCodingLauncher.swift` — §4a sequence (as Linux #3), spawning into the existing `MacTerminalRunner`/`MacTerminalDock` ghostty infra; `codingSessions.start`/`.end` hooked on the runner's completion path.
4. "Start coding" button in MacIssueDetailView near the PR section; disabled + "Link a repository in workspace settings" help when unresolved.
5. Mac settings "Coding" section (find the settings surface via MacRootView/MacShell): claude path + doctor, repos root (`~/Exponential/repos`), branch prefix, personal-key management (UserDefaults/JSON in MacAppSupport.dir(); Keychain = Phase 6 TODO).
Gate on Linux authoring: grep sweep + brace balance; real gate = compiles on the Mac.

## Environment / loose ends on the Linux machine

- **Stale companion daemon still running on the Linux box**: `bun ~/.local/share/exponential-companion/source/apps/companion/src/cli.ts start` (separate old checkout; the companion server routes no longer exist). Disable its autostart when back on that machine — harmless but noisy.
- Dev DB on the Linux box was greenfield-reset (backend:clear + migrate + triggers). Any other dev DB must be reset the same way — there are NO migrations from the old schema, by design.
- GitHub Actions builds the web image on push to master (`build-issues-web.yml`); Coolify deploys stay manual and LAN-only. Nothing was deployed during this session; staging/production still run the pre-refactor image — do NOT redeploy until Phase 10 (greenfield reset wipes those DBs).
- Remaining known-open items from the pre-refactor era (memory `project_known_open_findings`): resend-verification gap, og:image SVG — untouched.

## What Phase 4 needs (next big build after Mac verify)

`apps/steer-relay` does not exist yet. Full spec: masterplan §3. Pinned decisions from this session's design pass: model on `apps/push-relay` (Hono for HTTP + Bun-native `websocket` handler, export `{port, fetch, websocket}`); ticket = compact HS256 token (payload `{sub, ws, name?, deviceLabel?, sessionId?, role: control|publisher|viewer, perm: view|steer, exp}`) minted by a web `steer` router, verified relay-side with `STEER_RELAY_SECRET`; WS auth via `?ticket=` query param (browsers can't set WS headers); device presence `Map<userId, Map<deviceId, socket>>`; session rooms keyed by `coding_sessions.id` with ~256KB ring buffer; viewer backpressure via `ws.getBufferedAmount()` → drop output frames + request publisher resync → evict after sustained saturation (close code `slow_consumer`); server-to-server secret-authed HTTP endpoints for the web tRPC procs: `GET /devices/:userId`, `GET /sessions/:id` (liveness), `POST /sessions/:id/kill`.
