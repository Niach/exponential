// @vitest-environment node
// #EDE-115 (chat-store.json `catchUp`, S4): two web loops over a fake eNet,
// each on its own store: the asker walks the holder's events to complete
// through steer envelopes, the holder refuses a stranger, and a caught-up
// session is not walked again until its row changes.

import { afterEach, describe, expect, it, vi } from "vitest"
import { encodeSteerEnvelope, parseSteerEnvelope } from "@exp/domain-contract/steer-enet"
import { startCatchUp, type CatchUpDeps, type CatchUpInstall, type CatchUpRow } from "./catch-up"
import { ChatStoreCore, type StoredEvent } from "./store-core"
import { loadTestWasm, memoryOpfs, wasmBuilt } from "./test-support"

type Listener = (from: string, body: Uint8Array) => void

/** A fake eNet: endpoints deliver chat bodies to each other's listeners. */
function fakeNet() {
  const listeners = new Map<string, Set<Listener>>()
  const sent: { from: string; to: string; types: string[] }[] = []
  const node = (endpointId: string): CatchUpDeps[`node`] => ({
    running: true,
    endpointId,
    sendChat: async (to, body) => {
      sent.push({ from: endpointId, to, types: parseSteerEnvelope(body)?.frames.map((f) => f.t) ?? [] })
      const targets = listeners.get(to)
      if (!targets) throw new Error(`unreachable`)
      setTimeout(() => targets.forEach((listener) => listener(endpointId, body)), 0)
    },
    onChat: (listener) => {
      const set = listeners.get(endpointId) ?? new Set()
      set.add(listener)
      listeners.set(endpointId, set)
      return () => set.delete(listener)
    },
  })
  return { node, sent }
}

const row: CatchUpRow = { id: `s1`, rootId: `r1`, teamId: `t1`, userId: `u1`, shared: false, deviceId: `d1`, updatedAt: `2026-11-17T10:00:00.000Z`, busy: false }
const installs: CatchUpInstall[] = [
  { endpointId: `e-web`, userId: `u1`, deviceId: null, lastSeenAt: null },
  { endpointId: `e-holder`, userId: `u2`, deviceId: `d1`, lastSeenAt: null },
]

function deps(core: ChatStoreCore, node: CatchUpDeps[`node`], userId: string, rows: CatchUpRow[]): CatchUpDeps {
  return {
    userId,
    node,
    store: async <T,>(op: string, ...args: unknown[]) =>
      (await (core as unknown as Record<string, (...a: unknown[]) => unknown>)[op]!.apply(core, args)) as T,
    sessions: () => rows,
    onSessionsChange: () => () => {},
    installs: async () => installs,
    power: () => `ac`,
    idleMs: () => 0,
  }
}

const stops: (() => void)[] = []
afterEach(() => {
  while (stops.length) stops.pop()!()
})

describe.skipIf(!wasmBuilt)(`the web's eNet catch-up`, () => {
  it(`walks the runner's events into the store and answers as a holder`, async () => {
    const wasm = await loadTestWasm()
    const net = fakeNet()
    const holderStore = new ChatStoreCore(wasm, memoryOpfs().host)
    const events: StoredEvent[] = []
    for (let seq = 1; seq <= 450; seq++) {
      events.push({ rootId: `r1`, sessionId: `s1`, deviceId: `d1`, seq, storedAt: Date.now(), event: { kind: `narration`, text: `n${seq}` } })
    }
    await holderStore.putEvents(`r1`, events)
    await holderStore.take(`r1`, `s1`, `d1`, null, { seqs: [], events: [], throughSeq: 450, done: true, fullTo: 450 })
    const holderRows = [row]
    const holderDeps = deps(holderStore, net.node(`e-holder`), `u2`, holderRows)
    stops.push(startCatchUp({ ...holderDeps, node: { ...holderDeps.node, running: false } }))

    const askerStore = new ChatStoreCore(wasm, memoryOpfs().host)
    stops.push(startCatchUp(deps(askerStore, net.node(`e-web`), `u1`, [row])))
    await vi.waitFor(async () => expect((await askerStore.openTail(`r1`, `s1`, `d1`, 1000)).events.length).toBe(450), { timeout: 5_000 })
    expect((await askerStore.ask(`r1`, `s1`, `d1`, undefined)).afterSeq).toBe(450)
    const pages = net.sent.filter((s) => s.from === `e-web` && s.types.includes(`history_page`))
    expect(pages.length).toBe(3)
    expect(pages.every((p) => p.to === `e-holder`)).toBe(true)
    await vi.waitFor(async () => expect(await askerStore.meta(`_index`, `catchup.checked.s1`)).toBe(`d1|2026-11-17T10:00:00.000Z`))
  })

  it(`refuses a page from an install that is not the starter's on a private chat`, async () => {
    const wasm = await loadTestWasm()
    const net = fakeNet()
    const holderStore = new ChatStoreCore(wasm, memoryOpfs().host)
    const holderDeps = deps(holderStore, net.node(`e-holder`), `u2`, [{ ...row, userId: `someone-else` }])
    stops.push(startCatchUp({ ...holderDeps, node: { ...holderDeps.node, running: false } }))
    const answers: unknown[] = []
    const asker = net.node(`e-web`)
    asker.onChat((_, body) => answers.push(...(parseSteerEnvelope(body)?.frames ?? [])))
    await asker.sendChat(`e-holder`, encodeSteerEnvelope(`s1`, [{ t: `history_page`, requestId: `q1`, deviceId: `d1`, limit: 10, have: [] }]))
    await vi.waitFor(() => expect(answers).toEqual([expect.objectContaining({ t: `history_chunk`, requestId: `q1`, error: `unauthorized` })]))
    await asker.sendChat(`e-holder`, encodeSteerEnvelope(`nope`, [{ t: `history_page`, requestId: `q2`, deviceId: `d1`, limit: 10, have: [] }]))
    await vi.waitFor(() => expect(answers[1]).toMatchObject({ requestId: `q2`, error: `no_such_session` }))
  })
})
