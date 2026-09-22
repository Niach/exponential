import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-955: the size backfill sweep picks the due rows and runs each through
// the ONE finalize function in backfill mode; a missing object is counted,
// not raised, so one abandoned mint never aborts the pass.

const h = vi.hoisted(() => ({
  due: [] as { id: string }[],
  // One result set per pass, in call order; falls back to `due`.
  queue: [] as { id: string }[][],
  wheres: [] as unknown[],
  orderBy: vi.fn(),
  finalize: vi.fn(),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        where: (condition: unknown) => {
          h.wheres.push(condition)
          return {
            orderBy: (...args: unknown[]) => {
              h.orderBy(...args)
              return { limit: async () => h.queue.shift() ?? h.due }
            },
          }
        },
      }),
    }),
  },
  pool: {},
}))

vi.mock(`@/lib/attachments/finalize-core`, () => ({
  finalizeAttachmentUploadWith: h.finalize,
}))

vi.mock(`@/lib/storage/bun-s3-cleanup`, () => ({
  bunAttachmentObjectProbe: () => null,
}))

import { TRPCError } from "@trpc/server"
import type { SQL } from "drizzle-orm"
import { PgDialect } from "drizzle-orm/pg-core"
import {
  SIZE_BACKFILL_GRACE_MS,
  isSizeBackfillDue,
  resetSizeBackfillCursor,
  runAttachmentSizeBackfill,
  sizeBackfillCursor,
} from "@/lib/attachment-size-backfill"

function whereOf(index: number) {
  return new PgDialect().sqlToQuery(h.wheres[index] as SQL)
}

const probe = {
  head: vi.fn(),
  read: vi.fn(),
  remove: vi.fn(),
}

beforeEach(() => {
  h.due = []
  h.queue = []
  h.wheres = []
  h.orderBy.mockClear()
  h.finalize.mockReset()
  resetSizeBackfillCursor()
})

describe(`isSizeBackfillDue`, () => {
  const now = new Date(`2026-09-20T12:00:00Z`)

  it(`is due for a 0-size row older than the grace window`, () => {
    const createdAt = new Date(now.getTime() - SIZE_BACKFILL_GRACE_MS - 1)
    expect(isSizeBackfillDue(0, createdAt, now)).toBe(true)
  })

  it(`waits out the grace window so a live signed upload is left alone`, () => {
    const createdAt = new Date(now.getTime() - SIZE_BACKFILL_GRACE_MS + 1000)
    expect(isSizeBackfillDue(0, createdAt, now)).toBe(false)
  })

  it(`never touches a row that already has a size`, () => {
    const createdAt = new Date(`2020-01-01T00:00:00Z`)
    expect(isSizeBackfillDue(1, createdAt, now)).toBe(false)
  })
})

describe(`runAttachmentSizeBackfill`, () => {
  it(`finalizes every due row in backfill mode and tallies the outcomes`, async () => {
    h.due = [{ id: `a` }, { id: `b` }, { id: `c` }, { id: `d` }]
    h.finalize.mockImplementation(async (id: string) => {
      if (id === `a`) return { id, sizeBytes: 22_000 }
      if (id === `b`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `no bytes`,
        })
      }
      if (id === `c`) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `gone` })
      }
      throw new Error(`storage unreachable`)
    })
    const errorSpy = vi.spyOn(console, `error`).mockImplementation(() => {})

    const result = await runAttachmentSizeBackfill(probe)

    expect(result).toEqual({
      candidates: 4,
      healed: 1,
      missing: 2,
      failed: 1,
    })
    expect(h.finalize).toHaveBeenCalledTimes(4)
    for (const call of h.finalize.mock.calls) {
      expect(call[1]).toBe(probe)
      expect(call[2]).toEqual({ mode: `backfill` })
    }
    errorSpy.mockRestore()
  })

  it(`walks the due set in id order with a cursor across passes, so permanently missing rows cannot starve the rows behind them`, async () => {
    // Every object is gone for good: the rows stay due forever.
    h.finalize.mockImplementation(async (id: string) => ({ id, sizeBytes: 0 }))
    h.queue = [
      [{ id: `a` }, { id: `b` }],
      [{ id: `c` }],
      [{ id: `a` }, { id: `b` }],
    ]

    // A full batch: the next pass resumes after its last id.
    await runAttachmentSizeBackfill(probe, new Date(), 2)
    expect(sizeBackfillCursor()).toBe(`b`)
    expect(whereOf(0).params).not.toContain(`b`)

    // The second pass asks only for ids past the cursor and reaches the tail;
    // a short batch means the end, so the walk starts over.
    const second = await runAttachmentSizeBackfill(probe, new Date(), 2)
    expect(whereOf(1).sql).toMatch(/"id" > \$\d/)
    expect(whereOf(1).params).toContain(`b`)
    expect(second.candidates).toBe(1)
    expect(sizeBackfillCursor()).toBeNull()

    await runAttachmentSizeBackfill(probe, new Date(), 2)
    expect(whereOf(2).sql).not.toMatch(/"id" >/)
    expect(h.orderBy).toHaveBeenCalledTimes(3)
    expect(h.finalize.mock.calls.map((call) => call[0])).toEqual([
      `a`,
      `b`,
      `c`,
      `a`,
      `b`,
    ])
    // Backfill mode never deletes.
    expect(probe.remove).not.toHaveBeenCalled()
  })

  it(`is a no-op pass when nothing is due`, async () => {
    const result = await runAttachmentSizeBackfill(probe)
    expect(result).toEqual({ candidates: 0, healed: 0, missing: 0, failed: 0 })
    expect(h.finalize).not.toHaveBeenCalled()
  })
})
