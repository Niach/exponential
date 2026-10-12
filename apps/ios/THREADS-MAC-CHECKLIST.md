# EXP-1284 phase B, iOS lane: Mac checklist

Written on Linux with NO Swift toolchain. Nothing below has been compiled. Phase A's iOS
data layer (ThreadEntity/ThreadItemEntity, v66 migration, ThreadQueries.swift, ThreadsApi.swift,
SyncManager shapes) has never been built either. Build it, fix it, then capture.

## Files

New:
- `apps/ios/ExpCore/Sources/Domain/ThreadView.swift`: port of `apps/web/src/lib/thread-view.ts`.
  `ThreadViews.build(thread:items:issues:runs:) -> ThreadViewModel` (named `ThreadViewModel`,
  not `ThreadView`, so it can't collide with SwiftUI names), `ThreadViews.partition(_:)`, the copy
  constants, and iOS-local helpers: `guideResults`, `replyTarget`, `mergeTarget`, `runResults`
  (the Guide entries one run published, windowed by their `at` stamps), `isFresh`, `hasGuide`.
- `apps/ios/ExpCore/Tests/ThreadViewTests.swift`: runs EVERY case of
  `packages/domain-contract/fixtures/thread-view.json` (`views` + `partitions`), plus focused tests
  for the helpers.
- `apps/ios/Exponential/UI/Thread/` (picked up by the `Exponential/**` glob):
  - `ThreadRows.swift`: `ThreadPage` (pager page enum, rides `AppRoute`) and `ThreadRows` (one
    GRDB pass: thread + items + runs + named issues + devices; builds the view).
  - `ThreadModels.swift`: `ThreadScreenModel`, `ThreadListModel` (Agent page), `IssueThreadsModel`
    (the issue's Threads row), `RunThreadLookup` (the thread of a run: run screens + Guide readers).
  - `ThreadScreen.swift`: `ThreadScreen` (header band = title + `thread` word + header caption +
    the strip as pager tabs; pages Conversation / issue = `IssueFaceView` / Guide = `GuideFace` on
    `threads.results` + `WorkMergePill`; THE Menu; link/unlink; reply box logic), `ThreadReplyBox`,
    `RunThreadScreen` (a run's thread, falling back to the run's `WorkScreen` after 4 s).
  - `ThreadConversation.swift`: `ThreadConversationView` (run blocks, link rows, parallel card),
    `ThreadBlockHeader`, `ThreadLinkRowView`.
  - `ThreadStripTabs.swift`: `ThreadStripTabs` (pager tabs, "+" trailing, long press = child menu,
    fresh-child fade + ring), `ThreadHeaderTitle`.
  - `ThreadLinkIssueSheet.swift`: Link an issue = `SearchListSheet(SearchListView)` over the team's
    issues.
  - `ThreadLists.swift`: `ThreadListRow`, `AgentThreadLists` (Running/Recent), `IssueThreadsRow`,
    `ThreadChip`.
  - `ThreadStyleguideScreen.swift`: capture-only specimens `thread` / `thread-runs` (fake rows that
    match drafts 1/3 and 2).

Changed:
- `ExpCore/Sources/API/SteerApi.swift`: `threadId` on `StartSessionInput` / `StartBatchSessionInput` /
  `StartActionSessionInput` (a defaulted `var`, so it's omitted when nil) and a trailing
  `threadId: String? = nil` on the three `startSession` overloads.
- `ExpCore/Sources/API/IssuesApi.swift`: `CreateIssueInput.threadId`.
- `ExpCore/Sources/Domain/AgentComposerSeed.swift`: `threadId`.
- `ExpCore/Tests/SteerStartInputEncodingTests.swift`, `ExpCore/Tests/ThreadsApiWireTests.swift`:
  wire tests for the two `threadId`s.
- `Exponential/UI/Agent/AgentComposerModel.swift`: `threadId` from the seed, sent on all 4 starts.
- `Exponential/UI/Agent/AgentPageView.swift`: Running/Recent are THREADS (`AgentThreadLists`).
  The run-based `homeRecent` is removed. A started run now opens `RunThreadScreen`.
- `Exponential/UI/Agent/AgentSessionsList.swift`: the now-unused `AgentSessionsList` struct is
  removed (`SessionListRow` stays).
- `Exponential/UI/Issue/IssueFaceView.swift`: the `Threads` row under the property chips. The face
  owns an `IssueThreadsModel` through a `.task(id:)`.
- `Exponential/UI/Issue/IssueDraftPageView.swift`, `IssueDraftViewModel.swift`: `threadId` →
  `issues.create({threadId})`.
- `Exponential/UI/Navigation/AppNavigator.swift`: routes `.thread(accountId:threadId:child:)` and
  `.runThread(accountId:sessionId:)`; `.issueDraft` gains a 6th field `threadId` (all patterns
  updated); a third `.fullScreenCover` for `-uiTestingStyleguide thread|thread-runs`.
- `Exponential/UI/Work/WorkScreen.swift`: an always-mounted toolbar item `threadLink` (opens the
  shown run's thread); the Guide reads `thread.results ?? run.results` (`resultsRaw`).
- `Exponential/UI/Work/RunChangesView.swift`: the same Guide rule.
- `ExponentialUITests/StyleguideScreenshots.swift`: `sg_thread`, `sg_thread-runs` (+ the
  `openThreadSpecimen` helper). `bun --filter @exp/view-catalog test -t "iOS styleguide"` passes
  on Linux. The Android lane fails until Android adds its two shots.

## Commands

```bash
cd apps/ios
tuist install && tuist generate --no-open
tuist test ExpCore          # ThreadViewTests, SteerStartInputEncodingTests, ThreadsApiWireTests,
                            # DatabaseMigrationTests / SyncApplyTests (phase A)
tuist test ExpUI
xcodebuild -scheme Exponential -destination 'platform=iOS Simulator,name=iPhone 17' build   # app target
# the two shots only (fastlane styleguide lane, shots: narrows to ids):
cd apps/ios && bundle exec fastlane styleguide_screenshots shots:sg_thread,sg_thread-runs
#   raw output: apps/ios/fastlane/screenshots-styleguide/
# or via the pipeline (imports + encodes to the committed store):
bun run shots --platform ios --views thread,thread-runs
#   → shots/thread/ios.webp, shots/thread-runs/ios.webp
cd ../.. && bun --filter @exp/view-catalog test
```
Look at both webps against the drafts (draft 3 = phone; draft 2 = the menu). The `thread-runs`
specimen draws the child menu at a hard-coded `.padding(.top, 118)` under the strip. Tune it so
it hangs under the EXP-1270 tab.

## Unsure spots (verify / fix while compiling)

1. **Swift language details**
   - `switch child(for: page) { case let .issue(...) ... default: }` matches enum cases against an
     `Optional<ThreadChild>` (ThreadStripTabs). I rely on implicit optional promotion. Add `?` if
     the compiler refuses.
   - `ThreadViews.runState` is overloaded (`(status:needsInput:)` and `(_:)`). The call sites use
     closures, so this should be fine.
   - `ThreadViewTests`: `ExpectedChild.model` uses a `switch` expression into an optional with
     implicit members (`.conversation` as `ThreadChild?`).
   - `FacePager(pages:selection:page:)` with a trailing closure (memberwise init with a
     `@ViewBuilder let page`), as `WorkFacePager` does.
   - `WorkMergePill(... state:identifier:)` (style defaulted). This is the same memberwise call
     WorkScreen makes.
   - `ThreadStripTab` is a private generic struct with a `@ViewBuilder let content` memberwise init.
2. **Concurrency (Swift 6 strict)**
   - `ValueObservation.tracking { try ThreadRows.load(...) }`: `ThreadRows` is declared
     `Sendable`. Check the entities really are.
   - `ThreadScreenModel.fetchMissing` writes `detail.thread.entity().save(db)` in `pool.write`.
     Check that `ThreadEntity` / `ThreadItemEntity` `save` compiles (PersistableRecord +
     synthesized `encode(to:)` in the phase A extension).
   - Closures handed to `ThreadStripTabs` (`isFresh`, `issue`) capture the MainActor model.
3. **GRDB**: `Array(...).contains(Column("id"))` for `IN` (also with an empty array);
   `ThreadEntity.order(...).filter(...)` reassigned to `var request`.
4. **SwiftUI availability (deployment 17.4)**: `onGeometryChange` (already used in the app),
   `onLongPressGesture(minimumDuration:perform:)` together with `.onTapGesture` on the same tab
   (check both fire correctly), `Task.sleep(until:clock:)` in `RunThreadScreen`, the
   `ToolbarItem(...).sharedBackgroundVisibility(.hidden)` block for iOS 26 in WorkScreen (same
   pattern as the badge).
5. **WorkScreen type-check time**: one more `.onChange` was added to the long `withLifecycle` chain
   and one more ToolbarItem pair to `withChrome`. If the compiler times out, split the chain.
6. **Two-claim socket logic** (`ThreadScreen.claimReplyRun` / `sendReply`) uses
   `deps.attachSteerModel` and `steerSessions.detach`. Check that the attach/detach counts balance
   (claimed run + transient send claims) and that `AgentSessionModel.sendMessage` returns false
   until the socket is connected (the resume path retries for 30 s).
7. **Toolbar on ThreadScreen**: the screen only hides the nav bar background (no principal title,
   since the title lives in the header band). Check the back button over the band looks right.
8. **AgentPageView centring**: `centred` now keys on the thread lists (nil before the model
   exists = centred). Check there's no jump on first load.

## Gaps / decisions (for the coordinator)

- The phone conversation shows no user prompt bubble: no synced column carries a run's prompt.
  The run screen's turns come from the relay feed. A run block's body = the run's slice of the
  thread Guide (`ThreadViews.runResults`, windowed by `at`) + the status row. Show work on a
  folded run expands that slice in place. The full transcript is the run's own Work screen (its
  header row and the status row's Show work button open it).
- Rename (`threads.update`) is not wired on iOS (it was optional).
- The issue draft row (`issue_drafts`) doesn't store `threadId`: a New issue in this thread that's
  left and reopened from Drafts files without the thread.
- The Agent page history sheet (`RecentRunsSheet`, `sg_session-row`) still lists runs. Home's
  Running/Recent list threads; Recent = the ACTIVE team's threads with at least one item, capped
  at `HomeGreeting.recentCount`. Running = every team's threads with a live run of mine
  (heartbeat-live).
- `ThreadListModel` re-runs the whole thread assembly on any write to threads/items/sessions/
  issues/devices (device heartbeats included). Fine for a few hundred threads; profile on a big
  account.
- Outside apps/ios (not touched, needs the owner):
  `packages/shots/src/affected.ts` NATIVE_DIRS has no `thread` family. Add
  `{ dir: \`thread\`, views: /^thread/ }`; otherwise changes under `UI/Thread/` widen to every
  iOS view. Android must add `sg_thread` + `sg_thread-runs`, or view-catalog's Android lane stays
  red. views.json needs no change (the iOS lane captures specimens, not `DEMO_THREAD_ID`); maybe
  add an `ios` note saying so.

## Copy rule (added after the coordinator's follow-up)

The product says SESSION, never "run", in every user-facing string (identifiers unchanged). The
coordinator already updated `thread-view.json` and `ThreadView.swift` (`Session N`,
`Sessions 3 and 4`, `started by Session K`, `n session(s)`, `Start a session here`) and
ThreadScreen's resume toast ("the session"). The thread files have no other user-facing run copy.
When you check the two webps, make sure they read "Session".

Pre-existing strings in files this lane touched, NOT changed here because they predate EXP-1284
(left for the product-wide sweep): AgentPageView `accessibilityLabel("Recent runs")` and the
relay-off note ("Start runs from the desktop app; live runs show up below."), and WorkScreen's
Stop `accessibilityLabel("Stop the agent and end the run")`. The issue title "ios run error"
in the thread specimen is fake draft DATA (an issue name), not copy.

## Coordinator additions (AppLink `thread`, EXP-1284)
- `ExpCore/Sources/Domain/AppLink.swift`: new case `.thread(teamSlug:threadId:)` for `/t/{team}/threads/{id}`; `ExpCore/Tests/AppLinkTests.swift` decodes the fixture's `thread` kind (3 new cases in `packages/domain-contract/fixtures/app-link.json`).
- `Exponential/UI/Markdown/AgentMarkdownText.swift`: the exhaustive `switch AppLink.classify` gained `.thread` → `deps.deepLinkBus.navigateToThread`.
- `Exponential/Data/Push/DeepLinkBus.swift`: `pendingThreadId`/`pendingThreadAccountId`, `navigateToThread`, `consumeThread`; `Exponential/UI/Navigation/AppNavigator.swift`: `.onChange(of: deps.deepLinkBus.pendingThreadId)` pushes `AppRoute.thread`. Check any OTHER exhaustive switch over `AppLink` the compiler flags.
