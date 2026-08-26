import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-498: merge always closes. Two writers share the semantics — the in-tx
// sweep `endLiveIssueSessionsInTx` (run by applyPrMergeState's claim winner)
// and the standalone idempotent sweep `endMergedPrSessions` (mergePr's
// backstop for claim races). A structural fake `tx`
// (recording set values + where clause) plus mocked relay helpers is enough —
// the where clause is asserted by SHAPE, since a fake db cannot execute it
// (the coding-session-kill.test.ts pattern).
const h = vi.hoisted(() => ({
  updates: [] as { set: Record<string, unknown>; where: unknown }[],
  returning: [] as { id: string }[],
  getSteerRelayConfig: vi.fn(),
  relayPostKill: vi.fn(async () => ({ delivered: true })),
}))

function fakeTx() {
  return {
    update: () => ({
      set: (set: Record<string, unknown>) => ({
        where: (where: unknown) => ({
          returning: async () => {
            h.updates.push({ set, where })
            return h.returning
          },
        }),
      }),
    }),
  }
}

vi.mock(`@/db/connection`, () => ({
  db: {
    transaction: async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeTx()),
  },
}))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: async () => `1` }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayPostKill: h.relayPostKill,
}))

import {
  endLiveIssueSessionsInTx,
  endMergedPrSessions,
} from "@/lib/integrations/pr-sync"

const ISSUE = `11111111-1111-4111-8111-111111111111`
const ISSUE_2 = `22222222-2222-4222-8222-222222222222`

// Flatten a drizzle condition into its column names + bound values, in order:
// eq(a.b, `x`) ⇒ [`col:b`, `x`].
function whereShape(cond: unknown, out: unknown[] = []): unknown[] {
  if (!cond || typeof cond !== `object`) return out
  if (Array.isArray(cond)) {
    for (const child of cond) whereShape(child, out)
    return out
  }
  const rec = cond as Record<string, unknown>
  if (Array.isArray(rec.queryChunks)) return whereShape(rec.queryChunks, out)
  if (`value` in rec && `encoder` in rec) {
    out.push(rec.value)
    return out
  }
  if (typeof rec.name === `string` && rec.table) {
    out.push(`col:${rec.name}`)
    return out
  }
  return out
}

beforeEach(() => {
  h.updates.length = 0
  h.returning = []
  vi.clearAllMocks()
  h.getSteerRelayConfig.mockReturnValue({ url: `ws://relay`, secret: `s` })
  h.relayPostKill.mockResolvedValue({ delivered: true })
})

describe(`endLiveIssueSessionsInTx`, () => {
  it(`flips every live status to ended and returns the ids`, async () => {
    h.returning = [{ id: `sess-1` }, { id: `sess-2` }]

    const ids = await endLiveIssueSessionsInTx(
      fakeTx() as never,
      ISSUE
    )

    expect(ids).toEqual([`sess-1`, `sess-2`])
    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set).toMatchObject({
      status: `ended`,
      endedAt: expect.any(Date),
      updatedAt: expect.any(Date),
    })
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:issue_id`,
      ISSUE,
      `col:status`,
      `running`,
      `in_review`,
    ])
  })
})

describe(`endMergedPrSessions`, () => {
  it(`ends every live session across the linked issues and relays one kill each`, async () => {
    h.returning = [{ id: `sess-1` }, { id: `sess-2` }]

    await endMergedPrSessions([ISSUE, ISSUE_2])

    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set).toMatchObject({
      status: `ended`,
      endedAt: expect.any(Date),
      updatedAt: expect.any(Date),
    })
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:issue_id`,
      ISSUE,
      ISSUE_2,
      `col:status`,
      `running`,
      `in_review`,
    ])
    expect(h.relayPostKill).toHaveBeenCalledTimes(2)
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-1`)
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-2`)
  })

  it(`relays nothing when no row matched (idempotent re-run)`, async () => {
    await endMergedPrSessions([ISSUE])

    expect(h.updates).toHaveLength(1)
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })

  it(`is a no-op for an empty issue list`, async () => {
    await endMergedPrSessions([])

    expect(h.updates).toHaveLength(0)
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })
})
