import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1284: the tRPC half of threads. lib/threads.ts owns the data rules
// (same team, one thread per run) and is stubbed here; this locks WHO may do
// what — membership through resolveTeamAccess on the thread's team, a run
// moving only at its owner's or host's hand — plus the txid every write hands
// back and the BAD_REQUEST a data-rule refusal becomes.

const TEAM = `11111111-1111-4111-8111-111111111111`
const THREAD = `22222222-2222-4222-8222-222222222222`
const ISSUE = `33333333-3333-4333-8333-333333333333`
const RUN = `44444444-4444-4444-8444-444444444444`
const FRESH = `55555555-5555-4555-8555-555555555555`

const h = vi.hoisted(() => {
  class ThreadRuleError extends Error {}
  return {
    ThreadRuleError,
    resolveTeamAccess: vi.fn(async (..._args: unknown[]) => ({ kind: `member` })),
    getThread: vi.fn(),
    openThread: vi.fn(),
    linkIssueToThread: vi.fn(),
    unlinkIssueFromThread: vi.fn(),
    linkRunToThread: vi.fn(),
    unlinkRunFromThread: vi.fn(),
    listItemsOfThreads: vi.fn(),
  }
})

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/auth/access`, () => ({ resolveTeamAccess: h.resolveTeamAccess }))
vi.mock(`@/lib/threads`, () => ({
  ThreadRuleError: h.ThreadRuleError,
  getThread: h.getThread,
  openThread: h.openThread,
  linkIssueToThread: h.linkIssueToThread,
  unlinkIssueFromThread: h.unlinkIssueFromThread,
  linkRunToThread: h.linkRunToThread,
  unlinkRunFromThread: h.unlinkRunFromThread,
  listItemsOfThreads: h.listItemsOfThreads,
}))

import { threadsRouter } from "@/lib/trpc/threads"

const state = {
  // The run row loadOwnRun reads.
  run: null as Record<string, unknown> | null,
  updates: [] as Record<string, unknown>[],
  transactions: 0,
}

const fakeTx = {
  execute: async () => ({ rows: [{ txid: `42` }] }),
  update: () => ({
    set: (values: Record<string, unknown>) => ({
      where: () => ({
        returning: async () => {
          state.updates.push(values)
          return [{ id: THREAD, teamId: TEAM, hasVisibleItems: true, ...values }]
        },
      }),
    }),
  }),
}

function chain(rows: () => unknown[]) {
  const c: Record<string, unknown> = {}
  for (const name of [`from`, `where`, `limit`]) c[name] = () => c
  c.then = (resolve: (rows: unknown[]) => void) => resolve(rows())
  return c
}

function callerAs(userId: string) {
  return threadsRouter.createCaller({
    session: { user: { id: userId } },
    db: {
      select: () => chain(() => (state.run ? [state.run] : [])),
      transaction: async (fn: (tx: unknown) => Promise<unknown>) => {
        state.transactions += 1
        return fn(fakeTx)
      },
    },
    request: new Request(`http://localhost/`),
  } as never)
}

const caller = callerAs(`actor`)

beforeEach(() => {
  vi.clearAllMocks()
  state.run = { id: RUN, teamId: TEAM, userId: `actor`, hostUserId: null }
  state.updates = []
  state.transactions = 0
  h.resolveTeamAccess.mockResolvedValue({ kind: `member` })
  h.getThread.mockResolvedValue({ id: THREAD, teamId: TEAM, title: null })
  h.openThread.mockResolvedValue({ id: THREAD, teamId: TEAM })
  h.linkIssueToThread.mockResolvedValue({ added: true })
  h.unlinkIssueFromThread.mockResolvedValue({ removed: true })
  h.linkRunToThread.mockResolvedValue({ added: true, movedFrom: null })
  h.unlinkRunFromThread.mockResolvedValue({ removed: true, newThreadId: FRESH })
  h.listItemsOfThreads.mockResolvedValue([])
})

