import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { FEED_WINDOW } from "@/lib/agent-feed"
import { openChat, realJournal, replay, REPLAY_TAIL, SLOT_KINDS, syntheticJournal } from "@/lib/perf/large-chat"

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

// Large chats (steer-enet.json REPLAY TAIL): a device replays only the newest
// `steerFeed.replayTail` log events on join, the view paints `steerFeed.window`
// rows. This guards both numbers and the web store's open time over a
// synthetic 20k-event journal (`STEER_REAL_JOURNAL=<file.jsonl>` swaps in a
// real one and prints before/after).

describe(`large chat open (REPLAY TAIL + window)`, () => {
  beforeEach(() => vi.useFakeTimers({ toFake: [`setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `Date`] }))
  afterEach(() => vi.useRealTimers())

  it(`opens the newest tail fast and paints at most one window`, async () => {
    const path = process.env.STEER_REAL_JOURNAL
    const events = path ? realJournal(path) : syntheticJournal(20_000)
    const tail = replay(events, REPLAY_TAIL)
    expect(tail.frames.length).toBeLessThanOrEqual(REPLAY_TAIL + SLOT_KINDS.size + 2)
    const after = await openChat(tail.frames)
    expect(after.rows).toBeLessThanOrEqual(FEED_WINDOW)
    expect(after.canLoadEarlier).toBe(true)
    // Generous for CI; a laptop opens this tail in ~5-10 ms.
    expect(after.ms).toBeLessThan(1_000)
    if (path) {
      // Before: the whole journal, and the old 1500-row window.
      const before = await openChat(replay(events, null).frames, 1_500)
      const { snapshot: _a, ...a } = after
      const { snapshot: _b, ...b } = before
      process.stderr.write(`${JSON.stringify({ events: events.length, before: b, after: a })}\n`)
    }
  })
})
