import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-297: attachments are deleted MANUALLY now (the automatic GC in
// issues.update is gone). Deleting a markdown-referenced image must rewrite
// every referencing description/comment body in the SAME transaction —
// leaving a dead `![](…)` behind would make the next issues.update 400 on the
// round-trip guard.

const h = vi.hoisted(() => ({
  assertTeamMember: vi.fn(async () => undefined),
  assertTeamOwner: vi.fn(async () => undefined),
  deleteStorageObjects: vi.fn(async () => undefined),
}))

// lib/trpc.ts imports db/auth at module scope; runtime here only needs the
// exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  assertTeamOwner: h.assertTeamOwner,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: h.deleteStorageObjects,
}))

import { attachments, comments, issues } from "@/db/schema"
import { attachmentsRouter, SWEEP_GRACE_MS } from "@/lib/trpc/attachments"

const ATT_A = `11111111-1111-4111-8111-111111111111`
const ATT_B = `22222222-2222-4222-8222-222222222222`
const TEAM = `33333333-3333-4333-8333-333333333333`
const ORIGIN = `http://localhost:5173/api/trpc/attachments.delete`

interface AttachmentRow {
  id: string
  teamId?: string
  issueId?: string
  boardId?: string
  uploaderId?: string | null
  filename: string
  contentType: string
  sizeBytes: number
  width?: number | null
  height?: number | null
  storageKey: string
  createdAt: Date
}

const state = {
  attachmentRows: [] as AttachmentRow[],
  issueRows: [] as { id: string; description: string | null }[],
  commentRows: [] as { id: string; body: string }[],
  issueUpdates: [] as Record<string, unknown>[],
  commentUpdates: [] as Record<string, unknown>[],
  deletedTables: [] as string[],
}

function project(
  fields: Record<string, unknown>,
  row: Record<string, unknown>
) {
  const out: Record<string, unknown> = {}
  for (const key of Object.keys(fields)) {
    // The referenced-ids pass selects the body/description under `text`.
    out[key] =
      key === `text` ? (row.description ?? row.body ?? ``) : row[key as string]
  }
  return out
}

function rowsFor(table: unknown) {
  if (table === attachments) return state.attachmentRows
  if (table === issues) return state.issueRows
  if (table === comments) return state.commentRows
  throw new Error(`unexpected table in fake query`)
}

function thenable(rows: unknown[]) {
  return {
    then: (resolve: (value: unknown[]) => unknown, reject?: unknown) =>
      Promise.resolve(rows).then(resolve, reject as never),
    limit: async (count: number) => rows.slice(0, count),
    orderBy: async () => rows,
  }
}

function makeQueryable() {
  return {
    execute: async () => ({ rows: [{ txid: `42` }] }),
    select: (fields: Record<string, unknown>) => ({
      from: (table: unknown) => ({
        where: () =>
          thenable(
            rowsFor(table).map((row) =>
              project(fields, row as unknown as Record<string, unknown>)
            )
          ),
      }),
    }),
    update: (table: unknown) => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          if (table === issues) state.issueUpdates.push(values)
          else state.commentUpdates.push(values)
        },
      }),
    }),
    delete: (table: unknown) => ({
      where: async () => {
        state.deletedTables.push(
          table === attachments ? `attachments` : `other`
        )
      },
    }),
  }
}

const fakeDb = {
  ...makeQueryable(),
  transaction: async (fn: (tx: unknown) => Promise<unknown>) =>
    fn(makeQueryable()),
}

const caller = attachmentsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(ORIGIN),
} as never)

function resetState() {
  state.attachmentRows = []
  state.issueRows = []
  state.commentRows = []
  state.issueUpdates = []
  state.commentUpdates = []
  state.deletedTables = []
  h.assertTeamMember.mockReset()
  h.assertTeamMember.mockResolvedValue(undefined)
  h.assertTeamOwner.mockReset()
  h.assertTeamOwner.mockResolvedValue(undefined)
  h.deleteStorageObjects.mockClear()
}

function imageRow(overrides: Partial<AttachmentRow> = {}): AttachmentRow {
  return {
    id: ATT_A,
    teamId: `ws-1`,
    issueId: `issue-1`,
    boardId: `board-1`,
    uploaderId: `actor`,
    filename: `shot.png`,
    contentType: `image/png`,
    sizeBytes: 100,
    width: 10,
    height: 20,
    storageKey: `issues/issue-1/${ATT_A}-shot.png`,
    createdAt: new Date(Date.now() - 2 * SWEEP_GRACE_MS),
    ...overrides,
  }
}

