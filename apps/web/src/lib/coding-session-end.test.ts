import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-637: the agent's own close-out; EXP-673: it ends only an
// automation-started run. A structural fake db (recording the SET values and
// the where clause) is enough — the where clause is asserted by SHAPE, since a
// fake db cannot execute it (the coding-session-kill pattern).
vi.mock(`@/db/connection`, () => ({ db: {} }))

import { endSessionByAgent } from "@/lib/coding-session-end"

const SESSION = `11111111-1111-4111-8111-111111111111`

const selectResults: unknown[][] = []
const updates: { values: Record<string, unknown>; where: unknown }[] = []
let updateReturning: unknown[] = []

const fakeDb = {
  select: () => ({
    from: () => ({
      where: () => ({ limit: async () => selectResults.shift() ?? [] }),
    }),
  }),
  update: () => ({
    set: (values: Record<string, unknown>) => ({
      where: (where: unknown) => ({
        returning: async () => {
          updates.push({ values, where })
          return updateReturning
        },
      }),
    }),
  }),
} as never

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

const close = { summary: `Shipped the fix.` } as const

beforeEach(() => {
  selectResults.length = 0
  updates.length = 0
  updateReturning = [{ id: SESSION, status: `ended` }]
})

describe(`endSessionByAgent`, () => {
  it(`ends an automation-started run: stamps the close-out, the agent end path and clears needsInput`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `actor`,
        hostUserId: null,
        status: `running`,
        startedReason: `schedule`,
      },
    ])

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)

    expect(result).toEqual({
      sessionId: SESSION,
      status: `ended`,
      alreadyEnded: false,
      keptOpen: false,
    })
    expect(updates[0]!.values).toMatchObject({
      status: `ended`,
      endedBy: `agent`,
      summary: `Shipped the fix.`,
      needsInput: false,
    })
    // Status-fenced so a close-out racing a kill can't resurrect the row.
    expect(whereShape(updates[0]!.where)).toEqual([
      `col:id`,
      SESSION,
      `col:status`,
      `running`,
      `in_review`,
    ])
  })

  // EXP-673: a person is (or may come back to be) on the other side of a
  // run they started — the report lands, the conversation stays open.
  it(`records the close-out but keeps a person-started run live`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `actor`,
        hostUserId: null,
        status: `running`,
        startedReason: null,
      },
    ])
    updateReturning = [{ id: SESSION, status: `running` }]

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)

    expect(result).toEqual({
      sessionId: SESSION,
      status: `running`,
      alreadyEnded: false,
      keptOpen: true,
    })
    // Summary only: no status flip, no ended stamp, no end path, and
    // needsInput is left for the idle nudge to own.
    expect(Object.keys(updates[0]!.values).sort()).toEqual([
      `summary`,
      `updatedAt`,
    ])
    expect(updates[0]!.values).toMatchObject({
      summary: `Shipped the fix.`,
    })
    // Still status-fenced: a close-out racing a kill must not stamp a
    // summary onto a row that just ended.
    expect(whereShape(updates[0]!.where)).toEqual([
      `col:id`,
      SESSION,
      `col:status`,
      `running`,
      `in_review`,
    ])
  })

  it(`closes an automation-started in_review run (its PR is already open)`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `actor`,
        hostUserId: null,
        status: `in_review`,
        startedReason: `event`,
      },
    ])

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)
    expect(result.alreadyEnded).toBe(false)
    expect(result.keptOpen).toBe(false)
    expect(updates).toHaveLength(1)
    expect(updates[0]!.values).toMatchObject({ status: `ended` })
  })

  it(`is idempotent and never overwrites an earlier close-out`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `actor`,
        hostUserId: null,
        status: `ended`,
        startedReason: null,
      },
    ])

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)

    expect(result).toEqual({
      sessionId: SESSION,
      status: `ended`,
      alreadyEnded: true,
      keptOpen: false,
    })
    expect(updates).toHaveLength(0)
  })

  it(`reports alreadyEnded when it loses the race to a concurrent end`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `actor`,
        hostUserId: null,
        status: `running`,
        startedReason: `schedule`,
      },
    ])
    updateReturning = []

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)
    expect(result.alreadyEnded).toBe(true)
    expect(result.keptOpen).toBe(false)
  })

  it(`lets the HOST end a run it operates for a teammate (EXP-432)`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `requester`,
        hostUserId: `actor`,
        status: `running`,
        startedReason: null,
      },
    ])

    await endSessionByAgent(fakeDb, SESSION, `actor`, close)
    expect(updates).toHaveLength(1)
  })

  it(`refuses a stranger and a vanished row`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `someone-else`,
        hostUserId: null,
        status: `running`,
        startedReason: null,
      },
    ])
    await expect(
      endSessionByAgent(fakeDb, SESSION, `actor`, close)
    ).rejects.toMatchObject({ code: `FORBIDDEN` })

    selectResults.push([])
    const error = await endSessionByAgent(fakeDb, SESSION, `actor`, close).catch(
      (e: unknown) => e
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect(updates).toHaveLength(0)
  })
})
