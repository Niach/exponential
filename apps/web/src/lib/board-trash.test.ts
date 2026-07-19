import { describe, expect, it, vi } from "vitest"
import { BOARD_TRASH_RETENTION_MS } from "@exp/db-schema/domain"

// Isolate the pure logic — never touch a real DB / S3 / cache.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: vi.fn(),
}))
import { isBoardPurgeDue, purgeBoardInTx } from "@/lib/board-trash"

// Minimal chainable stub matching the drizzle calls purgeBoardInTx makes:
// select(...).from(...).where(...) → attachment rows; delete(...).where(...)
// .returning() → the deleted board rows.
function makeTx(opts: {
  attachmentRows: { storageKey: string }[]
  deletedRows: { id: string }[]
}) {
  return {
    select: () => ({
      from: () => ({
        where: () => Promise.resolve(opts.attachmentRows),
      }),
    }),
    delete: () => ({
      where: () => ({
        returning: () => Promise.resolve(opts.deletedRows),
      }),
    }),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any
}

describe(`isBoardPurgeDue`, () => {
  const now = new Date(`2026-07-09T12:00:00Z`)

  it(`is false for a board that is not trashed`, () => {
    expect(isBoardPurgeDue(null, now)).toBe(false)
  })

  it(`is true once the retention window has fully elapsed`, () => {
    const deletedAt = new Date(now.getTime() - BOARD_TRASH_RETENTION_MS - 1000)
    expect(isBoardPurgeDue(deletedAt, now)).toBe(true)
  })

  it(`is false while still inside the retention window`, () => {
    const deletedAt = new Date(now.getTime() - BOARD_TRASH_RETENTION_MS + 60_000)
    expect(isBoardPurgeDue(deletedAt, now)).toBe(false)
  })
})

describe(`purgeBoardInTx`, () => {
  const cutoff = new Date(`2026-07-09T12:00:00Z`)

  it(`skips S3 deletion when a concurrent restore wins the race (0 rows deleted)`, async () => {
    const tx = makeTx({ attachmentRows: [{ storageKey: `k1` }], deletedRows: [] })
    const result = await purgeBoardInTx(tx, `p1`, cutoff)
    expect(result).toEqual({ purged: false, storageKeys: [] })
  })

  it(`returns the attachment storage keys when the board is purged`, async () => {
    const tx = makeTx({
      attachmentRows: [{ storageKey: `k1` }, { storageKey: `k2` }],
      deletedRows: [{ id: `p1` }],
    })
    const result = await purgeBoardInTx(tx, `p1`, cutoff)
    expect(result).toEqual({ purged: true, storageKeys: [`k1`, `k2`] })
  })

  it(`reports purged with no keys when the board has no attachments`, async () => {
    const tx = makeTx({ attachmentRows: [], deletedRows: [{ id: `p1` }] })
    const result = await purgeBoardInTx(tx, `p1`, cutoff)
    expect(result).toEqual({ purged: true, storageKeys: [] })
  })
})
