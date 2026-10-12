# iOS lane: merge of origin/exp/batch-ddafdb85 @ a5dc29724a (home Recent gone) (Linux, uncompiled)

## Files
- `Exponential/UI/Agent/AgentPageView.swift`: conflicts resolved in the working tree (no markers;
  the index still says `UU`, the coordinator stages it). Took the batch's home: greeting →
  composer → options → `homeGettingStarted` (vertical list, `composerToGettingStarted`), NO Recent
  band. Kept ours:
  - `AgentThreadLists` (Running threads + T4 orphan session rows) under Getting started,
    `.padding(.top, 8)` like the batch's old Running list.
  - `centred` = no running threads AND no orphans.
  - The toolbar `RecentThreadsSheet` (threads, banded).
  - `land(_:)` (thread start → pop back to the thread; else `RunThreadScreen`), fed by the batch's
    `landedSession` + `visible`.
  - StartIntent / `threadId` untouched.
- `Exponential/UI/Thread/ThreadLists.swift`: `AgentThreadLists` is Running-only (the home Recent
  section is deleted). `RecentThreadsSheet` titles itself with `ThreadViews.recentLabel`.
- `ExpCore/Sources/Domain/ThreadView.swift`: `ThreadViews.recentLabel = "Recent"` replaces the
  deleted `HomeGreeting.recentLabel`.
- The deleted names (`HomeGreeting.recentLabel/recentCount/TestId.recent/recentRow`,
  `Geometry.optionsToRecent`, `AgentsViewModel.homeRecentRows`) have no references left in
  apps/ios (grep clean). `AgentsViewModel.pastRows` still computes, unused by any view (the old
  `RecentRunsSheet` was deleted in round 3); deleting it is optional.

## Verify
```bash
cd apps/ios && tuist generate --no-open && tuist test ExpCore && tuist test ExpUI
xcodebuild -scheme Exponential -destination 'platform=iOS Simulator,name=iPhone 17' build
bundle exec fastlane styleguide_screenshots shots:sg_agent-home,sg_session-row,sg_getting-started
cd ../.. && bun --filter @exp/view-catalog test
```
- `sg_agent-home`: greeting + composer + options and nothing beneath while nothing runs (the page
  centres). With a live session: the Running band (threads) below Getting started / the options.
- `sg_session-row`: the history sheet `recent-threads-sheet` with `agent-thread-row` rows.
- By hand: a team with no board shows Getting started (the vertical list) ABOVE the Running band;
  starting from a thread still lands back on that thread's Conversation.
