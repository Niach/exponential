import { describe, expect, it } from "vitest"
import { attachments, issues } from "@/db/schema"
import { cloneIssueForRecurrence } from "@/lib/issue-recurrence"

const REQUEST_URL = `http://localhost:5173/api/trpc/issues.update`
const SOURCE_ISSUE_ID = `11111111-1111-4111-8111-111111111111`
const SOURCE_WORKSPACE_ID = `22222222-2222-4222-8222-222222222222`
const SOURCE_PROJECT_ID = `33333333-3333-4333-8333-333333333333`
const OLD_ATTACHMENT_ID = `44444444-4444-4444-8444-444444444444`

interface SourceAttachmentRow {
  id: string
  uploaderId: string
  filename: string
  contentType: string
  sizeBytes: number
  storageKey: string
  width: number | null
  height: number | null
}

// Minimal chainable Drizzle-tx stand-in: `select().from().where()` resolves to
// the seeded source attachments, `insert(table).values(...)` records the rows
// (and is awaitable / supports `.returning()`), `execute()` is a no-op.
function makeMockTx(sourceAttachments: SourceAttachmentRow[]) {
  const insertedIssues: Array<Record<string, unknown>> = []
  const insertedAttachments: Array<Record<string, unknown>> = []

  const tx = {
    select: () => ({
      from: () => ({
        where: () => Promise.resolve(sourceAttachments),
      }),
    }),
    insert: (table: unknown) => ({
      values: (vals: Record<string, unknown> | Array<Record<string, unknown>>) => {
        if (table === issues) {
          insertedIssues.push(vals as Record<string, unknown>)
        } else if (table === attachments) {
          insertedAttachments.push(
            ...(Array.isArray(vals) ? vals : [vals])
          )
        }
        const result = table === issues ? [{ ...(vals as object) }] : undefined
        const promise = Promise.resolve(result)
        return {
          returning: () => Promise.resolve([{ ...(vals as object) }]),
          then: (onF: never, onR: never) => promise.then(onF, onR),
        }
      },
    }),
    execute: () => Promise.resolve(undefined),
  }

  return { tx, insertedIssues, insertedAttachments }
}

const baseParams = {
  sourceIssueId: SOURCE_ISSUE_ID,
  sourceProjectId: SOURCE_PROJECT_ID,
  sourceWorkspaceId: SOURCE_WORKSPACE_ID,
  sourceTitle: `Water the plants`,
  sourcePriority: `none` as const,
  sourceAssigneeId: null,
  recurrenceInterval: 1,
  recurrenceUnit: `week` as const,
  creatorId: `user-1`,
  requestUrl: REQUEST_URL,
}

