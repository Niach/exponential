import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-825: an image attached to a START before any session exists — the
// Agent page composer's upload. Team-scoped (any member), images only, the
// same size cap and storage budget as the session route; the row lands with
// a NULL session_id and a team-scoped pending storage key.

const h = vi.hoisted(() => ({
  resolveSession: vi.fn(),
  assertTeamMember: vi.fn(),
  assertWithinStorageLimit: vi.fn(),
  uploadObject: vi.fn(),
  deleteObject: vi.fn(),
  insertValues: vi.fn(),
  sessionRows: [] as unknown[],
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        where: () => ({
          limit: async () => h.sessionRows,
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
}))

vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: h.assertWithinStorageLimit,
}))

vi.mock(`@/lib/storage`, () => ({
  uploadObject: h.uploadObject,
  deleteObject: h.deleteObject,
}))

import { handleTeamSessionAttachmentUpload } from "@/lib/storage/session-attachment-upload"
import { maxImageUploadBytes } from "@/lib/storage/issue-attachments"

const TEAM_ID = `00000000-0000-4000-8000-000000000007`

function upload(formData: FormData, teamId = TEAM_ID) {
  return handleTeamSessionAttachmentUpload({
    params: { teamId },
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
  formData.append(`file`, fileOfSize(`shot.png`, `image/png`, size))
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
})

describe(`handleTeamSessionAttachmentUpload`, () => {
  it(`401s anonymous callers`, async () => {
    h.resolveSession.mockResolvedValue(null)
    await expect(upload(imageForm())).rejects.toMatchObject({
      code: `UNAUTHORIZED`,
    })
  })

  it(`404s a non-uuid team id before the membership check`, async () => {
    await expect(upload(imageForm(), `not-a-uuid`)).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    expect(h.assertTeamMember).not.toHaveBeenCalled()
  })

  it(`refuses a non-member through the membership assert`, async () => {
    h.assertTeamMember.mockRejectedValue(new Error(`FORBIDDEN`))
    await expect(upload(imageForm())).rejects.toThrow(`FORBIDDEN`)
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`rejects non-image content types and oversize images`, async () => {
    const pdf = new FormData()
    pdf.append(`file`, fileOfSize(`doc.pdf`, `application/pdf`, 4))
    await expect(upload(pdf)).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      upload(imageForm(maxImageUploadBytes + 1))
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`stores a pending row: NULL session, the caller as uploader, a team-scoped key`, async () => {
    const response = await upload(imageForm())
    expect(response.status).toBe(200)
    const json = (await response.json()) as { id: string; url: string }
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, TEAM_ID)
    expect(h.assertWithinStorageLimit).toHaveBeenCalledWith(TEAM_ID, 4)
    expect(h.insertValues).toHaveBeenCalledWith(
      expect.objectContaining({
        id: json.id,
        teamId: TEAM_ID,
        sessionId: null,
        uploaderId: `user-1`,
        contentType: `image/png`,
        storageKey: `session-attachments/pending/${TEAM_ID}/${json.id}-shot.png`,
      })
    )
    expect(json.url).toBe(`/api/attachments/${json.id}`)
  })

  it(`rolls the object back when the row insert fails`, async () => {
    h.insertValues.mockRejectedValue(new Error(`db down`))
    await expect(upload(imageForm())).rejects.toThrow(`db down`)
    expect(h.deleteObject).toHaveBeenCalledTimes(1)
  })
})
