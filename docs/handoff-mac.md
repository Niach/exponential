# Handoff — switching to the MacBook (2026-07-01)

Continuation state for the **masterplan v2 execution** (see `docs/masterplan.md` — the spec; `docs/vision.md` — the north star). This doc is the delta: what's done, what's in flight, exactly where the Mac session picks up. Written at the end of the Linux session that executed Phases 0–3.

## Where we are (phase map)

| Phase | Status | Notes |
| --- | --- | --- |
| 0+1 Delete + greenfield schema | ✅ **committed** | Commit `refactor!: v2 hard cut…` — agent-core/FFI/companion/agent-plan/calendar all deleted (−83k LOC); 14 synced shapes with `coding_sessions`; fresh `0000` migration; all validation green (web typecheck + 83 tests + build; Zig 43/43 + full build; Android assembleProductionDebug). Swift compile deferred to this Mac session. |
| 2 Server contract | ✅ **committed** | repositories router (list/add/remove/link/unlink/setPrimary/forIssue/installationToken), codingSessions router (start/end, returns `{session}`), users personal-API-key procs (mint returns raw key once: `{key,id,name,start,prefix,createdAt}`; list returns `{keys:[…]}`), MCP `exponential_pr_open` + `exponential_issues_update_status` + identifier resolution, repositories-section settings UI (GithubRepoPicker re-homed), webhook links by prUrl AND branch-parse (opened + merged, idempotent), dogfood repo row. Green: typecheck + 88 tests + build:web. |
| 3 Launcher (Linux + macOS) | ✅ **committed** (macOS = build-green-by-inspection; compile on the Mac!) | Linux: credentials.zig, git_worktree.zig, coding_prompt.zig, coding_launcher.zig, play button, settings Coding section — 52/52 zig tests + full build green. macOS: RepositoriesApi/CodingSessionsApi/UsersApi, GitWorktree.swift, MacCodingLauncher.swift (single `start(accountId:issueId:)`, relay-ready), MacCodingSettings (JSON @ MacAppSupport.dir()/coding-settings.json 0600, Keychain = Phase 6 TODO), play button + settings section. |
| 4 Steer relay | 🟢 **done up to live E2E** (2026-07-02 Mac session) | DONE: `packages/steer-ticket` + `apps/steer-relay` (as before) + **web `steer` tRPC router re-derived** (`apps/web/src/lib/trpc/steer.ts` + pure core `lib/steer.ts` + 20 vitest cases: config/mintTicket(control\|publisher\|viewer)/myDevices/startSession/killSession; url returned WITH `?ticket=` embedded) + **macOS client COMPILED green** with all four known issues fixed (slow 15-min disabled recheck, PtyTail deadlock→NSLock flag, real grid cols/rows + live resize frames, config off the launcher critical path w/ 5-min cache; `script -q -t 0` for a live tail — note: BSD script DOES support -F, but does NOT propagate SIGWINCH to the nested PTY → claude keeps spawn-time geometry across local resizes; forkpty helper remains the fallback) + **Linux client** (`src/core/steer/`: RFC6455 ws_client.zig, control_channel.zig w/ persistent deviceId + backoff + 15-min disabled recheck, publisher.zig PTY tee/input-inject/resize/kill/ring-resync; ws:// only — `error.TlsUnsupported` for wss://, LAN relay is the self-host target) + **web viewer UI** (`components/steer-terminal.tsx`: xterm.js watch/steer, presence bar, claim/release, kill, Start-on-my-desktop w/ device picker, "Coding now" badge off the synced shape). REMAINING: iOS/Android native VT viewers (Phase 7), the Coolify `exponential-steer-relay` app (deploy-time), wss:// TLS in the Zig WS client, desktop watching its own `coding_sessions` row to honor server-side kill when the relay is unreachable, publisher auto-reconnect, and a live end-to-end steer session (Phase 6 hardware pass). Wire protocol FROZEN in `apps/steer-relay/src/protocol.ts`. |
| 5 Desktop IDE (run configs, multi-window, Linux parity) | ⬜ (carried-over cleanup ✅) | ~~Carried-over TODO~~ DONE 2026-07-02: Linux `projects.github_repo` remnants swapped to the repositories registry (SQLite column + ProjectRow dropped, repo-banner UI deleted, preview repo resolution via tRPC `repositories.list` primary/sole link on a worker thread, `repoCloneDir` → the launcher's shared `<reposRoot>/<owner>/<name>` clone). Run configs / multi-window / 1:1 parity pass still open. |
| 6 macOS/iOS real-hardware verify | 🟡 **compile gate ✅ (2026-07-02)** — runtime checklist still owed | Both schemes compile green on the real Mac (only 3 real errors in all the Linux-authored Swift — Swift 6 strict-concurrency in MacPreviewBackends.swift). The runtime/hardware checklist below (login, 14-shape sync, launcher E2E, ghostty render, steer E2E) remains open. |
| 7 Coordination clients | ⬜ (carried-over cleanup ✅; web steer viewer ✅) | ~~Carried-over~~ DONE 2026-07-02: Android repo-connect screens swapped to the repositories registry (githubRepo entity field + IntegrationsApi + GithubRepoPicker deleted; Repositories settings section over tRPC, Room v6→v7; connecting new repos links out to web). Web steer viewer UI landed with Phase 4. Remaining: My Issues, iOS/Android VT viewers + Start-on-my-desktop, PR-diff/native repo surfaces per §5. |
| 8 Notifications/email/helpdesk | ⬜ | Schema already in place (`user_notification_prefs`, `email_deliveries`, `issue_subscribers.email`, `widget_submissions.resolvedNotifiedAt`, `pr_opened`/`pr_merged` notification types). |
| 9 Billing/self-host finalize | ⬜ | |
| 10 Final green + release | ⬜ | |

Task list in the session tracks the same phases (#1–#10).

## Mac session — do this first

1. `git pull` (everything is pushed to `origin/master`; pushsync was run — verify tags with `git ls-remote --tags origin` per the standing rule).
2. Bootstrap ghostty + project: `cd apps/ios && ./scripts/setup-ghostty-macos.sh && tuist generate` (libghostty is NEVER built from source on macOS — the script fetches the prebuilt `GhosttyKit.xcframework` into `vendor/`).
3. **Expect compile errors.** All Swift for Phases 0–3 was written on Linux without a compiler (grep/brace-verified only). Build `Exponential` (iOS) and `Exponential-macOS` schemes; fix what falls out. The riskiest areas, in order:
   - `Project.swift`: agent-core build scripts removed; `agentCoreSettings` renamed → `macLinkSettings`; both mac targets now `scripts: [ghosttyBootstrapScript]`. If `tuist generate` fails, start here.
   - New files: `MacCodingLauncher.swift`, `GitWorktree.swift`, `RepositoriesApi.swift`, `CodingSessionsApi.swift`, `MacDiffView.swift`, `MacAppSupport.swift`, `MacToasts.swift`, `MacEventPhrases.swift`, `EventPhrases.swift` (iOS), plus settings additions — **and the Phase 4 steer set: `SteerApi.swift` (ExpCore), `SteerProtocol.swift`, `MacSteerControlChannel.swift`, `MacSteerPublisher.swift`, `MacSteerPtyTail.swift` + steer wiring in MacAppDependencies/MacCodingLauncher/MacCodingSettings/MacGhosttyTerminal (`writeToPty`).**
   - `ExpCore` entities: `CodingSessionEntity` (replaces AgentRunEntity), `IssueEntity` (+`duplicateOfId`, −googleCalendar*/−agentPlanState), `IssueSubscriberEntity` (optional `userId`, +`email`), DatabaseManager cache-key bumped `-v3`→`-v4`.
4. Then run the **Phase 6 checklist** (below).

### Known runtime assumptions baked into the Swift launcher (verify on hardware)
- Worktree layout: clone at `<reposRoot>/<owner>/<name>`, worktrees at `<reposRoot>/<owner>/<name>.worktrees/<branch with / → ->`; re-launch reuses the worktree (one issue = one worktree).
- `claude` resolves via the augmented PATH inside the ghostty script when the setting is the bare `claude`; set an absolute path in settings if the GUI-app PATH misses it.
- PROMPT.md hardcodes MCP tool names `exponential_pr_open` / `exponential_issues_update_status` — these match the server as landed.
- "Regenerate" best-effort revokes the previously stored key id after minting the new one.
- Decode envelopes were aligned to the server post-review (`{session}` for codingSessions.start, `{keys}` for listPersonalApiKeys, nullable `start` on mint) — if a decode fails at runtime, compare against apps/web/src/lib/trpc/{coding-sessions,users}.ts return shapes first.

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
- **macOS steer deviations from §3.3** (libghostty owns the PTY): output teed via `script(1)` + file tail, input injected via `ghostty_surface_text`, resync = raw ring replay. The relay wire format overrides masterplan §3.2's tables (`input.data`, `online.deviceId`, claims `{sub, ws}`, HTTP `POST /start`) — pinned in both docs; `apps/steer-relay/src/protocol.ts` is the source of truth.

## Phase 2 + 3 implementation briefs (IMPLEMENTED — kept as the spec of record for what landed)

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

## What remains of Phase 4 (after Mac verify)

> **2026-07-02 update (Mac session):** items 1–4 below are DONE — web `steer` router re-derived exactly per item 1 (plus a pure `lib/steer.ts` core + 20 tests; `url` is returned WITH `?ticket=` embedded and the Swift `connectURL()` was made idempotent to match), macOS client compiled with all four known issues (a)–(d) fixed, the Linux client landed (`apps/linux/src/core/steer/`: ws_client/control_channel/publisher; ws:// only — wss/TLS deferred), and the WEB viewer UI landed (`steer-terminal.tsx`). Still open from this list: iOS/Android native VT viewers, the Coolify relay app, publisher auto-reconnect, desktop honoring server-side kill via its own `coding_sessions` row, and a live E2E steer session on hardware. `script(1)` verification result: flags valid, `-t 0` added for immediate flush, and BSD script does NOT propagate SIGWINCH → nested claude keeps spawn-time geometry (forkpty helper remains the documented fallback). The section below is kept as the spec of record.

The relay SERVICE is built and tested (see phase table). Remaining, in order:
1. **Web `steer` tRPC router** (apps/web/src/lib/trpc/steer.ts, mount `steer`): `config()` → `{enabled, relayUrl}`; `mintTicket({kind:'control',deviceLabel?} | {kind:'publisher',codingSessionId} | {kind:'viewer',codingSessionId})` — permission check (owner/admin→steer, member→view; publisher must own the session row), sign via `@exp/steer-ticket` (exp = 60s), return `{ticket, url}` or `{disabled:true}`; `myDevices()` → GET `${STEER_RELAY_URL}/devices/${userId}` with `x-relay-secret`; `startSession({issueId, deviceId})` → check repo linked (forIssue) then POST `${STEER_RELAY_URL}/start`; `killSession({codingSessionId})` → set row ended + POST `/sessions/:id/kill`. (A first draft existed and was REVERTED at handoff — re-derive from this spec, don't look for it.)
2. **macOS client: VERIFY, don't rebuild.** It exists in-tree (5 new Swift files + 4 wired edits). Design deviations from masterplan §3.3, all deliberate (libghostty OWNS the PTY on macOS — there is no host PTY master): output tee = spawn claude under `/usr/bin/script -q <rawfile> …` + MacSteerPtyTail file-tail → 0x01 frames; remote input = `ghostty_surface_text` (the same seam a paste uses); resync = raw 256KB ring replay (not a grid snapshot). KNOWN ISSUES to fix when enabling steer (all dormant while the router is missing): (a) MacSteerControlChannel retry-polls steer.config every ≤30s forever — including when the proc is missing (today: a 404 every 30s from every running Mac app) or the instance reports enabled:false; make disabled/NOT_FOUND a slow terminal state (~15min recheck). (b) MacSteerPtyTail.stop() self-deadlocks — it enqueues `stopped=true` on the same serial queue loop() occupies; make `stopped` a lock-protected flag set synchronously (leaks a polling thread + fd per session otherwise). (c) Terminal geometry never wired — `hello` hardcodes 80×24 and sendResize has no callers; derive real cols/rows from the ghostty surface and send resize on change. (d) MacCodingLauncher awaits a steer.config round-trip on EVERY local play-button start (pure latency while the router is missing; cache it or run concurrently with git prep). Also verify `script -q` keeps claude interactive + flushes promptly (BSD script has no -F; fallback = a forkpty helper).
3. **Linux client** (not started): minimal RFC6455 WS client first (feasibility-check zig std TLS/upgrade), then control socket (`online{deviceId,deviceLabel}` → `start_session{issueId}` → coding launcher) + per-session publisher (Linux DOES own the PTY master — tee its read stream, write remote input to the same fd). Protocol source of truth: `apps/steer-relay/src/protocol.ts` + its hub tests as executable spec.
4. **Viewer UI** (Phase 7): web xterm.js SteerTerminal, iOS/Android VT viewers, Start-on-my-desktop buttons, presence bar.
