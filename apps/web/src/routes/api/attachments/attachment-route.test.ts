import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// REV-49: the attachment read route must answer anonymous probes with a
// uniform 401 BEFORE any DB lookup (no existence oracle) and reject non-uuid
// ids as 404 before they reach Postgres (a 22P02 cast error is not a
// TRPCError and used to surface as a logged 500 on every crawler hit).

const h = vi.hoisted(() => ({
  resolveSession: vi.fn(),
  getAttachmentTeamContext: vi.fn(),
  assertTeamMember: vi.fn(),
  getObject: vi.fn(),
  toResponseBody: vi.fn(),
}))

vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSession: h.resolveSession,
  SessionResolveError: class SessionResolveError extends Error {},
}))

vi.mock(`@/lib/team-membership`, () => ({
  getAttachmentTeamContext: h.getAttachmentTeamContext,
  assertTeamMember: h.assertTeamMember,
}))

vi.mock(`@/lib/storage`, () => ({
  getObject: h.getObject,
  toResponseBody: h.toResponseBody,
}))

import { Route } from "@/routes/api/attachments/$attachmentId"

type Handler = (args: {
  params: { attachmentId: string }
  request: Request
}) => Promise<Response>

const handler = (
  Route as unknown as {
    options: { server: { handlers: { GET: Handler } } }
  }
).options.server.handlers.GET

const ATTACHMENT_ID = `00000000-0000-4000-8000-000000000001`

beforeEach(() => {
  h.resolveSession.mockReset()
  h.getAttachmentTeamContext.mockReset()
  h.assertTeamMember.mockReset()
  h.getObject.mockReset()
  h.toResponseBody.mockReset()
})

describe(`GET /api/attachments/$attachmentId`, () => {
  it(`answers anonymous requests 401 without touching the database`, async () => {
    h.resolveSession.mockResolvedValue(null)

    // Existing AND garbage ids get the identical answer — no oracle.
    for (const attachmentId of [ATTACHMENT_ID, `not-a-uuid`]) {
      const response = await handler({
        params: { attachmentId },
        request: new Request(`https://example.com/api/attachments/${attachmentId}`),
      })
      expect(response.status).toBe(401)
    }
    expect(h.getAttachmentTeamContext).not.toHaveBeenCalled()
  })

  it(`404s a non-uuid id before querying`, async () => {
    h.resolveSession.mockResolvedValue({ user: { id: `user-1` } })

    const response = await handler({
      params: { attachmentId: `not-a-uuid` },
      request: new Request(`https://example.com/api/attachments/not-a-uuid`),
    })

    expect(response.status).toBe(404)
    expect(h.getAttachmentTeamContext).not.toHaveBeenCalled()
  })

  it(`serves members the attachment after the team check`, async () => {
    h.resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    h.getAttachmentTeamContext.mockResolvedValue({
      teamId: `w-1`,
      contentType: `image/png`,
      filename: `shot.png`,
      sizeBytes: 4,
      storageKey: `attachments/x`,
    })
    h.getObject.mockResolvedValue({ Body: `body` })
    h.toResponseBody.mockResolvedValue(`ok!!`)

    const response = await handler({
      params: { attachmentId: ATTACHMENT_ID },
      request: new Request(
        `https://example.com/api/attachments/${ATTACHMENT_ID}`
      ),
    })

    expect(response.status).toBe(200)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, `w-1`)
    expect(response.headers.get(`content-type`)).toBe(`image/png`)
  })

  it(`403s a cross-team member via the membership assert`, async () => {
    h.resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    h.getAttachmentTeamContext.mockResolvedValue({ teamId: `w-2` })
    h.assertTeamMember.mockRejectedValue(
      new TRPCError({ code: `FORBIDDEN`, message: `Not a member` })
    )

    const response = await handler({
      params: { attachmentId: ATTACHMENT_ID },
      request: new Request(
        `https://example.com/api/attachments/${ATTACHMENT_ID}`
      ),
    })

    expect(response.status).toBe(403)
  })
})
