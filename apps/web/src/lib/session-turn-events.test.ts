import { describe, expect, it } from "vitest"
import { sessionTurns } from "@exp/ui"
import {
  SEEN_CAP,
  emptyTurnLog,
  firstTurnEndKnown,
  hasTurnLog,
  recordFeedMessages,
  recordTurnSlot,
  releaseTurnLog,
  turnEventsOf,
  turnLogFor,
} from "@/lib/session-turn-events"

// EXP-1245: the owner's turn facts off the relay feed — observed turn edges,
// placed messages, and the run's start opening the first turn.

const START = 1_000

describe(`session turn events`, () => {
  it(`yields nothing while only the run's start is known`, () => {
    const log = emptyTurnLog()
    recordTurnSlot(log, `ended`, null, 2_000)
    recordFeedMessages(log, [], 2_000)
    expect(turnEventsOf(log, [], START)).toEqual([])
  })

  it(`records a started edge and an ended edge seen to follow it`, () => {
    const log = emptyTurnLog()
    recordTurnSlot(log, `started`, 5_000, 5_100)
    recordTurnSlot(log, `started`, 5_000, 6_000)
    recordTurnSlot(log, `ended`, 5_000, 9_000)
    expect(log.edges).toEqual([
      { kind: `turn`, state: `started`, at: 5_000 },
      { kind: `turn`, state: `ended`, at: 9_000 },
    ])
  })

  it(`never invents an end for a slot already ended on arrival`, () => {
    const log = emptyTurnLog()
    recordTurnSlot(log, `ended`, 5_000, 9_000)
    expect(log.edges).toEqual([])
  })

  it(`places a message by its at, a live arrival by now, never a replay's bulk`, () => {
    const log = emptyTurnLog()
    const replay = [
      { id: 1, kind: `user_message`, text: `old one` },
      { id: 2, kind: `narration`, text: `x` },
      { id: 3, kind: `user_message`, text: `old two` },
    ]
    recordFeedMessages(log, replay, 2_000)
    expect(log.messageAt.size).toBe(0)
    const live = [...replay, { id: 4, kind: `user_message`, text: `follow-up` }]
    recordFeedMessages(log, live, 7_000)
    const stamped = [...live, { id: 5, kind: `user_message`, text: `stamped`, at: 8_000 }]
    recordFeedMessages(log, stamped, 9_500)
    expect([...log.messageAt.entries()]).toEqual([
      [4, 7_000],
      [5, 8_000],
    ])
  })

  // F35: `primed` used to flip on the first EMPTY composition, so a short
  // history replay (one or two rows without `at`) read as a live arrival
  // and was stamped `now`. Only a feed AFTER the first non-empty one is live.
  it(`never stamps the first non-empty feed, however short`, () => {
    const log = emptyTurnLog()
    recordFeedMessages(log, [], 1_000)
    recordFeedMessages(log, [], 1_500)
    const replay = [{ id: 1, kind: `user_message`, text: `replayed` }]
    recordFeedMessages(log, replay, 2_000)
    expect(log.messageAt.size).toBe(0)
    recordFeedMessages(log, [...replay, { id: 2, kind: `user_message`, text: `live` }], 3_000)
    expect([...log.messageAt.entries()]).toEqual([[2, 3_000]])
  })

  it(`never stamps a history page's older rows, even one or two`, () => {
    const log = emptyTurnLog()
    recordFeedMessages(log, [{ id: 10, kind: `narration`, text: `x` }], 2_000)
    // A history page prepends lower ids: not live, whatever its size.
    recordFeedMessages(
      log,
      [{ id: 9, kind: `user_message`, text: `older` }, { id: 10, kind: `narration`, text: `x` }],
      3_000
    )
    expect(log.messageAt.size).toBe(0)
    recordFeedMessages(
      log,
      [
        { id: 9, kind: `user_message`, text: `older` },
        { id: 10, kind: `narration`, text: `x` },
        { id: 11, kind: `user_message`, text: `new` },
      ],
      4_000
    )
    expect([...log.messageAt.entries()]).toEqual([[11, 4_000]])
  })

  // F16: the per-session registry and its `seen` set are bounded.
  it(`caps the seen set, dropping the oldest ids and their placements`, () => {
    const log = emptyTurnLog()
    const rows = Array.from({ length: SEEN_CAP + 10 }, (_, i) => ({
      id: i,
      kind: `user_message`,
      text: `m${i}`,
      at: 1_000 + i,
    }))
    recordFeedMessages(log, rows, 9_000)
    expect(log.seen.size).toBe(SEEN_CAP)
    expect(log.seen.has(0)).toBe(false)
    expect(log.seen.has(9)).toBe(false)
    expect(log.seen.has(10)).toBe(true)
    expect(log.messageAt.has(9)).toBe(false)
    expect(log.messageAt.get(10)).toBe(1_010)
    expect(log.maxSeen).toBe(SEEN_CAP + 9)
  })

  it(`releases a session's log`, () => {
    const log = turnLogFor(`s-release`)
    expect(turnLogFor(`s-release`)).toBe(log)
    expect(hasTurnLog(`s-release`)).toBe(true)
    releaseTurnLog(`s-release`)
    expect(hasTurnLog(`s-release`)).toBe(false)
    expect(turnLogFor(`s-release`)).not.toBe(log)
    releaseTurnLog(`s-release`)
  })

  it(`skips a subagent's message`, () => {
    const log = emptyTurnLog()
    recordFeedMessages(log, [], 1)
    recordFeedMessages(log, [{ id: 1, kind: `user_message`, text: `hi`, subagentId: `s1` }], 2)
    expect(log.messageAt.size).toBe(0)
  })

  it(`feeds sessionTurns two turns with the person's bubble between`, () => {
    const log = emptyTurnLog()
    recordFeedMessages(log, [], START)
    recordTurnSlot(log, `started`, START, START)
    recordTurnSlot(log, `ended`, START, 4_000)
    const feed = [{ id: 1, kind: `user_message`, text: `stack it`, at: 5_000 }]
    recordFeedMessages(log, feed, 5_000)
    recordTurnSlot(log, `started`, 5_100, 5_100)
    const results = [
      { topic: `Summary`, label: null, attachmentId: null, text: `first`, at: 3_000 },
      { topic: `Reviews`, label: null, attachmentId: null, text: `second`, at: 6_000 },
    ]
    const { perTurn, turns } = sessionTurns(results, turnEventsOf(log, feed, START))
    expect(perTurn).toBe(true)
    expect(turns).toHaveLength(2)
    expect(turns[0]?.message).toBeNull()
    expect(turns[0]?.endedAt).toBe(4_000)
    expect(turns[0]?.reply).toBe(`first`)
    expect(turns[1]?.message?.text).toBe(`stack it`)
    expect(turns[1]?.startedAt).toBe(5_100)
    expect(turns[1]?.endedAt).toBeNull()
    expect(turns[1]?.items.map((item) => item.kind === `text` && item.text)).toEqual([`second`])
  })

  it(`knows the first turn's end only when the view watched it run`, () => {
    // Watched: the first event after the run's start is an observed start.
    const watched = emptyTurnLog()
    recordFeedMessages(watched, [], START)
    recordTurnSlot(watched, `started`, START + 50, START + 50)
    recordTurnSlot(watched, `ended`, START + 50, 4_000)
    const feed = [{ id: 1, kind: `user_message`, text: `next`, at: 9_000 }]
    recordFeedMessages(watched, feed, 9_000)
    expect(firstTurnEndKnown(turnEventsOf(watched, feed, START), START)).toBe(true)

    // Mounted after the first turn ended: only the next message closes it.
    const late = emptyTurnLog()
    recordTurnSlot(late, `ended`, null, 8_000)
    recordFeedMessages(late, [], 8_000)
    recordFeedMessages(late, feed, 9_000)
    const events = turnEventsOf(late, feed, START)
    expect(firstTurnEndKnown(events, START)).toBe(false)
    const { turns } = sessionTurns([], events)
    // sessionTurns still closes it at the message; the row drops the time.
    expect(turns[0]?.endedAt).toBe(9_000)
  })

  it(`treats a feed without the run's start as observed`, () => {
    expect(firstTurnEndKnown([], START)).toBe(true)
    expect(
      firstTurnEndKnown(
        [
          { kind: `turn`, state: `started`, at: 5_000 },
          { kind: `turn`, state: `ended`, at: 6_000 },
        ],
        null
      )
    ).toBe(true)
  })

  it(`carries a message's file links beside its images (wave D)`, () => {
    const log = emptyTurnLog()
    const text = `look\n\n![image](/api/attachments/i1)\n[build.log](/api/attachments/f1)`
    const feed = [{ id: 1, kind: `user_message`, text, at: 5_000 }]
    recordFeedMessages(log, feed, 5_000)
    const message = turnEventsOf(log, feed, START).find(
      (event) => event.kind === `user_message`
    )
    expect(message).toEqual({
      kind: `user_message`,
      at: 5_000,
      text: `look`,
      images: [`/api/attachments/i1`],
      files: [{ name: `build.log`, url: `/api/attachments/f1` }],
    })
  })
})
