import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/storage/bun-s3-cleanup`, () => ({ deleteStorageObjectsViaBun: vi.fn() }))

const {
  ORPHAN_SESSION_ATTACHMENT_RETENTION_MS,
  isOrphanSessionAttachmentPurgeDue,
  reclaimOrphanSessionAttachmentsInTx,
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
