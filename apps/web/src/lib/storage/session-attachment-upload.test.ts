import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-702: steer attachment upload for coding sessions (wave D: any file,
// images 10 MB, files 50 MB, like the issue `/files` route). Only the
// session's OWNER may upload — steering is
// owner-only (EXP-312), so nobody else can put the resulting embed on the
// wire anyway.

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

import {
  handleSessionAttachmentUpload,
  prepareSessionImageBytes,
} from "@/lib/storage/session-attachment-upload"
import {
  maxFileUploadBytes,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"

const SESSION_ID = `00000000-0000-4000-8000-000000000002`

// The handler only ever calls `request.formData()`; a real multipart Request
// cannot be built here because jsdom's `File` is not undici's, and undici's
// parser asserts on it.
function upload(formData: FormData, sessionId = SESSION_ID) {
  return handleSessionAttachmentUpload({
    params: { sessionId },
    request: {
      formData: async () => formData,
    } as unknown as Request,
  })
}

function fileOfSize(name: string, type: string, size: number) {
  const file = new File([`x`], name, { type })
  Object.defineProperty(file, `size`, { value: size })
  // jsdom's File has no arrayBuffer(); the happy-path tests need one.
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
  h.sessionRows = [{ id: SESSION_ID, teamId: `t-1`, userId: `user-1` }]
})

describe(`handleSessionAttachmentUpload`, () => {
  it(`401s anonymous callers`, async () => {
    h.resolveSession.mockResolvedValue(null)

    await expect(upload(imageForm())).rejects.toMatchObject({
      code: `UNAUTHORIZED`,
    })
  })

  it(`404s a non-uuid session id before querying`, async () => {
    await expect(upload(imageForm(), `not-a-uuid`)).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
  })

  it(`404s an unknown session`, async () => {
    h.sessionRows = []

    await expect(upload(imageForm())).rejects.toMatchObject({
      code: `NOT_FOUND`,
      message: `Session not found`,
    })
  })

  it(`rejects a caller who is not the session owner`, async () => {
    h.sessionRows = [{ id: SESSION_ID, teamId: `t-1`, userId: `user-2` }]

    await expect(upload(imageForm())).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, `t-1`)
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`rejects a body with no part named "file"`, async () => {
    const formData = new FormData()
    formData.append(`image`, new File([`x`], `shot.png`, { type: `image/png` }))

    await expect(upload(formData)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Missing file`,
    })
  })

  it(`stores a non-image file under the 50 MB file cap`, async () => {
    const formData = new FormData()
    formData.append(`file`, fileOfSize(`notes.pdf`, `application/pdf`, 4))

    const response = await upload(formData)
    const body = (await response.json()) as {
      filename: string
      contentType: string
      width: number | null
    }
    expect(body).toMatchObject({
      filename: `notes.pdf`,
      contentType: `application/pdf`,
      width: null,
    })
    expect(h.insertValues).toHaveBeenCalledWith(
      expect.objectContaining({ contentType: `application/pdf`, height: null })
    )
  })

  it(`accepts a 15 MB file but rejects one over 50 MB`, async () => {
    const ok = new FormData()
    ok.append(`file`, fileOfSize(`trace.zip`, `application/zip`, 15 * 1024 * 1024))
    await expect(upload(ok)).resolves.toBeInstanceOf(Response)

    const big = new FormData()
    big.append(
      `file`,
      fileOfSize(`trace.zip`, `application/zip`, maxFileUploadBytes + 1)
    )
    await expect(upload(big)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Files must be 50 MB or smaller`,
    })
  })

  it(`keeps result pictures images-only`, async () => {
    await expect(
      prepareSessionImageBytes(
        { filename: `notes.pdf`, contentType: `application/pdf`, body: new Uint8Array(4) },
        { teamId: `t-1`, sessionId: SESSION_ID },
        undefined,
        { imagesOnly: true }
      )
    ).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Only images can be attached to a session`,
    })
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`rejects images over the 10 MB image ceiling`, async () => {
    await expect(
      upload(imageForm(maxImageUploadBytes + 1))
    ).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Images must be 10 MB or smaller`,
    })
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`stores the blob, inserts the row and returns the attachment url`, async () => {
    const response = await upload(imageForm())
    const body = (await response.json()) as { id: string; url: string }

    expect(h.assertWithinStorageLimit).toHaveBeenCalledWith(`t-1`, 4)
    expect(h.uploadObject).toHaveBeenCalledWith(
      expect.objectContaining({
        contentType: `image/png`,
        key: expect.stringContaining(`session-attachments/${SESSION_ID}/`),
      })
    )
    expect(h.insertValues).toHaveBeenCalledWith(
      expect.objectContaining({
        teamId: `t-1`,
        sessionId: SESSION_ID,
        uploaderId: `user-1`,
        contentType: `image/png`,
      })
    )
    expect(body.url).toBe(`/api/attachments/${body.id}`)
  })

  it(`rolls the blob back when the row insert fails`, async () => {
    h.insertValues.mockRejectedValue(new Error(`boom`))

    await expect(upload(imageForm())).rejects.toThrow(`boom`)
    expect(h.deleteObject).toHaveBeenCalledWith(
      expect.stringContaining(`session-attachments/${SESSION_ID}/`)
    )
  })
})
