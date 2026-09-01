import { describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// Isolate the pure logic — never touch a real DB / S3.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/storage/bun-s3-cleanup`, () => ({
  deleteStorageObjectsViaBun: vi.fn(),
}))
import {
  ORPHAN_SESSION_ATTACHMENT_RETENTION_MS,
  isOrphanSessionAttachmentPurgeDue,
  orphanSessionAttachmentCondition,
  reclaimOrphanSessionAttachmentsInTx,
} from "@/lib/session-attachment-sweep"

// Minimal chainable stub matching the drizzle calls
// reclaimOrphanSessionAttachmentsInTx makes:
// select(...).from(...).where(...).limit(...) → due rows;
// delete(...).where(...).returning() → the deleted rows.
function makeTx(opts: {
  dueRows: { id: string }[]
  deletedRows: { storageKey: string }[]
}) {
  return {
    select: () => ({
      from: () => ({
        where: () => ({
          limit: () => Promise.resolve(opts.dueRows),
        }),
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

describe(`isOrphanSessionAttachmentPurgeDue`, () => {
  const now = new Date(`2026-09-02T12:00:00Z`)

  it(`is true for an orphan older than the retention window`, () => {
    const createdAt = new Date(
      now.getTime() - ORPHAN_SESSION_ATTACHMENT_RETENTION_MS - 1000
    )
    expect(isOrphanSessionAttachmentPurgeDue(null, createdAt, now)).toBe(true)
  })

  it(`keeps an orphan still inside the retention window`, () => {
    const createdAt = new Date(
      now.getTime() - ORPHAN_SESSION_ATTACHMENT_RETENTION_MS + 60_000
    )
    expect(isOrphanSessionAttachmentPurgeDue(null, createdAt, now)).toBe(false)
  })

  it(`never touches a row still attached to a session, however old`, () => {
    // The run's transcript still embeds it — age is irrelevant while the FK
    // holds. Only the SET NULL from a coding_sessions delete makes a row due.
    const ancient = new Date(
      now.getTime() - 10 * ORPHAN_SESSION_ATTACHMENT_RETENTION_MS
    )
    expect(isOrphanSessionAttachmentPurgeDue(`sess-1`, ancient, now)).toBe(
      false
    )
  })

  it(`uses a 7 day window`, () => {
    expect(ORPHAN_SESSION_ATTACHMENT_RETENTION_MS).toBe(
      7 * 24 * 60 * 60 * 1000
    )
  })
})

describe(`orphanSessionAttachmentCondition`, () => {
  it(`encodes the same rule server-side: session_id IS NULL and aged out`, () => {
    const cutoff = new Date(`2026-08-26T12:00:00Z`)
    const condition = orphanSessionAttachmentCondition(cutoff)
    expect(condition).toBeDefined()
    const query = new PgDialect().sqlToQuery(condition!)
    const normalized = query.sql.toLowerCase()
    expect(normalized).toContain(`"session_id" is null`)
    expect(normalized).toContain(`"created_at" <=`)
    // drizzle serializes the timestamp param on the way to postgres.
    expect(query.params).toEqual([cutoff.toISOString()])
  })
})

describe(`reclaimOrphanSessionAttachmentsInTx`, () => {
  const cutoff = new Date(`2026-08-26T12:00:00Z`)

  it(`returns the storage keys of the rows it deleted, for post-commit S3 cleanup`, async () => {
    const tx = makeTx({
      dueRows: [{ id: `a1` }, { id: `a2` }],
      deletedRows: [{ storageKey: `k1` }, { storageKey: `k2` }],
    })
    const result = await reclaimOrphanSessionAttachmentsInTx(tx, cutoff)
    expect(result).toEqual({ storageKeys: [`k1`, `k2`] })
  })

  it(`does no delete and reclaims nothing when nothing is due`, async () => {
    // Nothing due covers both kept cases — the young orphan and the still
    // attached row are filtered out by the condition above.
    const tx = makeTx({ dueRows: [], deletedRows: [{ storageKey: `never` }] })
    const result = await reclaimOrphanSessionAttachmentsInTx(tx, cutoff)
    expect(result).toEqual({ storageKeys: [] })
  })

  it(`reclaims only what the re-checking delete actually removed`, async () => {
    // A concurrent purge (team delete) can take a row between the select and
    // the delete; only the returned rows may have their blobs dropped.
    const tx = makeTx({
      dueRows: [{ id: `a1` }, { id: `a2` }],
      deletedRows: [{ storageKey: `k1` }],
    })
    const result = await reclaimOrphanSessionAttachmentsInTx(tx, cutoff)
    expect(result).toEqual({ storageKeys: [`k1`] })
  })
})
