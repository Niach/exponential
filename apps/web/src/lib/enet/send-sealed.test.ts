// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest"

// #EXP-1958 (sealed-content.json `cardStamps`, NEVER IN CLEAR): a tap on a
// sealed card hands OPENED text to `sendSessionMessage`. Whatever the writer
// could not seal (a `clearTree` target) is refused here: no request is made.

const sessions = new Map<string, Record<string, unknown>>()
const send = vi.fn(async (..._args: unknown[]) => ({ sessionId: `root`, txid: 1 }))

vi.mock(`@/lib/collections`, () => ({
  sessionCollection: { get: (id: string) => sessions.get(id), values: () => sessions.values() },
  deviceCollection: { values: () => [][Symbol.iterator]() },
  issueCollection: { values: () => [][Symbol.iterator]() },
  userCollection: { values: () => [][Symbol.iterator]() },
  messageCollection: { get: () => undefined, values: () => [][Symbol.iterator]() },
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: { sessions: { send: { mutate: (...args: unknown[]) => send(...args) } } } }))
vi.mock(`./current-key-holder`, () => ({ currentKeyHolder: () => undefined, awaitKeyHolder: async () => undefined }))

const { sendSessionMessage } = await import(`./send-sealed`)
const { NoKeyError } = await import(`./seal-send`)

const cardItem = { messageId: `m`, itemId: `i` }

beforeEach(() => {
  sessions.clear()
  send.mockClear()
  sessions.set(`root`, { id: `root`, rootId: `root`, parentId: null, teamId: `t`, userId: `u`, clearTree: true })
})

describe(`a tap's opened text never leaves in clear`, () => {
  it(`a reply into a clearTree is refused before any request`, async () => {
    await expect(sendSessionMessage({ sessionId: `root`, text: `Yes, ship it` }, undefined, { cardItem })).rejects.toBeInstanceOf(NoKeyError)
    expect(send).not.toHaveBeenCalled()
  })

  it(`Start into a clearTree is refused too (its text and name)`, async () => {
    await expect(
      sendSessionMessage({ parentId: `root`, text: `Add tests\n\nCover it`, name: `Add tests` }, undefined, { cardItem })
    ).rejects.toBeInstanceOf(NoKeyError)
    expect(send).not.toHaveBeenCalled()
  })

  it(`the composer's own send into a clearTree still goes in clear`, async () => {
    await sendSessionMessage({ sessionId: `root`, text: `hello` })
    expect(send).toHaveBeenCalledTimes(1)
    expect((send.mock.calls[0]![0] as { text: string }).text).toBe(`hello`)
  })
})
