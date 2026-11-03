import { describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  statements: [] as string[],
  deleteStorageObjectsViaBun: vi.fn(async () => undefined),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    transaction: async (fn: (tx: unknown) => Promise<unknown>) =>
      fn({
        execute: async (query: { queryChunks?: unknown[] }) => {
          mocks.statements.push(JSON.stringify(query.queryChunks ?? query))
          return { rows: [{ storage_key: `orphan`, poster_storage_key: null }] }
        },
      }),
  },
}))
vi.mock(`@/lib/storage/bun-s3-cleanup`, () => ({
  deleteStorageObjectsViaBun: mocks.deleteStorageObjectsViaBun,
}))

const {
  ORPHAN_SESSION_ATTACHMENT_RETENTION_MS,
  isOrphanSessionAttachmentPurgeDue,
  reclaimOrphanSessionAttachmentsInTx,
  runSessionAttachmentSweep,
} = await import(`@/lib/session-attachment-sweep`)

const now = new Date(`2026-10-13T12:00:00Z`)
const old = new Date(now.getTime() - ORPHAN_SESSION_ATTACHMENT_RETENTION_MS - 1000)
const orphan = { issueId: null, draftId: null, messageId: null, createdAt: old }

describe(`isOrphanSessionAttachmentPurgeDue`, () => {
  it(`reclaims an owner-less upload past the window`, () => {
    expect(isOrphanSessionAttachmentPurgeDue(orphan, false, now)).toBe(true)
  })

  it(`keeps a fresh upload (a composer that has not sent yet)`, () => {
    const fresh = { ...orphan, createdAt: new Date(now.getTime() - 60_000) }
    expect(isOrphanSessionAttachmentPurgeDue(fresh, false, now)).toBe(false)
  })

  it(`keeps a Guide picture and every owned row, however old`, () => {
    expect(isOrphanSessionAttachmentPurgeDue(orphan, true, now)).toBe(false)
    expect(isOrphanSessionAttachmentPurgeDue({ ...orphan, messageId: `m` }, false, now)).toBe(false)
    expect(isOrphanSessionAttachmentPurgeDue({ ...orphan, issueId: `i` }, false, now)).toBe(false)
    expect(isOrphanSessionAttachmentPurgeDue({ ...orphan, draftId: `d` }, false, now)).toBe(false)
    // VAPP-221: an app's record file is the app-file sweep's.
    expect(isOrphanSessionAttachmentPurgeDue({ ...orphan, appId: `a` }, false, now)).toBe(false)
  })
})

describe(`reclaimOrphanSessionAttachmentsInTx`, () => {
  it(`returns the deleted rows' blobs, posters included`, async () => {
    const tx = {
      execute: vi.fn(async () => ({
        rows: [
          { storage_key: `a`, poster_storage_key: null },
          { storage_key: `b`, poster_storage_key: `b.poster` },
        ],
      })),
    }
    const result = await reclaimOrphanSessionAttachmentsInTx(tx as never, now)
    expect(result.storageKeys).toEqual([`a`, `b`, `b.poster`])
    expect(tx.execute).toHaveBeenCalledTimes(1)
  })
})

describe(`runSessionAttachmentSweep`, () => {
  // NEW-43: `sessions.handoff` is gone (NEW-1); the sweep is the attachment
  // reclaim alone and never touches the sessions table's dropped columns.
  // VAPP-221: plus the app-file reclaim (rows no record of their app names).
  it(`runs the orphan and app-file reclaims, with no handoff statement`, async () => {
    mocks.statements.length = 0
    const result = await runSessionAttachmentSweep(now)
    expect(result).toEqual({ attachmentsDeleted: 2, objectsDeleted: 2 })
    expect(mocks.statements).toHaveLength(2)
    expect(mocks.statements[0]).toContain(`DELETE FROM attachments`)
    expect(mocks.statements[0]).toContain(`a.app_id IS NULL`)
    expect(mocks.statements[1]).toContain(`regexp_matches(r.data::text`)
    expect(mocks.statements.join()).not.toMatch(/handoff/)
    expect(mocks.deleteStorageObjectsViaBun).toHaveBeenCalledWith([`orphan`, `orphan`])
  })
})
