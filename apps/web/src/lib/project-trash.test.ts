import { describe, expect, it, vi } from "vitest"
import { PROJECT_TRASH_RETENTION_MS } from "@exp/db-schema/domain"

// Isolate the pure logic — never touch a real DB / S3 / cache.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: vi.fn(),
}))
import { isProjectPurgeDue, purgeProjectInTx } from "@/lib/project-trash"

// Minimal chainable stub matching the drizzle calls purgeProjectInTx makes:
// select(...).from(...).where(...) → attachment rows; delete(...).where(...)
// .returning() → the deleted project rows.
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

describe(`isProjectPurgeDue`, () => {
  const now = new Date(`2026-07-09T12:00:00Z`)

  it(`is false for a project that is not trashed`, () => {
    expect(isProjectPurgeDue(null, now)).toBe(false)
  })

  it(`is true once the retention window has fully elapsed`, () => {
    const deletedAt = new Date(now.getTime() - PROJECT_TRASH_RETENTION_MS - 1000)
    expect(isProjectPurgeDue(deletedAt, now)).toBe(true)
  })

  it(`is false while still inside the retention window`, () => {
    const deletedAt = new Date(now.getTime() - PROJECT_TRASH_RETENTION_MS + 60_000)
    expect(isProjectPurgeDue(deletedAt, now)).toBe(false)
  })
})

describe(`purgeProjectInTx`, () => {
  const cutoff = new Date(`2026-07-09T12:00:00Z`)

  it(`skips S3 deletion when a concurrent restore wins the race (0 rows deleted)`, async () => {
    const tx = makeTx({ attachmentRows: [{ storageKey: `k1` }], deletedRows: [] })
    const result = await purgeProjectInTx(tx, `p1`, cutoff)
    expect(result).toEqual({ purged: false, storageKeys: [] })
  })

  it(`returns the attachment storage keys when the project is purged`, async () => {
    const tx = makeTx({
      attachmentRows: [{ storageKey: `k1` }, { storageKey: `k2` }],
      deletedRows: [{ id: `p1` }],
    })
    const result = await purgeProjectInTx(tx, `p1`, cutoff)
    expect(result).toEqual({ purged: true, storageKeys: [`k1`, `k2`] })
  })

  it(`reports purged with no keys when the project has no attachments`, async () => {
    const tx = makeTx({ attachmentRows: [], deletedRows: [{ id: `p1` }] })
    const result = await purgeProjectInTx(tx, `p1`, cutoff)
    expect(result).toEqual({ purged: true, storageKeys: [] })
  })
})