describe(`threads.get`, () => {
  it(`gates on the thread's team and drops hidden-board items + mirrors`, async () => {
    h.listItemsOfThreads.mockResolvedValue([
      { id: `a`, kind: `issue`, issueId: ISSUE, boardDeletedAt: null, boardArchivedAt: null },
      { id: `b`, kind: `issue`, issueId: ISSUE, boardDeletedAt: new Date(), boardArchivedAt: null },
      { id: `c`, kind: `issue`, issueId: ISSUE, boardDeletedAt: null, boardArchivedAt: new Date() },
      { id: `d`, kind: `run`, sessionId: RUN, boardDeletedAt: null, boardArchivedAt: null },
    ])
    const result = await caller.get({ id: THREAD })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(`actor`, TEAM, `read`)
    expect(result.items.map((item) => item.id)).toEqual([`a`, `d`])
    expect(result.items[0]).not.toHaveProperty(`boardDeletedAt`)
    expect(result.items[0]).not.toHaveProperty(`boardArchivedAt`)
  })

  it(`answers NOT_FOUND for a missing thread`, async () => {
    h.getThread.mockResolvedValue(null)
    await expect(caller.get({ id: THREAD })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`refuses a non-member`, async () => {
    h.resolveTeamAccess.mockRejectedValueOnce(new Error(`FORBIDDEN`))
    await expect(caller.get({ id: THREAD })).rejects.toThrow(`FORBIDDEN`)
    expect(h.listItemsOfThreads).not.toHaveBeenCalled()
  })
})

describe(`threads.create`, () => {
  it(`opens a thread holding the issues, under one txid`, async () => {
    const result = await caller.create({
      teamId: TEAM,
      title: `  Billing rework  `,
      issueIds: [ISSUE, ISSUE],
    })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(`actor`, TEAM, `create_issue`)
    expect(h.openThread).toHaveBeenCalledWith(fakeTx, {
      teamId: TEAM,
      createdBy: `actor`,
      title: `Billing rework`,
    })
    // De-duplicated, linked as the person.
    expect(h.linkIssueToThread).toHaveBeenCalledTimes(1)
    expect(h.linkIssueToThread).toHaveBeenCalledWith(
      fakeTx,
      { id: THREAD, teamId: TEAM },
      ISSUE,
      { userId: `actor`, source: `user` }
    )
    expect(result).toMatchObject({ txid: 42, thread: { id: THREAD } })
  })

  it(`refuses an issue of another team as BAD_REQUEST`, async () => {
    h.linkIssueToThread.mockRejectedValueOnce(
      new h.ThreadRuleError(`Issue not found in this thread's team`)
    )
    await expect(
      caller.create({ teamId: TEAM, issueIds: [ISSUE] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`refuses a non-member before writing`, async () => {
    h.resolveTeamAccess.mockRejectedValueOnce(new Error(`FORBIDDEN`))
    await expect(caller.create({ teamId: TEAM })).rejects.toThrow(`FORBIDDEN`)
    expect(state.transactions).toBe(0)
  })
})

describe(`threads.update`, () => {
  it(`sets a trimmed title`, async () => {
    const result = await caller.update({ id: THREAD, title: ` Ship it ` })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(`actor`, TEAM, `mutate_resources`)
    expect(state.updates[0]).toMatchObject({ title: `Ship it` })
    expect(result).toMatchObject({ txid: 42, thread: { title: `Ship it` } })
    // The visibility flag is server-only, like in the shape.
    expect(result.thread).not.toHaveProperty(`hasVisibleItems`)
  })

  it.each([null, ``, `   `])(`clears the title on %j`, async (title) => {
    await caller.update({ id: THREAD, title })
    expect(state.updates[0]).toMatchObject({ title: null })
  })

  it(`refuses a non-member`, async () => {
    h.resolveTeamAccess.mockRejectedValueOnce(new Error(`FORBIDDEN`))
    await expect(caller.update({ id: THREAD, title: `x` })).rejects.toThrow(`FORBIDDEN`)
    expect(state.updates).toEqual([])
  })
})

describe(`threads.link`, () => {
  it(`requires exactly one target`, async () => {
    await expect(caller.link({ id: THREAD })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(
      caller.link({ id: THREAD, issueId: ISSUE, sessionId: RUN })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`links an issue for any member`, async () => {
    const result = await caller.link({ id: THREAD, issueId: ISSUE })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(`actor`, TEAM, `comment`)
    expect(result).toEqual({ added: true, txid: 42 })
  })

  it(`refuses an issue of another team as BAD_REQUEST`, async () => {
    h.linkIssueToThread.mockRejectedValueOnce(
      new h.ThreadRuleError(`Issue not found in this thread's team`)
    )
    await expect(
      caller.link({ id: THREAD, issueId: ISSUE })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`moves the caller's own run`, async () => {
    const result = await caller.link({ id: THREAD, sessionId: RUN })
    expect(h.linkRunToThread).toHaveBeenCalledWith(
      fakeTx,
      { id: THREAD, teamId: TEAM },
      { id: RUN, teamId: TEAM },
      { userId: `actor`, source: `user` }
    )
    expect(result).toEqual({ added: true, txid: 42 })
  })

  it(`lets the shared-device host move the run`, async () => {
    state.run = { id: RUN, teamId: TEAM, userId: `requester`, hostUserId: `actor` }
    await caller.link({ id: THREAD, sessionId: RUN })
    expect(h.linkRunToThread).toHaveBeenCalledTimes(1)
  })

  it(`refuses a teammate's run`, async () => {
    state.run = { id: RUN, teamId: TEAM, userId: `someone`, hostUserId: null }
    await expect(
      caller.link({ id: THREAD, sessionId: RUN })
    ).rejects.toMatchObject({
      code: `FORBIDDEN`,
      message: `Only the session's owner can move it between threads`,
    })
    expect(h.linkRunToThread).not.toHaveBeenCalled()
  })

  it(`refuses a run of another team as BAD_REQUEST`, async () => {
    h.linkRunToThread.mockRejectedValueOnce(
      new h.ThreadRuleError(`That session belongs to another team`)
    )
    await expect(
      caller.link({ id: THREAD, sessionId: RUN })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`answers NOT_FOUND for a missing run`, async () => {
    state.run = null
    await expect(
      caller.link({ id: THREAD, sessionId: RUN })
    ).rejects.toMatchObject({ code: `NOT_FOUND`, message: `Session not found` })
  })

  it(`refuses a non-member`, async () => {
    h.resolveTeamAccess.mockRejectedValueOnce(new Error(`FORBIDDEN`))
    await expect(caller.link({ id: THREAD, issueId: ISSUE })).rejects.toThrow(`FORBIDDEN`)
    expect(h.linkIssueToThread).not.toHaveBeenCalled()
  })
})

describe(`threads.unlink`, () => {
  it(`drops an issue`, async () => {
    const result = await caller.unlink({ id: THREAD, issueId: ISSUE })
    expect(h.unlinkIssueFromThread).toHaveBeenCalledWith(fakeTx, THREAD, ISSUE)
    expect(result).toEqual({ removed: true, txid: 42, newThreadId: null })
  })

  it(`moves the owner's run into a fresh thread`, async () => {
    const result = await caller.unlink({ id: THREAD, sessionId: RUN })
    expect(result).toEqual({ removed: true, txid: 42, newThreadId: FRESH })
  })

  it(`refuses a teammate's run`, async () => {
    state.run = { id: RUN, teamId: TEAM, userId: `someone`, hostUserId: `other` }
    await expect(
      caller.unlink({ id: THREAD, sessionId: RUN })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect(h.unlinkRunFromThread).not.toHaveBeenCalled()
  })
})
