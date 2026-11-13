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
const upload = vi.fn()
vi.mock(`@/lib/storage/issue-image-upload`, () => ({
  uploadTeamSessionImageFile: (...args: unknown[]) => upload(...args),
}))

import { renderHook } from "@testing-library/react"
import { memoryOutboxFileStore, setOutboxFileStore } from "@/lib/outbox-files"
import {
  deleteOutboxEntry,
  resetOutboxForTests,
  retryOutboxEntry,
  sendThroughOutbox,
  useSessionOutbox,
} from "./use-session-outbox"

const stored = (sessionId: string) => JSON.parse(localStorage.getItem(`exp-outbox:${sessionId}`) ?? `[]`)
const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

// #FEED-237 (message-delivery.json): the session composer's outbox.
describe(`the session outbox`, () => {
  beforeEach(() => {
    localStorage.clear()
    resetOutboxForTests()
    send.mockReset()
    upload.mockReset()
    files = memoryOutboxFileStore()
    setOutboxFileStore(files)
  })
  let files = memoryOutboxFileStore()

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

  const pick = (name: string) => new File([name], name, { type: `image/png` })

  it(`FILES: the files are stored with the entry, each landed id kept as it lands, then the send carries them all`, async () => {
    upload.mockResolvedValueOnce({ id: `a1` }).mockResolvedValueOnce({ id: `a2` })
    send.mockResolvedValue({ sessionId: `s1`, messageId: `m1`, txid: 1 })
    const clientId = sendThroughOutbox({ teamId: `t1`, sessionId: `s1`, text: ``, files: [pick(`a.png`), pick(`b.png`)] })
    expect(stored(`s1`)).toMatchObject([{ clientId, pending: [`0:a.png`, `1:b.png`] }])
    expect(files.map.get(clientId)?.map((file) => file.name)).toEqual([`a.png`, `b.png`])
    await flush()
    await flush()
    expect(send.mock.calls[0]![0]).toMatchObject({ attachmentIds: [`a1`, `a2`], clientMessageId: clientId })
    expect(stored(`s1`)).toMatchObject([{ messageId: `m1`, attachmentIds: [`a1`, `a2`], pending: [] }])
    expect(files.map.has(clientId)).toBe(false)
  })

  it(`FILES: a failed upload keeps the landed id; Retry uploads only the rest`, async () => {
    upload.mockResolvedValueOnce({ id: `a1` }).mockRejectedValueOnce(new Error(`offline`))
    const clientId = sendThroughOutbox({ teamId: `t1`, sessionId: `s1`, text: `look`, files: [pick(`a.png`), pick(`b.png`)] })
    await flush()
    await flush()
    expect(send).not.toHaveBeenCalled()
    expect(stored(`s1`)).toMatchObject([{ state: `failed`, attachmentIds: [`a1`], pending: [`1:b.png`] }])
    upload.mockResolvedValueOnce({ id: `a2` })
    send.mockResolvedValue({ sessionId: `s1`, messageId: `m1`, txid: 1 })
    retryOutboxEntry(`s1`, clientId)
    await flush()
    await flush()
    expect(upload).toHaveBeenCalledTimes(3)
    expect((upload.mock.calls[2]![1] as File).name).toBe(`b.png`)
    expect(send.mock.calls[0]![0]).toMatchObject({ attachmentIds: [`a1`, `a2`] })
  })

  it(`FILES: a reload resumes a sending entry's remaining uploads from the store, then sends`, async () => {
    const entry = {
      clientId: `c1`,
      teamId: `t1`,
      text: `look`,
      createdAt: new Date().toISOString(),
      state: `sending`,
      messageId: null,
      attachmentIds: [`a1`],
      pending: [`1:b.png`],
      uploading: true,
    }
    localStorage.setItem(`exp-outbox:s1`, JSON.stringify([entry]))
    await files.put(`c1`, [{ key: `1:b.png`, name: `b.png`, type: `image/png`, blob: new Blob([`b`]) }])
    upload.mockResolvedValueOnce({ id: `a2` })
    send.mockResolvedValue({ sessionId: `s1`, messageId: `m1`, txid: 1 })
    renderHook(() => useSessionOutbox(`s1`, `u1`, []))
    await flush()
    await flush()
    expect((upload.mock.calls[0]![1] as File).name).toBe(`b.png`)
    expect(send.mock.calls[0]![0]).toMatchObject({ clientMessageId: `c1`, attachmentIds: [`a1`, `a2`] })
  })

  it(`FILES: a file that is gone fails the entry; Delete drops the stored files`, async () => {
    const entry = {
      clientId: `c1`,
      teamId: `t1`,
      text: ``,
      createdAt: new Date().toISOString(),
      state: `sending`,
      messageId: null,
      pending: [`0:a.png`],
    }
    localStorage.setItem(`exp-outbox:s1`, JSON.stringify([entry]))
    renderHook(() => useSessionOutbox(`s1`, `u1`, []))
    await flush()
    expect(upload).not.toHaveBeenCalled()
    expect(send).not.toHaveBeenCalled()
    expect(stored(`s1`)).toMatchObject([{ state: `failed` }])
    await files.put(`c1`, [{ key: `0:a.png`, name: `a.png`, type: `image/png`, blob: new Blob([`a`]) }])
    deleteOutboxEntry(`s1`, `c1`)
    await flush()
    expect(files.map.has(`c1`)).toBe(false)
  })
})
