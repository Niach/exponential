Batch D, Android lane: #ERR-28 (churn when the biggest chat opens; drop the `largeHeap` stopgap, #ERR-25 / #2863) and #ERR-27 (notification taps stacking chats).

## What changed
- **Timestamps parsed without java.time** (`WireTimestamps.parseFast`): the Electric and ISO shapes are read by a hand-rolled, allocation-free parser; anything odd still takes the old tolerant path. 24 B per parse (the boxed result) instead of ~590 B (the #ERR-23 dump: 56 MB over 95k parses). A 20k-case equivalence test against java.time locks it.
- **Titles indexed once** (`SessionTitle.Titler`, `SessionTree.titler`): links, issues and messages grouped by session once; only rows that can title a session (card-less, from a person or another session) converted; only linked issues mapped. The per-team Needs you fold and the waiting-label loop share one index instead of rebuilding it per team / per child. 2.0 MB per pass over 54k digest rows (was every row converted per call, 7× per sessions read in `needsYouByTeam`, and per child in `waiting`).
- **Digests re-parsed only when they change** (`Chats.DigestMessagesCache`), **session / issue rows re-mapped only when they change** (`MappedRows`): a synced write re-parsed all ~2k digests (JSON trees, cards, stamps) on every sessions read; now one changed row = one parse, and unchanged rows keep their instances.
- **Shared whole-table reads conflate** (`sharedRead`): a newer table replaces the one a slow reader hasn't taken yet, so a write burst keeps the copy being read plus the newest, never a third parked in the share's buffer.
- **`ChatViewModel.syncedIssues`** is the shared read itself, not a `StateFlow` that kept a stale whole-table snapshot in every chat on the back stack.
- **Notification taps land by THE back stack rule** (#EXP-1830): a push tap, push link or app intent (`DeepLinkBus` targets carry `rooted`) drops to the Agent tab root and pushes the target (`launchOnAgentRoot`; `launchIssueOnAgentRoot` now keeps `?comment=`). Each tap is still a fresh entry, so the EXP-528 stale-argument bug stays fixed. In-app opens (inbox rows, tapped links) still push.
- **`android:largeHeap` removed**: the 10-min C2 run is clean under the 256 MB growth limit with it off.

## Measured (MacBook arm64 emulator; Pixel 7 heap: heapgrowthlimit 256m / heapsize 512m, re-applied and checked after boot; `largeHeap` OFF in both builds)
Seed at prod sizes: 2175 sessions (4.3 MB digests), 3434 issues, 53,966 messages (biggest tree 10,000), 1847 PRs, 40,334 checks. C2 = 10 min of push-link chat opens (every third the 10k-message chat; a scroll; Back) while the server writes about once a second (~560 bursts: a message, the root session, an issue, 3 checks).

| | BEFORE (master 8ce3c96) | AFTER |
|---|---|---|
| C2 deaths / OOM lines | 0 / 0 (pinned at the 256 MB cap) | 0 / 0 |
| C2 peak Dalvik heap size | 256 MB (the limit) | 134 MB |
| C2 peak allocated | 240 MB | 113 MB |
| Big-chat open, peak allocated (max / median, 13–14 opens) | 234 / 177 MB | 107 / 79 MB |
| Other chat opens, median peak | 137 MB | 69 MB |
| End of C2, allocated after GC | 149 MB | 58 MB |
| End of C2, `IssueEntity` objects (copies of the 3433-row table) | 34,330 (10) | 6,865 (2) |
| End of C2, `ChatViewModel`s | 11 | 0 |
| 15 notification taps + Back: `ChatViewModel`s / Java heap | 13 / 100 MB | 0 / 70 MB |

BEFORE didn't die in this run (the #ERR-23 baseline had 1 OOM at +522 s), but it ran pinned at the 256 MB limit; AFTER never went above 134 MB.

## Memory-budget tests (a regression fails CI, not a phone)
- `WireTimestampsTest.parsingAllocatesNothingPerStamp`: 100k parses ≤ 3.2 MB (measured 2.4 MB, the boxed `Long?`); `fastPathMatchesJavaTime`: 20k random stamps, both wire forms, to the millisecond.
- `ChatsMemoryBudgetTest`: re-reading 2,000 digests with one changed parses one (< 1.5 MB); titling 2,000 sessions over 54k digest rows < 3 MB (measured 2.0 MB); unchanged rows keep their mapped instance.
- `DeepLinkBackStackTest` (Robolectric + `TestNavHostController`, the real `selectTab` / `launchOnAgentRoot`): 40 notification taps across 7 chats from another tab leave `[Agent root, one chat]`, each tap a fresh entry (EXP-528); issue taps keep `?comment=`; an in-app open still pushes.
