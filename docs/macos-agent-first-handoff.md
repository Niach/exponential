# macOS handoff — agent-first rework

This summarizes the agent-first UX rework that just landed on `master`
(commits `04fe4aa..f2f0cdf`) and what the **macOS app** (`apps/ios/ExponentialMac`,
sharing `apps/ios/ExpCore`) needs to do to reach parity. The web + Linux desktop
+ Rust `agent-core` + server are done and verified; iOS/Android got event-label
parity. macOS work below could not be built/tested from the Linux box.

Pull `master`, then `bun run --filter @exp/domain-contract generate` is already
run (the generated Swift constants are committed) — just build the Xcode targets.

---

## What changed on the server (the contract macOS consumes)

**Agent plan/question moved off comments into structured server state.**
- New server-only table `issue_agent_state(issue_id, plan_text, question,
  question_asked_at)` — **NOT** an Electric shape (so plan bodies don't sync to
  mobile). The lightweight `issues.agent_plan_state` / `agent_plan_revision` /
  `agent_plan_approved_at` columns are still synced and remain the state driver.
- **New tRPC**:
  - `agentPlan.getState({ issueId })` → `{ planText, question, questionAskedAt,
    state, revision, approvedAt }` — read the plan/question TEXT here (it has a
    fallback to legacy plan/question comments, so it's correct during rollout).
  - `agentPlan.answerQuestion({ issueId, answer })` → records the answer, flips
    the issue to `drafting` (re-plan signal), clears the open question.
  - existing: `agentPlan.approvePlan`, `requestChanges`, `retry`.
- **Notifications**: only action-needed agent events push now —
  `agent_plan_review` (plan ready) and `agent_question` (needs answer), to
  workspace **owners** only. Agent-authored comments no longer fan out. New
  `notification_type` values are in the regenerated contract.
- **New `issue_events`**: `agent_started`, `agent_question`, `agent_answer`
  (already labeled in `MacIssueDetailView.eventVerb` — verify it builds).
- **`agent-core`** (the cdylib the Mac app embeds) now submits questions via
  `exponential_agent_plan_submit { state:'awaiting_answer', question }` and reads
  plan/question from `issues_get`'s `agentPlanText`/`agentQuestion`; the agent
  posts **no** plan/question/error/PR comments. No macOS change needed for this —
  it's inside the shared cdylib — but it means the Mac UI must not rely on agent
  comments going forward (see Plan Panel below).

**Dual-write is intentionally still on.** The server *also* mirrors plan/question
into comments so the current comment-based mobile/macOS UIs keep working. The
drain (removing the dual-write + `getState`'s comment fallback) happens only
**after iOS + Android + macOS ship the native Plan Panel** below.

**Companion/setup contract:**
- `companion.register` is now **idempotent** (re-registering the same workspace
  reuses the agent identity, mints a fresh credential — no duplicate agents).
- `companion.listMine` → the owner's agents across **all** workspaces (for
  surfacing an agent registered against the wrong workspace).
- `companion.setupStatus({ workspaceId })` → `{ hasProject, githubConnected,
  machineRegistered, agentSeen, repoLinked, firstIssueAssignedToAgent,
  dismissed }` — backs a setup checklist (shared by web + desktop).
- `onboarding.dismissSetupChecklist({ dismissed })`.

---

## macOS tasks (in priority order)

### 1. Build + verify (quick)
- Build the iOS + macOS targets. Confirm the regenerated
  `DomainContract.generated.swift` and the new `eventVerb` cases
  (`agent_started`/`agent_question`/`agent_answer`) in `MacIssueDetailView.swift`
  and `Exponential/UI/Issue/CommentThreadView.swift` compile and render.

### 2. Owned-workspace agent registration (P0 parity — important)
On Linux the agent now registers/runs against the user's **own non-public owned
workspace**, not the first synced one (which can be the shared/public workspace,
orphaning the agent). See `apps/linux/src/core/db/database.zig:defaultOwnedWorkspaceId`.
- In `MacAgentService.swift` / wherever the Mac picks the workspace to register
  (`MacWorkspaceSettingsView` register action), ensure it resolves the **oldest
  workspace where the current user is `owner` AND `is_public = false`** — not a
  "first workspace" heuristic. Mirror the SQL: join `workspace_members`(role=
  'owner') to `workspaces`(is_public=false), order by created_at.
- Surface orphans: call `companion.listMine`, and in `MacWorkspaceSettingsView`
  show agents registered in a *different* workspace with a Revoke action (see
  the web `agents-section.tsx` amber card for the pattern).

### 3. Onboarding parity (P4)
- First-run/welcome: mention agents ("…then let a coding agent open pull
  requests for you") and add a **"Set up coding agent"** entry that opens the
  agent-register flow (mirror `apps/linux/src/ui/app.zig:showOnboarding`).
- Optional but recommended: a setup checklist driven by `companion.setupStatus`
  (project → connect GitHub → register agent → assign first issue), mirroring the
  web `SetupChecklist` + `/w/$slug/setup-agent` route.

### 4. Unregister confirmation (P5)
- Wrap the macOS "Unregister this machine" action in a confirm dialog (Linux:
  `apps/linux/src/ui/settings.zig:onUnregister`). Accidental unregister silently
  stops the agent.

### 5. Native Plan Panel — the big one (unblocks the dual-write drain)
Today `MacIssueDetailView.swift` (≈lines 833–871) renders plan/question from
**comments** and hosts Approve on the plan comment. Replace this with a
first-class panel (the web equivalents are `apps/web/src/components/
agent-plan-panel.tsx` + `agent-activity-feed.tsx`). `MacAgentPanel.swift` already
has the state chip — extend it (or add a sibling) to:
- Read `issue.agentPlanState` for the state; fetch plan/question **text** via
  `agentPlan.getState` (add a method to `MacAgentPlanApi`/the agent-plan API
  alongside `approvePlan`).
- States: `drafting` → "working on a plan…"; `awaiting_approval` → plan markdown
  + **Approve / Request changes**; `awaiting_answer` → question + an inline
  **answer box** calling `agentPlan.answerQuestion`; `approved` → approved badge
  + "implementing…"; latest `agent_error` event → error + **Retry**.
- Add a quiet **agent activity feed** (events `agent_started`/`plan_ready`/
  `agent_question`/`agent_answer`/`pr_opened`/`pr_merged`/`agent_error`) separate
  from the human comment thread, and **stop rendering `kind=='plan'`/`'question'`
  comments** in the thread.
- Do the same on iOS phone (`Exponential/UI/Issue/CommentThreadView.swift`) and
  Android (`ui/issue/CommentThread.kt`). **Once all three ship this**, ping me to
  remove the server dual-write (`agent-plan.ts submitPlan` comment writes +
  `getState` comment fallback + the `question` kind in `comments_create`).

### 6. Deferred desktop-runtime UX (applies to macOS + Linux)
These need a running desktop to test (couldn't do from Linux CI):
- **Online/offline status indicator** for the agent (heartbeat success/failure,
  debounced ~10s so jitter doesn't flash offline).
- **Run-start toast** when the agent begins a run.
- **Structured `repo_token_unavailable` run error** surfaced clearly ("This
  issue's project isn't linked to a connected GitHub repo — connect GitHub and
  link a repo, then reassign"), with a jump to the GitHub-connect step. The
  agent-core side reports needs_human via the `agent_error` event already.
- A dedicated guided **agent-setup assistant** (register → connect GitHub →
  assign an issue), reading `companion.setupStatus`.

---

## Verification (per area)
- Build iOS + macOS; smoke-test an issue with `agentPlanState` set in each state.
- Register a machine on macOS → confirm it lands in the **owner's** workspace and
  shows in web Settings → Agents (the P0 bug was wrong-workspace registration).
- Assign an issue to the agent → plan appears (via comments today; via the Plan
  Panel once #5 ships), owner gets a single "plan ready" notification.
- Answer a question → agent re-plans; PR open/merge are quiet activity entries.

See also `docs/native-desktop-roadmap.md` (architecture + A1–A5 macOS plan) and
the approved rework plan for the full phase breakdown.
