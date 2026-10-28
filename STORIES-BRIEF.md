# S-3 stories: shared brief (temporary, deleted before the PR opens)

Branch `exp/session-24317fd5` is stacked on #1309 (`exp/session-8d293f85`). Read CLAUDE.md first.

## The contract (already committed; don't change it without telling the parent session 24317fd5)
- **Spec:** `packages/domain-contract/fixtures/stories.json`. Its `$comment` is the full spec of the ring, the viewer, navigation, the footer and the constants. TS reference: `packages/domain-contract/src/stories.ts`, test `stories.test.ts`. Mirrors must replay `cases` and compare the WHOLE expected array.
- **Guide tab gone ×4:** `session-thread.json` no longer has a `guide` tab in any expected `tabs`, and `session-tree.ts` dropped the `{kind:'guide'}` tab. See the S-3 STORIES paragraph at the end of its `$comment`. Remove the Guide tab, its dot, the Guide face/body (text, Changes rows, StackRail) and the phones' bottom Merge capsule. The header Merge pill and `…` (`guidePullRequest`/`guideMenuEntries`) stay on every platform, phones included.
- **Dock:** hosts no longer pass `plan`/`question` sources to `dockRows` (dock.json + ask-dock.json, S-3 paragraphs). Cards stay inline in the transcript, answerable. Auto-open of asks goes away.
- **Data:**
  - `session_reads.story_seen_at` (timestamptz, nullable) is a new column in the session-reads shape.
  - Seen = createdAt <= max(read_at, story_seen_at) of the ROOT's row.
  - Mark seen when a story is SHOWN: tRPC `sessions.markStoriesSeen({sessionId: rootId, at: createdAtMs})`. The server keeps the max.
  - Card fields: `card.story?: boolean` on plan/question/surface, and `surface.pictures?: PlanCardPicture[]` (same shape as plan pictures: label, attachmentId, width, height, caption).
  - Answers go through the existing card-answer path (`sessions.answer`, the same call the inline plan/question card uses).
- **Fold input:**
  - sessions = synced sessions, with `name` = the session-title name.
  - agents = synced agents, with `assistant` = role == 'assistant' when that column exists, else false.
  - messages = synced messages with a card.
  - reads = the viewer's session_reads.
- **UI (identical look ×4):**
  - **StoryRing:** the avatar inside a segmented ring, one arc per story. Waiting = the plan/question tint from ask-dock icons/tints, unseen = foreground, seen = the glass-stroke hairline.
  - **StoryViewer:** phones full screen; web wide + desktop a centered modal over a scrim. StoryProgress segments sit on top, then the header (avatar, sessionName, meta, close). The body holds pictures (pager) and/or the surface, drawn by the EXISTING surface-card renderer (chats.json `surfaceCard`), else the plan markdown. The footer has the plan options (Approve/Change), question choices or a reply field, or nothing. Navigation: tap thirds, swipe, arrow keys, side arrows on wide screens. Only results auto-advance.
  - Reuse the existing components: avatars, the surface renderer, plan/question answer controls, sheet/modal chrome, buttons.
- **Styleguide:** new entries `story-ring` and `story-viewer`. That's five sites: `apps/styleguide/src/sections/sections.json`, `entries/index.ts`, `entries/<id>.tsx`, `sections/page.ts` ENTRY_BAND, plus the desktop styleguide twin (`apps/desktop/crates/ui/src/styleguide/entries/`). Natives have their own styleguide screens (iOS ChatSpecimens/ThreadCardsScreen, Android ui/styleguide/*). Add the specimens there too. NOT in eUI (decided: Exponential-specific; they compose existing Ring/Progress concepts).
- **Chat list wiring is NOT yet allowed:** sibling session 58cca31b owns chats.json and the bubble row files. Build StoryRing + StoryViewer + a host-level "stories" state/hook, and mount the ring row at the TOP of the chats list. Make it a separate small component, `StoryRingRow`, placed in the chats page just above the existing agent bubbles row. The parent session merges it into the bubbles later.
- **Rules:** Never lint/format. Never `git stash/checkout/reset/clean`. Commit only your own files (`git add <paths>`), with message prefix `S-3 stories:`. Don't push (the parent pushes). Use lane locks for heavy builds: `scripts/lanes/lane.sh <android|heavy> <owner> -- <cmd>`.
