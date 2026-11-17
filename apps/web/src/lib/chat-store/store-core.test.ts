// @vitest-environment node
// #EDE-115 (chat-store.json S4): the web's chat store over memory "OPFS"
// with the real wasm: the log per chat, two tabs on one profile, RETENTION,
// OPEN's REPLAY TAIL, messages and the key, the catch-up's two halves.

import { describe, expect, it } from "vitest"
import { ChatStoreCore, type StoredEvent } from "./store-core"
import { loadTestWasm, memoryOpfs, wasmBuilt } from "./test-support"

const DAY = 86_400_000

const event = (seq: number, body: Record<string, unknown>, sessionId = `s1`): StoredEvent => ({
  rootId: `r1`,
  sessionId,
  deviceId: `d1`,
  seq,
  storedAt: 1_000,
  event: body,
})

describe.skipIf(!wasmBuilt)(`the web's chat store (wasm + OPFS)`, () => {
  it(`appends only what is new and a second tab reads the other's appends`, async () => {
    const wasm = await loadTestWasm()
    const { host, files } = memoryOpfs()
    const a = new ChatStoreCore(wasm, host, () => 10 * DAY)
    const b = new ChatStoreCore(wasm, host, () => 10 * DAY)
    await a.putEvents(`r1`, [event(1, { kind: `user_message`, text: `Fix the café` }), event(2, { kind: `narration`, text: `Done.` })])
    const size = files.get(`r1.log`)!.getSize()
    await a.putEvents(`r1`, [event(2, { kind: `narration`, text: `a repeat` })])
    expect(files.get(`r1.log`)!.getSize()).toBe(size)
    expect((await b.openTail(`r1`, `s1`, `d1`, 50)).events.map((e) => e.seq)).toEqual([1, 2])
    await b.putEvents(`r1`, [event(3, { kind: `narration`, text: `Later.` })])
    const tail = await a.openTail(`r1`, `s1`, `d1`, 50)
    expect(tail).toEqual({
      truncated: false,
      events: [
        { seq: 1, event: { kind: `user_message`, text: `Fix the café` } },
        { seq: 2, event: { kind: `narration`, text: `Done.` } },
        { seq: 3, event: { kind: `narration`, text: `Later.` } },
      ],
    })
    expect(await a.openTail(`r1`, `s1`, `other`, 50)).toEqual({ events: [], truncated: false })
    await expect(a.openTail(`../x`, `s1`, `d1`, 50)).rejects.toThrow()
  })

  it(`opens a big chat through the REPLAY TAIL and pages the rest`, async () => {
    const wasm = await loadTestWasm()
    const core = new ChatStoreCore(wasm, memoryOpfs().host)
    const events: StoredEvent[] = []
    for (let seq = 0; seq < 300; seq++) events.push(event(seq, { kind: `narration`, text: `n${seq}` }))
    events.push(event(300, { kind: `turn`, state: `ended` }))
    await core.putEvents(`r1`, events)
    const tail = await core.openTail(`r1`, `s1`, `d1`, 100)
    expect(tail.truncated).toBe(true)
    expect(tail.events[0]!.seq).toBe(200)
    expect(tail.events.at(-1)!.seq).toBe(300)
    const page = await core.page(`r1`, `s1`, `d1`, 200, 50)
    expect(page.seqs[0]).toBe(150)
    expect(page.seqs.at(-1)).toBe(199)
  })

  it(`RETENTION on load keeps prose and fresh tool steps and rewrites the file`, async () => {
    const wasm = await loadTestWasm()
    const opfs = memoryOpfs()
    const old = { at: 0 }
    await new ChatStoreCore(wasm, opfs.host, () => DAY).putEvents(`r1`, [
      event(1, { kind: `user_message`, text: `old`, ...old }),
      event(2, { kind: `tool`, name: `Bash`, id: `t`, ...old }),
      event(3, { kind: `tool_update`, id: `t`, output: `out`, ...old }),
      event(4, { kind: `tool`, name: `Read`, id: `u`, at: 40 * DAY }),
    ])
    const before = opfs.files.get(`r1.log`)!.getSize()
    const later = new ChatStoreCore(wasm, opfs.host, () => 60 * DAY)
    expect((await later.openTail(`r1`, `s1`, `d1`, 50)).events.map((e) => e.seq)).toEqual([1, 4])
    expect(opfs.files.get(`r1.log`)!.getSize()).toBeLessThan(before)
    // A tab that held the old file reloads it whole (a new generation).
    const sameDay = new ChatStoreCore(wasm, opfs.host, () => 60 * DAY)
    expect((await sameDay.openTail(`r1`, `s1`, `d1`, 50)).events.map((e) => e.seq)).toEqual([1, 4])
  })

  it(`a body that opened is never replaced by one that did not`, async () => {
    const wasm = await loadTestWasm()
    const core = new ChatStoreCore(wasm, memoryOpfs().host)
    const message = { id: `m1`, rootId: `r1`, sessionId: `s1`, authorKind: `agent`, createdAt: 5 }
    await core.putMessages([{ ...message, body: null }])
    await core.putMessages([{ ...message, body: `The café is fixed.` }])
    await core.putMessages([{ ...message, body: null }])
    const found = (await core.searchChat(`r1`, `CAFE`, 10)) as { total: number; hits: { messageId?: string }[] }
    expect(found.total).toBe(1)
    expect(found.hits[0]!.messageId).toBe(`m1`)
  })

  it(`walks a holder's store to complete and resumes from the watermark`, async () => {
    const wasm = await loadTestWasm()
    const holder = new ChatStoreCore(wasm, memoryOpfs().host)
    const asker = new ChatStoreCore(wasm, memoryOpfs().host)
    const events: StoredEvent[] = []
    for (let seq = 1; seq <= 450; seq++) events.push(event(seq, { kind: `narration`, text: `n${seq}` }))
    await holder.putEvents(`r1`, events)
    // The holder learned it whole (a caught-up reader): its fullTo.
    await holder.take(`r1`, `s1`, `d1`, null, { seqs: [], events: [], throughSeq: 450, done: true, fullTo: 450 })
    let cursor: number | null | undefined = undefined
    let asks = 0
    for (;;) {
      const ask = await asker.ask(`r1`, `s1`, `d1`, cursor)
      cursor = ask.afterSeq
      const page = { t: `history_page`, requestId: `q${++asks}`, deviceId: `d1`, limit: 200, have: ask.have, minAt: ask.minAt, ...(cursor === null ? {} : { afterSeq: cursor }) }
      const chunk = JSON.parse((await holder.answer(`r1`, `s1`, page, 1 << 20))!)
      const took = await asker.take(`r1`, `s1`, `d1`, cursor, chunk)
      if (took.end) break
      cursor = took.next
    }
    expect(asks).toBe(3)
    expect((await asker.openTail(`r1`, `s1`, `d1`, 1000)).events.length).toBe(450)
    expect((await asker.ask(`r1`, `s1`, `d1`, undefined)).afterSeq).toBe(450)
    const refused = JSON.parse(holder.refusal({ requestId: `x`, deviceId: `d1`, limit: 5, have: [] }, `unauthorized`)!)
    expect(refused).toMatchObject({ t: `history_chunk`, error: `unauthorized`, seqs: [] })
    expect(holder.holders([{ endpointId: `e-run`, userId: `u2`, deviceId: `d1`, lastSeenAt: null }, { endpointId: `e-own`, userId: `u1`, deviceId: `d9`, lastSeenAt: null }], `d1`, `e-self`, `u1`, [])).toEqual([
      { endpointId: `e-run`, runner: true },
      { endpointId: `e-own`, runner: false },
    ])
  })
})
