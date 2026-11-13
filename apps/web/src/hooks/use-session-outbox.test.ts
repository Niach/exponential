import { beforeEach, describe, expect, it, vi } from "vitest"

// jsdom here does not always ship a working localStorage.
const map = new Map<string, string>()
Object.defineProperty(window, `localStorage`, {
  configurable: true,
  value: {
    get length() {
      return map.size
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => [...map.keys()][index] ?? null,
    removeItem: (key: string) => void map.delete(key),
    setItem: (key: string, value: string) => void map.set(key, value),
  },
})

const send = vi.fn()
vi.mock(`@/lib/enet/send-sealed`, () => ({ sendSessionMessage: (...args: unknown[]) => send(...args) }))
vi.mock(`@/lib/collections`, () => ({ messageCollection: { utils: { awaitTxId: () => Promise.resolve() } } }))

import { deleteOutboxEntry, resetOutboxForTests, retryOutboxEntry, sendThroughOutbox } from "./use-session-outbox"

const stored = (sessionId: string) => JSON.parse(localStorage.getItem(`exp-outbox:${sessionId}`) ?? `[]`)
const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

// #FEED-237 (message-delivery.json): the session composer's outbox.
describe(`the session outbox`, () => {
  beforeEach(() => {
    localStorage.clear()
    resetOutboxForTests()
    send.mockReset()
  })

  it(`persists the entry at the press and stores the server's messageId`, async () => {
    send.mockResolvedValue({ sessionId: `s1`, messageId: `m1`, txid: 1 })
    const clientId = sendThroughOutbox({ teamId: `t1`, sessionId: `s1`, text: ` ship it ` })
    expect(stored(`s1`)).toMatchObject([{ clientId, text: `ship it`, state: `sending`, messageId: null }])
    expect(send.mock.calls[0]![0]).toMatchObject({ sessionId: `s1`, text: `ship it`, clientMessageId: clientId })
    await flush()
    expect(stored(`s1`)).toMatchObject([{ clientId, messageId: `m1` }])
  })

  it(`a failure is the red mark; Retry resends with the SAME clientMessageId; Delete drops it`, async () => {
    send.mockRejectedValueOnce(new Error(`offline`))
    const clientId = sendThroughOutbox({ teamId: `t1`, sessionId: `s1`, text: `ship it` })
    await flush()
    expect(stored(`s1`)).toMatchObject([{ clientId, state: `failed` }])
    send.mockResolvedValue({ sessionId: `s1`, messageId: `m1`, txid: 1 })
    retryOutboxEntry(`s1`, clientId)
    await flush()
    expect(send.mock.calls[1]![0]).toMatchObject({ clientMessageId: clientId })
    expect(stored(`s1`)).toMatchObject([{ state: `sending`, messageId: `m1` }])
    deleteOutboxEntry(`s1`, clientId)
    expect(localStorage.getItem(`exp-outbox:s1`)).toBeNull()
  })
})