describe(`cloneIssueForRecurrence`, () => {
  it(`deep-clones referenced images into new rows owned by the clone`, async () => {
    const sourceRow: SourceAttachmentRow = {
      id: OLD_ATTACHMENT_ID,
      uploaderId: `uploader-1`,
      filename: `photo.png`,
      contentType: `image/png`,
      sizeBytes: 1234,
      storageKey: `issues/${SOURCE_ISSUE_ID}/${OLD_ATTACHMENT_ID}-photo.png`,
      width: 800,
      height: 600,
    }
    const { tx, insertedIssues, insertedAttachments } = makeMockTx([sourceRow])

    const result = await cloneIssueForRecurrence(tx as never, {
      ...baseParams,
      sourceDescription: {
        text: `Before\n\n![a photo](/api/attachments/${OLD_ATTACHMENT_ID})\n\nAfter`,
      },
    })

    // The clone issue was inserted with a fresh, explicit id.
    expect(insertedIssues).toHaveLength(1)
    const cloneIssueId = insertedIssues[0].id as string
    expect(cloneIssueId).not.toBe(SOURCE_ISSUE_ID)

    // Exactly one new attachment row, owned by the clone, with a new id + key.
    expect(insertedAttachments).toHaveLength(1)
    const cloned = insertedAttachments[0]
    expect(cloned.id).not.toBe(OLD_ATTACHMENT_ID)
    expect(cloned.issueId).toBe(cloneIssueId)
    expect(cloned.workspaceId).toBe(SOURCE_WORKSPACE_ID)
    expect(cloned.uploaderId).toBe(`uploader-1`)
    expect(cloned.filename).toBe(`photo.png`)
    expect(cloned.contentType).toBe(`image/png`)
    expect(cloned.sizeBytes).toBe(1234)
    expect(cloned.width).toBe(800)
    expect(cloned.height).toBe(600)
    expect(cloned.storageKey).toBe(
      `issues/${cloneIssueId}/${cloned.id as string}-photo.png`
    )
    expect(cloned.url).toBe(`/api/attachments/${cloned.id as string}`)

    // The clone's description references the NEW attachment, not the source's.
    const clonedText = (insertedIssues[0].description as { text: string }).text
    expect(clonedText).toContain(`/api/attachments/${cloned.id as string}`)
    expect(clonedText).not.toContain(OLD_ATTACHMENT_ID)
    expect(clonedText).toContain(`Before`)
    expect(clonedText).toContain(`After`)

    // The storage object copy is planned old-key → new-key.
    expect(result.attachmentCopies).toEqual([
      {
        sourceKey: sourceRow.storageKey,
        destKey: cloned.storageKey,
      },
    ])
  })

  it(`strips images whose attachment row is missing instead of leaving a dangling ref`, async () => {
    const presentId = OLD_ATTACHMENT_ID
    const missingId = `55555555-5555-4555-8555-555555555555`
    const sourceRow: SourceAttachmentRow = {
      id: presentId,
      uploaderId: `uploader-1`,
      filename: `kept.png`,
      contentType: `image/png`,
      sizeBytes: 10,
      storageKey: `issues/${SOURCE_ISSUE_ID}/${presentId}-kept.png`,
      width: null,
      height: null,
    }
    // SELECT only returns the present row — the missing id resolves to nothing.
    const { tx, insertedIssues, insertedAttachments } = makeMockTx([sourceRow])

    const result = await cloneIssueForRecurrence(tx as never, {
      ...baseParams,
      sourceDescription: {
        text: `![kept](/api/attachments/${presentId})\n\n![gone](/api/attachments/${missingId})`,
      },
    })

    // Only the present attachment is cloned.
    expect(insertedAttachments).toHaveLength(1)
    const cloned = insertedAttachments[0]
    expect(result.attachmentCopies).toHaveLength(1)

    const clonedText = (insertedIssues[0].description as { text: string }).text
    // Present image rewritten to the clone's new attachment.
    expect(clonedText).toContain(`/api/attachments/${cloned.id as string}`)
    // Missing image stripped entirely — no dangling source reference survives.
    expect(clonedText).not.toContain(missingId)
    expect(clonedText).not.toContain(presentId)
  })

  it(`copies a plain-text description verbatim with no copies planned`, async () => {
    const { tx, insertedIssues, insertedAttachments } = makeMockTx([])

    const result = await cloneIssueForRecurrence(tx as never, {
      ...baseParams,
      sourceDescription: { text: `Just text, no images.` },
    })

    expect(insertedAttachments).toHaveLength(0)
    expect(result.attachmentCopies).toEqual([])
    expect((insertedIssues[0].description as { text: string }).text).toBe(
      `Just text, no images.`
    )
  })

  it(`leaves the clone description empty when the source has none`, async () => {
    const { tx, insertedIssues, insertedAttachments } = makeMockTx([])

    const result = await cloneIssueForRecurrence(tx as never, {
      ...baseParams,
      sourceDescription: null,
    })

    expect(insertedAttachments).toHaveLength(0)
    expect(result.attachmentCopies).toEqual([])
    expect(insertedIssues[0].description).toBeNull()
  })
})
