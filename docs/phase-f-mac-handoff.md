# Phase F — Mac Handoff (iOS + macOS Swift)

**Status:** Phases A–E are complete on **web / Android / Linux / Rust**. All Swift
(iOS + macOS) work was deliberately batched into this single phase because it
can't be built in the Linux dev environment. This doc is the authoritative,
self-contained task list — every item points at the **reference commit + files**
on the already-done platforms so you can `git show <sha>` and mirror it.

Companion docs (still valid for the agent runtime): `docs/native-desktop-roadmap.md`,
`docs/macos-agent-first-handoff.md`, `docs/macos-ui-parity-plan.md`. This file
supersedes them for the A–E catch-up.

## Reference commits (read these first)

```
974a177  Phases A–C: agent_runs, jsonb→text, enum promotion, companion→agent,
         contract codegen, permission consolidation (lib/auth/access.ts)
104459f  Phase D: packages/design-tokens + Android color parity
45bc7ea  Phase E: filters-in-URL (web) + REMOVE native admin (Android, Linux)
94fcbb4  Phase E: rich markdown comment composer (Android + Linux)
9f78023  Phase E: @mention autocomplete in native editors (Android + Linux)
e157b6a  Phase E: create-workspace + create-project UI (Android)
16adc06  Phase E: PR inline diff viewer (Android)
4ab404d  Phase E: GitHub repo link/unlink UI (Android)
0302f3e  Phase E: richer activity-event rendering (Linux)
717173f  Phase E: inbox / notifications view (Linux)
818818a  Phase E: PR inline diff viewer (Linux)
```

The canonical schema + contract (the source of truth all clients mirror):
`packages/db-schema/src/schema.ts`, `packages/domain-contract/contract.json`.

---

## Global rules / decisions (locked — do not re-litigate)

- **Keep the macOS glass design.** Do NOT replace SwiftUI materials
  (`.ultraThinMaterial`, `.glassSection()`) with custom overlays. Only align
  *semantic* colors (status/priority) to the shared tokens.
- **Agent stays a moderator.** The permission consolidation was explicitly
  *no-behavior-change*. Don't "close the agent bypass" — the agent legitimately
  writes status. `WorkspacePermissions` role rules are unchanged.
- **jsonb `{text}` → plain markdown text** for `issues.description` and
  `comments.body`. WRITE sites must now send a plain `String` (not `{text:…}`);
  READ sites must stay tolerant of the legacy `{text:…}` shape (a record synced
  before the migration). Mirror the Android/Linux Phase-A catch-up.
- **`agent_runs.planText` / `.question` STAY jsonb `{text}`** (server-authored) —
  unwrap them like a comment body.
- **Verify-before-implement.** The plan's parity matrix (`§2` of the plan file)
  is unreliable — on Android/Linux ~4 "gaps" turned out already-done. Check the
  actual current Swift state of each item before building it.
- **Build/verify after each area:** `cd apps/ios && tuist generate` then build the
  iOS + macOS schemes (Xcode or `xcodebuild`). After contract/token edits, run the
  codegen (below) and `git diff --exit-code` the generated files.

---

## F0. Regenerate the codegen for Swift (do this first)

Both generators already include Swift output and were run for A–E; re-run to be
sure the Swift constants are current, then commit any diff:

```bash
bun run --filter @exp/domain-contract generate   # → ExpCore/Sources/Domain/DomainContract.generated.swift
bun run --filter @exp/design-tokens  generate     # see F5 — Swift target must be ADDED first
```

`DomainContract.generated.swift` already carries the new enums + the
`moderationRestrictedFields` list (added in Phase A/C). The **design-tokens
generator currently only emits Kotlin** — F5 adds the Swift target.

---

## F1. Data-model adoption (ExpCore) — the foundation

Canonical schema: `packages/db-schema/src/schema.ts`. Reference: `974a177`
(web `apps/web/src/lib/collections.ts`, the `agent_runs` shape proxy, and the
Android/Linux catch-up in the same commit + follow-ups).

Target files: `apps/ios/ExpCore/Sources/DB/Entities.swift`,
`…/DB/DatabaseManager.swift` (GRDB migrations), `…/Electric/SyncManager.swift`
(shape list, currently 13 shapes at ~L171–183).

1. **`agent_runs` — add the 14th synced shape.**
   - SyncManager: add `name: "agent-runs", path: "/api/shapes/agent-runs", table: "agent_runs"`.
   - Entities.swift: add an `AgentRun` GRDB record (PK `issueId`): `planState,
     planRevision, planText (jsonb {text}), question (jsonb {text}), questionAskedAt,
     approvedAt, approvedBy, runMode, sessionId, interactiveClaimedAt,
     interactiveClaimedExpiresAt, prUrl?, prNumber?, prState?, branch?, prMergedAt?,
     lastError, workspaceId`. (Match `packages/db-schema/src/schema.ts` `agentRuns`.)
   - DatabaseManager: add the `agent_runs` table migration.
   - **Plan Panels read from the synced shape**, not the `agentPlan.getState`
     round-trip: `AgentPlanPanel.swift` + `MacAgentPanel.swift` currently fetch via
     `AgentPlanApi.getState` (`ExpCore/Sources/API/AgentPlanApi.swift`). Switch them
     to observe the local `agent_runs` row (plan/question/revision/approvedBy).
     `getState` stays as a fallback proc — fine to leave the API method.
