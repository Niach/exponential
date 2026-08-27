import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-637: the agent's own close-out. A structural fake db (recording the SET
// values and the where clause) is enough — the where clause is asserted by
// SHAPE, since a fake db cannot execute it (the coding-session-kill pattern).
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

const close = { summary: `Shipped the fix.`, outcome: `done` } as const

beforeEach(() => {
  selectResults.length = 0
  updates.length = 0
  updateReturning = [{ id: SESSION, status: `ended`, outcome: `done` }]
})

describe(`endSessionByAgent`, () => {
  it(`stamps the close-out, the agent end path and clears needsInput`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `actor`,
        hostUserId: null,
        status: `running`,
        outcome: null,
      },
    ])

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)

    expect(result).toEqual({
      sessionId: SESSION,
      status: `ended`,
      outcome: `done`,
      alreadyEnded: false,
    })
    expect(updates[0]!.values).toMatchObject({
      status: `ended`,
      endedBy: `agent`,
      summary: `Shipped the fix.`,
      outcome: `done`,
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

  it(`closes an in_review run (its PR is already open)`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `actor`,
        hostUserId: null,
        status: `in_review`,
        outcome: null,
      },
    ])

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)
    expect(result.alreadyEnded).toBe(false)
    expect(updates).toHaveLength(1)
  })

  it(`is idempotent and never overwrites an earlier close-out`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `actor`,
        hostUserId: null,
        status: `ended`,
        outcome: `blocked`,
      },
    ])

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)

    expect(result).toEqual({
      sessionId: SESSION,
      status: `ended`,
      outcome: `blocked`,
      alreadyEnded: true,
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
        outcome: null,
      },
    ])
    updateReturning = []

    const result = await endSessionByAgent(fakeDb, SESSION, `actor`, close)
    expect(result.alreadyEnded).toBe(true)
  })

  it(`lets the HOST end a run it operates for a teammate (EXP-432)`, async () => {
    selectResults.push([
      {
        id: SESSION,
        userId: `requester`,
        hostUserId: `actor`,
        status: `running`,
        outcome: null,
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
        outcome: null,
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
