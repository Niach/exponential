import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-878: the issue-DRAFT upload route. It shares its whole body with the
// issue `/files` route (`storeAttachmentUpload`) — same multipart contract,
// same caps, same storage-budget check, same rollback — so what is worth
// locking here is the difference: OWNER-ONLY auth (a draft is private to the
// person composing it, and somebody else's draft is a 404, not a 403, so the
// route is no existence oracle), the `drafts/{id}/` key prefix, and a row
// that carries `draft_id` with NO issue and NO board.

const h = vi.hoisted(() => ({
  resolveSession: vi.fn(),
  assertTeamMember: vi.fn(),
  assertWithinStorageLimit: vi.fn(),
  uploadObject: vi.fn(),
  deleteObject: vi.fn(),
  insertValues: vi.fn(),
  draftRows: [] as unknown[],
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        where: () => ({
          limit: async () => h.draftRows,
        }),
      }),
    }),
    insert: () => ({ values: h.insertValues }),
  },
  pool: {},
}))

vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSession: h.resolveSession,
  SessionResolveError: class SessionResolveError extends Error {},
}))

vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  getIssueTeamContext: vi.fn(),
}))

vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: h.assertWithinStorageLimit,
}))

vi.mock(`@/lib/storage`, () => ({
  uploadObject: h.uploadObject,
  deleteObject: h.deleteObject,
}))

import { handleDraftAttachmentUpload } from "@/lib/storage/draft-attachment-upload"
import { maxFileUploadBytes } from "@/lib/storage/issue-attachments"

const DRAFT_ID = `00000000-0000-4000-8000-000000000003`

// The handler only ever calls `request.formData()`; a real multipart Request
// cannot be built here because jsdom's `File` is not undici's, and undici's
// parser asserts on it.
function upload(formData: FormData, draftId = DRAFT_ID) {
  return handleDraftAttachmentUpload({
    params: { draftId },
    request: {
      formData: async () => formData,
    } as unknown as Request,
  })
}

function fileOfSize(name: string, type: string, size: number) {
  const file = new File([`x`], name, { type })
  Object.defineProperty(file, `size`, { value: size })
  Object.defineProperty(file, `arrayBuffer`, {
    value: async () => new ArrayBuffer(1),
  })
  return file
}

function imageForm(size = 4) {
  const formData = new FormData()
  formData.append(`file`, fileOfSize(`paste.png`, `image/png`, size))
  return formData
}

beforeEach(() => {
  h.resolveSession.mockReset()
  h.assertTeamMember.mockReset()
  h.assertWithinStorageLimit.mockReset()
  h.uploadObject.mockReset()
  h.deleteObject.mockReset()
  h.insertValues.mockReset()

  h.resolveSession.mockResolvedValue({ user: { id: `user-1` } })
  h.draftRows = [{ id: DRAFT_ID, userId: `user-1`, teamId: `t-1` }]
})

describe(`handleDraftAttachmentUpload`, () => {
  it(`401s anonymous callers`, async () => {
    h.resolveSession.mockResolvedValue(null)

    await expect(upload(imageForm())).rejects.toMatchObject({
      code: `UNAUTHORIZED`,
    })
  })

  it(`404s a non-uuid draft id before querying`, async () => {
    await expect(upload(imageForm(), `not-a-uuid`)).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`404s an unknown draft`, async () => {
    h.draftRows = []

    await expect(upload(imageForm())).rejects.toMatchObject({
      code: `NOT_FOUND`,
      message: `Draft not found`,
    })
  })

  // A teammate's draft is invisible, not forbidden: answering 403 would
  // confirm the id exists.
  it(`404s somebody else's draft, without a membership check`, async () => {
    h.draftRows = [{ id: DRAFT_ID, userId: `user-2`, teamId: `t-1` }]

    await expect(upload(imageForm())).rejects.toMatchObject({
      code: `NOT_FOUND`,
      message: `Draft not found`,
    })
    expect(h.assertTeamMember).not.toHaveBeenCalled()
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  // Ownership alone is not enough: a draft outlives its author leaving the
  // team, and its bytes count against THAT team's storage budget.
  it(`still requires membership of the draft's team`, async () => {
    h.assertTeamMember.mockRejectedValue(new Error(`not a member`))

    await expect(upload(imageForm())).rejects.toThrow(`not a member`)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, `t-1`)
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`rejects a body with no part named "file"`, async () => {
    const formData = new FormData()
    formData.append(`image`, new File([`x`], `p.png`, { type: `image/png` }))

    await expect(upload(formData)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Missing file`,
    })
  })

  it(`applies the shared per-type caps`, async () => {
    const formData = new FormData()
    formData.append(
      `file`,
      fileOfSize(`huge.zip`, `application/zip`, maxFileUploadBytes + 1)
    )

    await expect(upload(formData)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Files must be 50 MB or smaller`,
    })
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`stores under the draft prefix and inserts an issue-less, board-less row`, async () => {
    const response = await upload(imageForm())
    const body = (await response.json()) as { id: string; url: string }

    expect(h.assertWithinStorageLimit).toHaveBeenCalledWith(`t-1`, 4)
    expect(h.uploadObject).toHaveBeenCalledWith(
      expect.objectContaining({
        contentType: `image/png`,
        key: expect.stringContaining(`drafts/${DRAFT_ID}/`),
      })
    )
    expect(h.insertValues).toHaveBeenCalledWith(
      expect.objectContaining({
        teamId: `t-1`,
        draftId: DRAFT_ID,
        issueId: null,
        boardId: null,
        uploaderId: `user-1`,
        contentType: `image/png`,
      })
    )
    // The response contract is byte-identical to the issue route's — the
    // client inserts `url` straight into the description.
    expect(body.url).toBe(`/api/attachments/${body.id}`)
  })

  // Non-images are accepted too: the create dialog's Files rail is uploaded
  // eagerly as well, and `issues.create({ draftId })` reparents every row.
  it(`accepts non-image files`, async () => {
    const formData = new FormData()
    formData.append(`file`, fileOfSize(`spec.pdf`, `application/pdf`, 9))

    await upload(formData)

    expect(h.insertValues).toHaveBeenCalledWith(
      expect.objectContaining({
        contentType: `application/pdf`,
        draftId: DRAFT_ID,
      })
    )
  })

  it(`rolls the blob back when the row insert fails`, async () => {
    h.insertValues.mockRejectedValue(new Error(`boom`))

    await expect(upload(imageForm())).rejects.toThrow(`boom`)
    expect(h.deleteObject).toHaveBeenCalledWith(
      expect.stringContaining(`drafts/${DRAFT_ID}/`)
    )
  })
})
