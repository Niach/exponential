// @vitest-environment node
// #EDE-115: a big chat opened on the web from the chat store (wasm + OPFS)
// vs from the live feed. BEFORE, nothing showed until the eNet discovery +
// join round trip delivered the device's replay (its fold measured here, the
// network not); AFTER, the store's REPLAY TAIL paints first: the file read
// into the wasm once (the worker's first open of that chat), the tail cut,
// the same fold. A synthetic 20k-event journal; `STEER_REAL_JOURNAL=<file>`
// swaps in a real one and prints the numbers.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { ChatStoreCore, type StoredEvent } from "@/lib/chat-store/store-core"
import { loadTestWasm, memoryOpfs, wasmBuilt } from "@/lib/chat-store/test-support"
import { createSteerSessionStore } from "@/lib/steer-session-store"
import { openChat, realJournal, replay, REPLAY_TAIL, syntheticJournal } from "@/lib/perf/large-chat"

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

const wallMs = () => performance.now()

describe.skipIf(!wasmBuilt)(`large chat open from the chat store`, () => {
  beforeEach(() => vi.useFakeTimers({ toFake: [`setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `Date`] }))
  afterEach(() => vi.useRealTimers())

  it(`paints the stored REPLAY TAIL without a feed`, async () => {
    const path = process.env.STEER_REAL_JOURNAL
    const journal = path ? realJournal(path) : syntheticJournal(20_000)
    const wasm = await loadTestWasm()
    const opfs = memoryOpfs()
    const stored: StoredEvent[] = journal.map((event, seq) => ({ rootId: `r1`, sessionId: `session-1`, deviceId: `d1`, seq, storedAt: 1, event }))
    let started = wallMs()
    await new ChatStoreCore(wasm, opfs.host).putEvents(`r1`, stored)
    const ingestMs = wallMs() - started
    const fileBytes = opfs.files.get(`r1.log`)!.getSize()

    // The worker's FIRST open of the chat: the file read into the wasm.
    const core = new ChatStoreCore(wasm, opfs.host)
    started = wallMs()
    const cold = await core.openTail(`r1`, `session-1`, `d1`, REPLAY_TAIL)
    const coldMs = wallMs() - started
    started = wallMs()
    const tail = await core.openTail(`r1`, `session-1`, `d1`, REPLAY_TAIL)
    const warmMs = wallMs() - started
    expect(cold.truncated).toBe(true)
    expect(tail.events.length).toBe(cold.events.length)

    // The seed's fold into the session store (the rows on screen).
    const store = createSteerSessionStore(`session-1`, {
      openFeed: () => new Promise(() => {}),
      stored: { record: () => {}, openTail: async () => tail, page: async () => null },
    })
    started = wallMs()
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const foldMs = wallMs() - started
    const seeded = store.getSnapshot()
    expect(seeded.feed.length).toBeGreaterThan(0)
    expect(seeded.canLoadEarlier).toBe(true)
    store.dispose()

    // Before: the live feed's replay of the same tail (after the network).
    const before = await openChat(replay(journal, REPLAY_TAIL).frames)
    const result = {
      events: journal.length,
      fileMiB: +(fileBytes / 1048576).toFixed(1),
      ingestOnceMs: Math.round(ingestMs),
      after: { storeColdMs: Math.round(coldMs), storeWarmMs: +warmMs.toFixed(1), foldMs: +foldMs.toFixed(1), items: seeded.feed.length },
      before: { replayFoldMs: +before.ms.toFixed(1), items: before.items, plus: `the eNet discovery + join round trip` },
    }
    if (path) process.stderr.write(`${JSON.stringify(result)}\n`)
    // Generous for CI; a laptop's warm open of this tail is a few ms.
    expect(warmMs + foldMs).toBeLessThan(2_000)
  }, 120_000)
})
