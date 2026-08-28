import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-639: the `/files` route is the only issue upload contract left — the
// image-only `/images` twin is gone — so its two pre-storage refusals ("no
// part named file" and the non-image 50 MB ceiling) are pinned here.

const h = vi.hoisted(() => ({
  resolveSession: vi.fn(),
  getIssueTeamContext: vi.fn(),
  assertTeamMember: vi.fn(),
  assertWithinStorageLimit: vi.fn(),
  uploadObject: vi.fn(),
  deleteObject: vi.fn(),
}))

vi.mock(`@/db/connection`, () => ({
  db: { insert: vi.fn() },
  pool: {},
}))

vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSession: h.resolveSession,
  SessionResolveError: class SessionResolveError extends Error {},
}))

vi.mock(`@/lib/team-membership`, () => ({
  getIssueTeamContext: h.getIssueTeamContext,
  assertTeamMember: h.assertTeamMember,
}))

vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: h.assertWithinStorageLimit,
}))

vi.mock(`@/lib/storage`, () => ({
  uploadObject: h.uploadObject,
  deleteObject: h.deleteObject,
}))

import { handleIssueAttachmentUpload } from "@/lib/storage/issue-attachment-upload"
import {
  maxFileUploadBytes,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"

const ISSUE_ID = `00000000-0000-4000-8000-000000000001`

// The handler only ever calls `request.formData()`; a real multipart Request
// cannot be built here because jsdom's `File` is not undici's, and undici's
// parser asserts on it.
function upload(formData: FormData) {
  return handleIssueAttachmentUpload({
    params: { issueId: ISSUE_ID },
    request: {
      formData: async () => formData,
    } as unknown as Request,
  })
}

function fileOfSize(name: string, type: string, size: number) {
  const file = new File([`x`], name, { type })
  Object.defineProperty(file, `size`, { value: size })
  return file
}

beforeEach(() => {
  h.resolveSession.mockReset()
  h.getIssueTeamContext.mockReset()
  h.assertTeamMember.mockReset()
  h.assertWithinStorageLimit.mockReset()
  h.uploadObject.mockReset()
  h.deleteObject.mockReset()

  h.resolveSession.mockResolvedValue({ user: { id: `user-1` } })
  h.getIssueTeamContext.mockResolvedValue({ teamId: `t-1`, boardId: `b-1` })
})

describe(`handleIssueAttachmentUpload`, () => {
  it(`rejects a body with no part named "file"`, async () => {
    const formData = new FormData()
    formData.append(`image`, new File([`x`], `shot.png`, { type: `image/png` }))

    await expect(upload(formData)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Missing file`,
    })
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`applies the 50 MB cap to non-image uploads and never stores them`, async () => {
    const formData = new FormData()
    formData.append(
      `file`,
      fileOfSize(`dump.zip`, `application/zip`, maxFileUploadBytes + 1)
    )

    await expect(upload(formData)).rejects.toBeInstanceOf(TRPCError)
    await expect(upload(formData)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Files must be ${maxFileUploadBytes / (1024 * 1024)} MB or smaller`,
    })
    expect(h.assertWithinStorageLimit).not.toHaveBeenCalled()
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`still holds inline images to the 10 MB image cap`, async () => {
    const formData = new FormData()
    formData.append(
      `file`,
      fileOfSize(`shot.png`, `image/png`, maxImageUploadBytes + 1)
    )

    await expect(upload(formData)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller`,
    })
    expect(h.uploadObject).not.toHaveBeenCalled()
  })
})