2. **Drop the now-stale agent columns from the local `Issue` record** (they moved
   to `agent_runs`): `agentPlanRevision, agentPlanApprovedAt/By, agentLastCommentSeenAt,
   agentSessionId, agentRunMode, agentInteractiveClaimedAt`. **KEEP** `agentPlanState`
   + the `pr*` summary cols on `issues` (the server kept them). NOTE: Android/Linux
   deliberately left these stale-but-harmless "to be dropped together in Phase F" —
   so this is the coordinated cleanup; dropping them on Android/Linux too is optional.
3. **jsonb `{text}` → plain text** for `issues.description` + `comments.body`: WRITE
   sites send `String`; READ stays tolerant (there's likely an existing
   `getCommentBodyText`/description extractor — keep it tolerant). Reference: the
   web `apps/web/src/lib/domain.ts` (`getIssueDescriptionText`/`getCommentBodyText`).
4. **`comments` — drop `kind` + `answeredAt`** (or leave with a default, like
   Android did — graceful: synced rows just won't have them).
5. **Enum promotions** (prState/runMode/issueEvents.type/subscriberSource → pgEnum):
   **no-op for natives** — Electric still sends the same strings.

Acceptance: app syncs the new schema; Plan Panel renders plan/question with **no
`agentPlan.getState` network call** (check the network log).

---

## F2. companion → agent rename

Reference: `974a177` (web `routes/api/trpc/$.ts` mounts `agent.*` primary +
`companion.*` temporary alias; agent-core `trpc.rs`; Linux `heartbeat.zig`/
`registration.zig`).

The server alias is temporary. Update all iOS/macOS `companion.*` call sites to
`agent.*` (register/heartbeat/repoToken/pollControl/uninstallSelf). Search:
`grep -rn "companion\." apps/ios`. Key files: the agent registration/heartbeat in
ExpCore + `apps/ios/ExponentialMac/MacAgentService.swift`.

---

## F3. Permission consolidation → `WorkspacePermissions.swift`

Reference: `974a177` (`apps/web/src/lib/auth/access.ts` — the canonical predicates;
`apps/web/src/hooks/use-workspace-permissions.ts`; Android
`domain/WorkspacePermissions.kt`).

`apps/ios/ExpCore/Sources/Domain/WorkspacePermissions.swift` already exists. It's a
*client-side mirror* of the role rules (the server enforces). Verify it matches the
consolidated semantics (no behavior change): `isModerator = member || admin`;
`canCreate` (public+everyone ⇒ any authed, else member); `canMutateIssue` (member,
or public+creator, or admin); `canApprovePlan` (creator || owner). Fix any stale
comment that references the old `assertCan*` helpers → now `assertIssueAccess(...)`
in `lib/auth/access.ts` (I made the same comment fix on Android in `45bc7ea`).
The restricted-field list is `DomainContract.moderationRestrictedFields` (generated).

---

## F4. Remove the admin console (iOS + macOS) — net deletion

Reference: `45bc7ea` (deleted Android `AdminApi.kt` + admin screens + nav/settings
wiring; Linux `ui/admin.zig` + the user-menu Admin button). **Locked decision:
admin = web-only.**

Delete: `apps/ios/ExpCore/Sources/API/AdminApi.swift`,
`apps/ios/Exponential/UI/Admin/AdminUsersView.swift`,
`apps/ios/Exponential/UI/Admin/AdminWorkspacesView.swift`,
`apps/ios/ExponentialMac/MacAdminView.swift`, and their nav/settings entry points
(`SettingsView.swift`, `MacShell.swift`/`AppNavigator.swift`, `MacWorkspaceSettingsView.swift`).
**KEEP the `isAdmin` flag** everywhere (auth `ServerAccount`/`AuthRepository` +
comment-moderation use) — only the *console UI* is removed.

---

## F5. Design tokens → Swift (add the target, then adopt)

Reference: `104459f` (`packages/design-tokens/` — `tokens.json`, `scripts/generate.ts`
with the OKLCH→sRGB converter; Android `DesignTokens.generated.kt`).

1. **Add a Swift emit target** to `packages/design-tokens/scripts/generate.ts`
   (next to `emitKotlin`): emit `DesignTokens.generated.swift` with
   `Color(red:green:blue:opacity:)` (or a hex initializer) for palette + semantic,
   and `CGFloat` for radius/size. The OKLCH→sRGB `parseColor` is already there;
   reuse it. Write to e.g. `apps/ios/ExpUI/Sources/DesignTokens.generated.swift`.
2. **Adopt for semantic colors only** in `apps/ios/ExpUI/Sources/GlassTheme.swift`
   + any `StatusColors`-equivalent (the Android `StatusColors.kt` mapping in
   `104459f` is the reference). **Do NOT touch the glass materials.**
3. Re-run `bun run --filter @exp/design-tokens generate`; commit the generated file.

(There's a web drift test `apps/web/src/lib/design-tokens.test.ts` proving
`tokens.json` ≡ web `styles.css` — the palette is already authoritative.)

---

## F6. Parity features (Swift). Verify current state before building each.

Per-feature reference impl + the Swift target. The web is the spec.

| Feature | Reference commit (mirror) | iOS/macOS target | Notes |
|---|---|---|---|
| **Rich markdown comment composer** | `94fcbb4` (Android `CommentThread.kt` + Linux `commentComposer` reuse the block editor for the composer + inline edit; image upload routed to issue attachments) | `Exponential/UI/Issue/CommentThreadView.swift` + `Exponential/UI/Markdown/MarkdownEditor.swift` (iOS); `ExponentialMac/MacMarkdownEditor.swift` (macOS ◐→●) | Reuse the existing block `MarkdownEditor` in the composer; comment images reuse the issue image-upload path (no server change). |
| **@mention autocomplete** | `9f78023` (regex `(?:^\|\s)@([A-Za-z0-9._%+-]*)$`, filter non-agent members, insert `@<email>`; web ref `apps/web/src/components/mention-textarea.tsx`) | the Swift editor(s) | Insert the canonical `@email` form; render known members as name pills (already done on read). |
| **Create project + create workspace UI** | `e157b6a` (Android `CreateWorkspaceSheet`/`CreateProjectSheet` + `Home` "+ " entries; per-account) | iOS Home/sidebar | The plan notes the **APIs already exist on iOS**; this is UI only. Project form = name + auto-derived prefix + `LabelPalette` color. |
| **PR inline diff viewer** | Android `16adc06` (web-mirror via `issues.prFiles` query) + Linux `818818a` (worker + render) | iOS `DiffView` (macOS ● already has it) | Call the `issues.prFiles` **query** (`AgentPlanApi`-style). Web ref: `apps/web/src/components/diff-view.tsx` (`lineClass()` + FilePatch). |
| **Finish Google Calendar (iOS)** | server `apps/web/src/lib/google-calendar.ts` + `lib/trpc/integrations.ts` | iOS integrations | iOS is a stub; wire real connect/status/backfill. (Calendar was "redesign & finish" per the plan — note: server calendar-sync currently still on the `issues` table; the `calendar_sync_links` redesign in the plan §4.3 was NOT done in A–E, so confirm the server shape before building iOS.) |
| **Invite-accept deep link (macOS)** | iOS already has `InviteAcceptView.swift`; Linux is also missing it | `ExponentialMac` | macOS ○ → mirror the iOS invite-accept (`workspaceInvites.getByToken` + `.accept`). |
| **GitHub repo link/unlink (iOS finish)** | `4ab404d` (Android `ProjectsApi.link/unlinkGithubRepo` + owner-gated dialog) | iOS project/issue-list | iOS is ◐ — finish link/unlink, owner-gated. |
| **Richer activity-event payloads** | `0302f3e` (Linux: select payload + render status from→to, PR#, assigned/unassigned) | iOS/macOS timeline | Only if the Swift timeline renders generic verbs — verify first (it may already be rich). |
| **Inbox/notifications** | `717173f` (Linux) | iOS `Inbox/InboxView.swift` already exists | Likely already done on iOS — VERIFY, probably no work. |

**filters-in-URL** (`45bc7ea`) is **web-only** — N/A for native.

---

## F7. Final verification on the Mac

- `cd apps/ios && tuist generate`; build **iOS** + **macOS** schemes clean.
- `bun run --filter @exp/domain-contract generate` and
  `bun run --filter @exp/design-tokens generate`; `git diff --exit-code` the
  generated Swift files (no drift).
- Smoke: log in, sync the new schema, open an issue → Plan Panel renders with **no
  `agentPlan.getState` call**; comment with the rich composer + an `@mention`;
  create a workspace + project; open a PR's inline diff; confirm **no Admin UI**;
  confirm the **glass design is unchanged**.
- Run an interactive agent on macOS: confirm `agent.*` (not `companion.*`) calls
  and that `--continue` resumes (session-id capture from `974a177`/agent-core).

---

## Known caveats carried in from A–E

- **Runtime-verification debt:** the Linux Phase-E features (comment composer,
  @mention, richer events, inbox, PR diff) are **build-verified only** — the dev
  backend was down + no login. A live Linux GUI pass is still owed and would also
  de-risk the Swift mirrors.
- **`calendar_sync_links` (plan §4.3) was NOT built** in A–E — Google Calendar
  sync still uses the `issues.googleCalendar*` columns server-side. Decide whether
  to do that server redesign before/with the iOS calendar finish (F6).
- **Linux invite-accept + archive/unarchive** were left undone in Phase E
  (URI-scheme registration is environment-specific; web has no archive UI either).
  Not Swift, but listed so parity tracking stays honest.
