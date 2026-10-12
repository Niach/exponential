# EXP-1284 iOS addendum 2: Mac checklist (draft thread, thread-not-found, menu sheet)

Written on Linux, uncompiled. It builds on the Mac-fixed tree (adae61cdb + the earlier build fixes).

## Files

1. A draft keeps its thread (`issue_drafts.thread_id`)
   - `ExpCore/Sources/DB/Entities.swift`: `IssueDraftEntity.threadId` (init param after `dueDate`,
     CodingKey `thread_id`, `decodeIfPresent`).
   - `ExpCore/Sources/DB/DatabaseManager.swift`: NEW migration `v67_issue_draft_thread`, a guarded
     `ALTER TABLE issue_drafts ADD thread_id TEXT` that resets the `issue-drafts` offset only when
     the column was added (the v63 pattern). Old migrations are untouched.
   - `ExpCore/Sources/API/IssueDraftsApi.swift`: `IssueDraftDto.threadId` (+ `entity()`) and
     `UpsertIssueDraftInput.threadId`. Omitted when nil, which keeps the row's thread server-side.
   - `Exponential/UI/Issue/IssueDraftViewModel.swift`: `threadId` is now `private(set) var`. On
     reopen it's restored from the row (the route's own threadId wins). `snapshot()` sends it on
     every upsert. Create still passes it. `threadTitle` is loaded through
     `ThreadRows.load(...)?.view.title`.
   - `Exponential/UI/Issue/IssueDraftPageView.swift`: an `In <thread title>` line (`ThreadChip`,
     inert) above the title field; id `issue-draft-thread`.
   - `ExpCore/Sources/Domain/ThreadView.swift`: `ThreadViews.inThreadPrefix` / `inThreadLabel`.
   - Tests:
     - `DatabaseMigrationTests`: v67 is appended to the 4 ordered lists; `thread_id` is appended to
       the 2 full `issue_drafts` column lists; new `testIssueDraftThreadColumnAddedToExistingStore`.
     - `SyncApplyTests.testIssueDraftThreadIdPersistsAndSurvivesPartials`.
     - `ThreadsApiWireTests.testDraftUpsertCarriesTheThreadOnlyWhenSet`.
2. `thread-not-found`
   - `ExpCore/Sources/Domain/EmptyStateCopy.swift`: `.threadNotFound` (+ in `all`).
     `EmptyStateCopyTests` needs no edit; it walks `all` against the fixture and is RED until this
     lands.
   - `ExpUI/Sources/EmptyState.swift`: `EmptyStateIcon` gains `"work-faces"` (checked by
     `EmptyStateIconTests`).
   - `Exponential/UI/Thread/ThreadModels.swift`: `ThreadScreenModel.resolved` is replaced by
     `missing`. It's true only once the store lacks the thread AND `threads.get` refused it (or a
     second miss), so no not-found flash while the fetch runs.
   - `Exponential/UI/Thread/ThreadScreen.swift`: `EmptyState(.threadNotFound) { pushRoute(.agent(…,
     seed: .empty)) }` (= the Agent tab).
3. The child menu is a BOTTOM SHEET
   - NEW `Exponential/UI/Thread/ThreadMenuSheet.swift`: `GlassSheetChrome(title: identifier)`
     (fitted), rows = `GlassMenuItemLabel` buttons (Link an issue · New issue in this thread ·
     Start a session here · destructive `Unlink <IDENT>` on an issue tab; the "+" is untitled with
     no Unlink). Ids: `thread-menu-sheet`, `thread-menu-<action>`.
   - `ThreadScreen.swift`: the inline `glassMenuOverlay` is gone. `.sheet(item: menuTarget)` hangs on
     its own background node. The pick runs on dismiss (`runPendingMenuAction`) so the link sheet,
     the New issue push and the composer push aren't dropped. `ThreadMenuTarget` is new.
   - `ThreadStyleguideScreen.swift`: the static popover is gone. `thread-runs` presents the REAL
     sheet (`.sheet(isPresented: .constant(true))`, EXP-1270).
   - `ExponentialUITests/StyleguideScreenshots.swift`: `openThreadSpecimen(_:_:awaiting:)`.
     `thread-runs` waits on `thread-menu-sheet`, because a presented sheet hides the screen beneath
     it from the a11y tree. `bun --filter @exp/view-catalog test` is green on Linux.

## Verify

```bash
cd apps/ios && tuist generate --no-open
tuist test ExpCore   # DatabaseMigrationTests, SyncApplyTests, ThreadsApiWireTests, EmptyStateCopyTests, ThreadViewTests
tuist test ExpUI     # EmptyStateIconTests
xcodebuild -scheme Exponential -destination 'platform=iOS Simulator,name=iPhone 17' build
bundle exec fastlane styleguide_screenshots shots:sg_thread-runs
bun run shots --platform ios --views thread-runs   # → shots/thread-runs/ios.webp
```
Recapture **sg_thread-runs** and compare it with `shots/thread-runs/android.webp`: identifier title,
4 rows, red Unlink, fitted height.

By hand:
- New issue in this thread → type a title → Back (keep) → reopen from Drafts. The `In <thread>`
  line shows, and Create links the issue into the thread.
- Long press an issue tab → the sheet opens. Each row works: Link opens the search sheet after the
  menu closes; Unlink shows a toast on failure.
- "+" → untitled sheet with 3 rows.
- A bogus thread id deep link → skeleton, then "Thread not found" + Go to Agent.

## Unsure spots

- `@State private var pendingMenuAction: (action:, issueId:)?` is a labeled-tuple @State assigned
  from an unlabeled tuple. If the compiler objects, swap it for a small struct.
- `EmptyState(.threadNotFound) { … }` uses the same trailing-closure overload as IssueListView's
  `EmptyState(.issues) { … }`. Its pill wears the `uiAdd` glyph (EmptyStatePrimaryPill's). If that
  looks wrong for "Go to Agent", it's an ExpUI-wide choice.
- A `.sheet` presented from inside the styleguide's fullScreenCover with a constant binding: check
  that it presents on first appear and that the fitted detent measures (GlassSheetChrome).
- `GlassSheetChrome(title: nil)` draws no header; the rows get `headerTopPadding` instead so they
  clear the drag indicator.
