
# iOS lane: review fixes (T-N + parity), written on Linux, uncompiled

## Files
- `ExpCore/Sources/Domain/ThreadView.swift`:
  - `ThreadListEntry` gains `teamId` + `mine`; `ThreadViews.partition(_:teamId:)` (fixture
    `partitions` incl. `scope`).
  - NEW `ThreadPrCandidate` + `ThreadViews.prTarget(_:)` (fixture `prTargets`).
  - NEW `ThreadViews.sessionGuideEntries(_:sessionId:firstSessionId:)` over the raw jsonb text
    (fixture `guideSlices`).
  - The `at`-window `runResults` and the old `mergeTarget(runs:issues:)` are REMOVED.
  - NEW `ThreadViews.PrTarget` + `prTarget(sessions:issues:)` (candidates = issue + session PRs,
    `at` = updatedAt; the capsule only while open) and `threadGuide(threadResults:sessions:)`.
- `ExpCore/Tests/ThreadViewTests.swift`: every case of `partitions` (with scope), `prTargets`,
  `guideSlices`; prTarget-over-rows and threadGuide tests; the runResults test is removed.
- `Exponential/UI/Thread/ThreadRows.swift`:
  - `liveStatus` (heartbeat rule); `runsForView(now:)` / `buildView(now:)` (the `viewRuns` /
    `view` vars wrap them).
  - `mine(userId:)`, `newestLiveAgent(now:)`, `prTarget`, `guide`.
  - NEW `ThreadLanding` (@Observable box; imports Observation).
- `Exponential/UI/Thread/ThreadModels.swift`:
  - `ThreadRead` (T14: rows + "an item names the id").
  - `ThreadListModel` rewritten: ALL teams, `partition(…, teamId: nil)`, `Entry.mine/liveAgent`,
    `orphans` (T4: my live sessions no `thread_items` row names), a 60 s liveness clock. The
    `activeTeamId` property is gone.
- `Exponential/UI/Thread/ThreadLists.swift`:
  - `AgentThreadLists(model:onOpen:onOpenSession:)`: Running = orphans (SessionListRow →
    `.runThread`) + threads, team bands; Recent = top 3, team headers.
  - NEW `ThreadRunningRow`, `ThreadEntryButton`, `ThreadTeamBandedRows`, `RecentThreadsSheet`.
- `Exponential/UI/Thread/ThreadConversation.swift`: a session block = `sessionGuideEntries` over
  `threadGuide`.
- `Exponential/UI/Thread/ThreadScreen.swift`:
  - `pendingPage` (an issue child held until it syncs), landing via `threadLanding`.
  - `currentThreadId` on the issue page (+ env keys at the end of the file).
  - Guide = `rows.guide`; PR = `rows.prTarget` (diff for open OR merged, capsule only when open).
  - Reply: `.resume` with no machine → info toast "That session ended" + composer; a resumed reply
    with no successor in 30 s gives its text back + error toast with Retry.
- `Exponential/UI/Navigation/AppNavigator.swift`:
  - `threadLanding` state + env.
  - `pushRoute(.thread)` for a thread already on the stack POPS back to it and lands the child.
  - The draft's `onCreated` with a threadId → `landAfterThreadCreate` (thread page with the new
    issue's tab).
- `Exponential/UI/Agent/AgentPageView.swift`:
  - Lists without the activeTeam scope.
  - The history sheet = `RecentThreadsSheet` (rows push `.thread`).
  - A start with `composer.threadId` → `pushRoute(.thread(…, child: .conversation))`.
- `Exponential/UI/Agent/RecentRunsSheet.swift`: DELETED (it was the only user of `pastRows`;
  `AgentsViewModel.pastRows` still computes, unused).
- `Exponential/UI/Issue/IssueFaceView.swift`: the Threads row drops `currentThreadId`.
- `ExponentialUITests/StyleguideScreenshots.swift`: `sg_session-row` opens
  `recent-threads-sheet` and waits for `agent-thread-row`.

## Verify
```bash
cd apps/ios && tuist generate --no-open && tuist test ExpCore && tuist test ExpUI
xcodebuild -scheme Exponential -destination 'platform=iOS Simulator,name=iPhone 17' build
bundle exec fastlane styleguide_screenshots shots:sg_session-row,sg_agent-home,sg_thread,sg_thread-runs
```
Recapture `sg_session-row`: its sheet is threads now. Also `sg_agent-home`, since Recent is now
"mine" across teams.

By hand:
1. T4: start a session, then delete its `thread_items` row locally (or block the shape). The
   session lists under Running as a session row, and a tap opens it (`RunThreadScreen` → Work
   fallback).
2. T6: an account in 2 teams. Running and Recent are banded per team; Recent lists only threads
   with a session of mine or created by me; home Recent never repeats a Running thread.
3. T-N1: the history glyph → the threads sheet, banded; a row opens the thread.
4. T8: a thread with an open PR on an older session and a newer merged one. The Guide's diff and
   capsule are the OPEN one. With only merged PRs: the diff shows, no capsule.
5. T9/T-N5: a `running` row with a heartbeat older than 2 h shows "ended". Reply resumes it; with
   no machine, an info toast + the composer with the text.
6. T10: a resume whose successor never lands. After 30 s the text is back + "Couldn't resume the
   session" with Retry.
7. T11/T-N7:
   - New issue in this thread → Create → back on the SAME thread page (no second one), the new
     issue's tab selected once it syncs.
   - Start a session here / reply-start → the composer → Start → back on the thread's
     Conversation (the composer page popped).
8. T-N9:
   - The embedded issue tab's Threads row omits the current thread.
   - Tapping a chip of a thread already on the stack pops back to it.
9. T14: open a thread link while its items have synced but the thread row has not. Skeleton (never
   "Thread not found"), then the thread.

## Unsure spots
- `ThreadLanding`: an `@Observable final class … : @unchecked Sendable` as an `EnvironmentKey`
  default. Check that `.onChange(of: threadLanding.request)` fires (Request is Equatable, with a
  nonce).
- The navigator's `pushRoute` handler: `else if case let .thread(…) = route, let index = …`. Also
  check that `path.removeSubrange((index + 1)...)` pops cleanly (one animation).
- Started-session landing in AgentPageView: `pushRoute(.thread)` runs from inside an `onChange`
  while that page is being popped. Check there's no double navigation.
- GRDB: `[...].contains(Column("status")) || Column("ended_by") == …` in the orphan query;
  `fetchCount` in ThreadRead.
- `ThreadRunningRow.rows` is `@MainActor static` (it reads the @MainActor model) and is called
  from a View body.
- `ThreadTeamBandedRows` is a generic view with a `@ViewBuilder let row` (memberwise init with a
  trailing closure).