describe(`attachments.delete`, () => {
  beforeEach(resetState)

  it(`rewrites every reference to a placeholder, deletes the row, then the blob`, async () => {
    state.attachmentRows = [imageRow()]
    state.issueRows = [
      {
        id: `issue-1`,
        description: `intro ![alt](/api/attachments/${ATT_A}?w=480) and ![keep](/api/attachments/${ATT_B})`,
      },
    ]
    state.commentRows = [
      { id: `comment-1`, body: `see ![](/api/attachments/${ATT_A})` },
    ]

    const result = await caller.delete({ id: ATT_A })

    expect(result).toEqual({ txId: 42 })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, `ws-1`)
    expect(state.issueUpdates).toEqual([
      {
        description: `intro *(deleted image: alt)* and ![keep](/api/attachments/${ATT_B})`,
      },
    ])
    // No alt text → the placeholder falls back to the stored filename.
    expect(state.commentUpdates).toEqual([
      { body: `see *(deleted image: shot.png)*` },
    ])
    expect(state.deletedTables).toEqual([`attachments`])
    expect(h.deleteStorageObjects).toHaveBeenCalledWith([
      `issues/issue-1/${ATT_A}-shot.png`,
    ])
  })

  it(`lowercases an uppercase input id so the rewrite still matches`, async () => {
    state.attachmentRows = [imageRow()]
    state.issueRows = [
      { id: `issue-1`, description: `![alt](/api/attachments/${ATT_A})` },
    ]

    // MCP agents may uppercase a uuid; the stored markdown is lowercase.
    await caller.delete({ id: ATT_A.toUpperCase() })

    expect(state.issueUpdates).toEqual([
      { description: `*(deleted image: alt)*` },
    ])
    expect(state.deletedTables).toEqual([`attachments`])
  })

  it(`leaves untouched bodies alone`, async () => {
    state.attachmentRows = [imageRow()]
    // Matched by the LIKE prefilter (bare id in the text) but not by the
    // exact markdown parser — nothing to rewrite.
    state.issueRows = [{ id: `issue-1`, description: `mentions ${ATT_A}` }]
    state.commentRows = []

    await caller.delete({ id: ATT_A })

    expect(state.issueUpdates).toEqual([])
    expect(state.deletedTables).toEqual([`attachments`])
  })

  it(`is member-gated: a non-member never reaches the transaction`, async () => {
    state.attachmentRows = [imageRow()]
    h.assertTeamMember.mockRejectedValue(new Error(`not allowed here`))

    await expect(caller.delete({ id: ATT_A })).rejects.toThrow(
      `not allowed here`
    )
    expect(state.deletedTables).toEqual([])
    expect(h.deleteStorageObjects).not.toHaveBeenCalled()
  })

  it(`throws NOT_FOUND when the row vanished before the transaction`, async () => {
    state.attachmentRows = []

    await expect(caller.delete({ id: ATT_A })).rejects.toThrow(
      `Attachment not found`
    )
    expect(h.deleteStorageObjects).not.toHaveBeenCalled()
  })
})

describe(`attachments.sweepUnreferencedImages`, () => {
  beforeEach(resetState)

  it(`deletes only unreferenced images older than the grace window`, async () => {
    const referenced = imageRow({ id: ATT_A, sizeBytes: 100 })
    const unreferencedOld = imageRow({
      id: ATT_B,
      sizeBytes: 250,
      storageKey: `issues/issue-1/${ATT_B}-old.png`,
    })
    const unreferencedRecent = imageRow({
      id: `44444444-4444-4444-8444-444444444444`,
      sizeBytes: 999,
      storageKey: `issues/issue-1/recent.png`,
      createdAt: new Date(Date.now() - 60 * 1000),
    })
    const pdf = imageRow({
      id: `55555555-5555-4555-8555-555555555555`,
      contentType: `application/pdf`,
      filename: `spec.pdf`,
      sizeBytes: 4242,
      storageKey: `issues/issue-1/spec.pdf`,
    })
    state.attachmentRows = [
      referenced,
      unreferencedOld,
      unreferencedRecent,
      pdf,
    ]
    state.issueRows = [
      { id: `issue-1`, description: `![a](/api/attachments/${ATT_A})` },
    ]
    state.commentRows = []

    const result = await caller.sweepUnreferencedImages({ teamId: TEAM })

    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM)
    expect(result).toEqual({
      txId: 42,
      deletedCount: 1,
      freedBytes: 250,
      skippedRecentCount: 1,
    })
    // The non-image row is never swept — Files-list rows are not markdown
    // references and would otherwise be reclaimed the moment they exist.
    expect(h.deleteStorageObjects).toHaveBeenCalledWith([
      `issues/issue-1/${ATT_B}-old.png`,
    ])
  })

  it(`is owner-gated`, async () => {
    h.assertTeamOwner.mockRejectedValue(new Error(`not allowed here`))
    await expect(
      caller.sweepUnreferencedImages({ teamId: TEAM })
    ).rejects.toThrow(`not allowed here`)
    expect(h.deleteStorageObjects).not.toHaveBeenCalled()
  })
})

describe(`attachments.listForTeam`, () => {
  beforeEach(resetState)

  it(`flags referenced rows from the exact markdown parse and totals bytes`, async () => {
    state.attachmentRows = [
      imageRow({ id: ATT_A, sizeBytes: 100 }),
      imageRow({
        id: ATT_B,
        sizeBytes: 200,
        contentType: `application/zip`,
        filename: `bundle.zip`,
      }),
    ]
    state.issueRows = [
      {
        id: `issue-1`,
        // ATT_B appears as bare text only — the exact parser must not count it.
        description: `![a](/api/attachments/${ATT_A}) mentions ${ATT_B}`,
      },
    ]
    state.commentRows = []

    const result = await caller.listForTeam({ teamId: TEAM })

    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM)
    expect(result.totalBytes).toBe(300)
    expect(
      result.attachments.map((row) => ({
        id: row.id,
        isImage: row.isImage,
        referenced: row.referenced,
      }))
    ).toEqual([
      { id: ATT_A, isImage: true, referenced: true },
      { id: ATT_B, isImage: false, referenced: false },
    ])
  })

  it(`is owner-gated`, async () => {
    h.assertTeamOwner.mockRejectedValue(new Error(`not allowed here`))
    await expect(caller.listForTeam({ teamId: TEAM })).rejects.toThrow(
      `not allowed here`
    )
  })
})
