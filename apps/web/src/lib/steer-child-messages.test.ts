import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-700: the bracketed source-prefix convention is a wire contract between
// the server and every agent reading injected messages — lock the exact
// strings, and the notify helper's "never throws, no-ops unless a live
// parent" rules.

const h = vi.hoisted(() => {
  const dbRows: { current: Array<unknown> } = { current: [] }
  const queryBuilder: Record<string, unknown> = {}
  for (const method of [`from`, `leftJoin`, `where`, `limit`]) {
    queryBuilder[method] = vi.fn(() => queryBuilder)
  }
  ;(queryBuilder as { then: unknown }).then = (
    resolve: (v: unknown) => unknown,
    reject: (e: unknown) => unknown
  ) => Promise.resolve(dbRows.current).then(resolve, reject)
  const db = { select: vi.fn(() => queryBuilder) }
  return { dbRows, db }
})

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getSteerRelayConfig: vi.fn(),
  relayPostInput: vi.fn(),
}))

import { getSteerRelayConfig, relayPostInput } from "@/lib/steer"
import {
  childRunLabel,
  formatChildEndedSilently,
  formatChildFinished,
  formatChildQuestion,
  formatParentAnswer,
  formatStarterMessage,
  loadChildParentContext,
  notifyParentOfChildEnd,
} from "@/lib/steer-child-messages"
import type { Context } from "@/lib/trpc"

const CHILD = `66666666-6666-4666-8666-666666666666`
const PARENT = `77777777-7777-4777-8777-777777777777`
const RELAY = { url: `https://relay.test`, secret: `s` }

const db = h.db as unknown as Context[`db`]

const childRow = (over: Record<string, unknown> = {}) => ({
  id: CHILD,
  userId: `user-1`,
  hostUserId: null,
  startedReason: `agent`,
  parentSessionId: PARENT,
  actionName: null,
  issueIdentifier: `EXP-12`,
  parentStatus: `running`,
  ...over,
})

beforeEach(() => {
  vi.clearAllMocks()
  h.dbRows.current = []
  vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
  vi.mocked(relayPostInput).mockResolvedValue({ delivered: true })
})

describe(`childRunLabel`, () => {
  it(`prefers the issue identifier, then the action name, then the bare id`, () => {
    const base = { id: CHILD, issueIdentifier: null, actionName: null }
    expect(
      childRunLabel({ ...base, issueIdentifier: `EXP-12`, actionName: `n` })
    ).toBe(`EXP-12 66666666`)
    expect(childRunLabel({ ...base, actionName: `Nightly build` })).toBe(
      `Nightly build 66666666`
    )
    expect(childRunLabel(base)).toBe(`66666666`)
  })
})

describe(`message formats`, () => {
  const child = { id: CHILD, issueIdentifier: `EXP-12`, actionName: null }

  it(`locks the exact prefixes`, () => {
    expect(formatChildFinished(child, `Shipped it.`)).toBe(
      `[Exponential child run EXP-12 66666666 finished] Shipped it.`
    )
    expect(formatChildEndedSilently(child, `client`)).toBe(
      `[Exponential child run EXP-12 66666666 ended without a report (client)]`
    )
    expect(formatChildQuestion(child, `Which env?`)).toBe(
      `[Exponential child run EXP-12 66666666 asks — reply with exponential_sessions_message sessionId=${CHILD}] Which env?`
    )
    expect(formatStarterMessage(`Use staging.`)).toBe(
      `[Message from your starter via exponential_sessions_message] Use staging.`
    )
    expect(formatParentAnswer(PARENT, `Use staging.`)).toBe(
      `[Answer from your parent run 77777777 via exponential_sessions_message] Use staging.`
    )
  })

  // The submit convention is a separate \r frame — a newline inside the text
  // would submit early and fragment the message into several.
  it(`collapses newlines so the injection lands as ONE message`, () => {
    expect(formatChildFinished(child, `line one\n\nline two\r\nthree`)).toBe(
      `[Exponential child run EXP-12 66666666 finished] line one line two three`
    )
  })
})

describe(`loadChildParentContext`, () => {
  it(`returns the joined row, null when absent`, async () => {
    h.dbRows.current = [childRow()]
    expect(await loadChildParentContext(db, CHILD)).toEqual(childRow())
    h.dbRows.current = []
    expect(await loadChildParentContext(db, CHILD)).toBeNull()
  })
})

describe(`notifyParentOfChildEnd`, () => {
  it(`injects the summary message into a live parent`, async () => {
    h.dbRows.current = [childRow()]
    await expect(
      notifyParentOfChildEnd(db, CHILD, { summary: `Done.`, endedBy: `agent` })
    ).resolves.toEqual({ delivered: true })
    expect(relayPostInput).toHaveBeenCalledWith(
      RELAY,
      PARENT,
      `[Exponential child run EXP-12 66666666 finished] Done.`
    )
  })

  it(`injects the silent-end message when there is no summary`, async () => {
    h.dbRows.current = [childRow()]
    await notifyParentOfChildEnd(db, CHILD, { summary: null, endedBy: `client` })
    expect(relayPostInput).toHaveBeenCalledWith(
      RELAY,
      PARENT,
      `[Exponential child run EXP-12 66666666 ended without a report (client)]`
    )
  })

  it.each([
    [`no row`, []],
    [`not agent-started`, [childRow({ startedReason: `schedule` })]],
    [`no parent linked`, [childRow({ parentSessionId: null })]],
    [`parent ended`, [childRow({ parentStatus: `ended` })]],
    [`parent row gone`, [childRow({ parentStatus: null })]],
  ])(`no-ops when %s`, async (_name, rows) => {
    h.dbRows.current = rows
    await expect(
      notifyParentOfChildEnd(db, CHILD, { summary: `s`, endedBy: `agent` })
    ).resolves.toEqual({ delivered: false })
    expect(relayPostInput).not.toHaveBeenCalled()
  })

  it(`no-ops when the relay is not configured`, async () => {
    h.dbRows.current = [childRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(null)
    await expect(
      notifyParentOfChildEnd(db, CHILD, { summary: `s`, endedBy: `agent` })
    ).resolves.toEqual({ delivered: false })
    expect(relayPostInput).not.toHaveBeenCalled()
  })

  it(`never throws — a db failure reads as not-delivered`, async () => {
    h.db.select.mockImplementationOnce(() => {
      throw new Error(`boom`)
    })
    await expect(
      notifyParentOfChildEnd(db, CHILD, { summary: `s`, endedBy: `agent` })
    ).resolves.toEqual({ delivered: false })
  })
})
