import { describe, expect, it } from "vitest"
import { sessionTurns } from "@exp/ui"
import {
  emptyTurnLog,
  recordFeedMessages,
  recordTurnSlot,
  turnEventsOf,
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
})
