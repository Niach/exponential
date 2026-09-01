import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// REV-49: the attachment read route must answer anonymous probes with a
// uniform 401 BEFORE any DB lookup (no existence oracle) and reject non-uuid
// ids as 404 before they reach Postgres (a 22P02 cast error is not a
// TRPCError and used to surface as a logged 500 on every crawler hit).

const h = vi.hoisted(() => ({
  resolveSession: vi.fn(),
  getAttachmentTeamContext: vi.fn(),
  getSessionAttachmentTeamContext: vi.fn(),
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
  getSessionAttachmentTeamContext: h.getSessionAttachmentTeamContext,
  assertTeamMember: h.assertTeamMember,
}))

vi.mock(`@/lib/storage`, () => ({
  getObject: h.getObject,
  toResponseBody: h.toResponseBody,
}))

// EXP-704: token minting/verification is exercised for real (no mock) — the
// route accepting a signed URL as a full credential is the security boundary.
vi.stubEnv(`BETTER_AUTH_SECRET`, `attachment-route-test-secret`)

import { Route } from "@/routes/api/attachments/$attachmentId"
import { mintAttachmentToken } from "@/lib/storage/attachment-token"

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
  h.getSessionAttachmentTeamContext.mockReset()
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

  it(`falls back to session attachments when no issue attachment matches (EXP-702)`, async () => {
    h.resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    h.getAttachmentTeamContext.mockRejectedValue(
      new TRPCError({ code: `NOT_FOUND`, message: `Attachment not found` })
    )
    h.getSessionAttachmentTeamContext.mockResolvedValue({
      teamId: `w-1`,
      contentType: `image/png`,
      filename: `shot.png`,
      sizeBytes: 4,
      storageKey: `session-attachments/x`,
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
    expect(h.getSessionAttachmentTeamContext).toHaveBeenCalledWith(
      ATTACHMENT_ID
    )
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, `w-1`)
  })

  it(`404s when neither table knows the id`, async () => {
    h.resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    h.getAttachmentTeamContext.mockRejectedValue(
      new TRPCError({ code: `NOT_FOUND`, message: `Attachment not found` })
    )
    h.getSessionAttachmentTeamContext.mockRejectedValue(
      new TRPCError({ code: `NOT_FOUND`, message: `Attachment not found` })
    )

    const response = await handler({
      params: { attachmentId: ATTACHMENT_ID },
      request: new Request(
        `https://example.com/api/attachments/${ATTACHMENT_ID}`
      ),
    })

    expect(response.status).toBe(404)
  })

  // EXP-704: a signed token bound to the attachment id is a complete
  // credential — no session, no membership re-check (the MCP layer ran
  // grant + membership at mint time; the short TTL bounds the window).
  it(`serves a valid signed token without a session or membership check`, async () => {
    h.resolveSession.mockResolvedValue(null)
    h.getAttachmentTeamContext.mockResolvedValue({
      teamId: `w-1`,
      contentType: `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet`,
      filename: `report.xlsx`,
      sizeBytes: 4,
      storageKey: `attachments/x`,
    })
    h.getObject.mockResolvedValue({ Body: `body` })
    h.toResponseBody.mockResolvedValue(`ok!!`)

    const { token } = mintAttachmentToken(ATTACHMENT_ID, `user-1`)
    const response = await handler({
      params: { attachmentId: ATTACHMENT_ID },
      request: new Request(
        `https://example.com/api/attachments/${ATTACHMENT_ID}?token=${token}`
      ),
    })

    expect(response.status).toBe(200)
    expect(h.assertTeamMember).not.toHaveBeenCalled()
  })

  it(`401s a token minted for a DIFFERENT attachment`, async () => {
    h.resolveSession.mockResolvedValue(null)
    const { token } = mintAttachmentToken(
      `00000000-0000-4000-8000-00000000beef`,
      `user-1`
    )

    const response = await handler({
      params: { attachmentId: ATTACHMENT_ID },
      request: new Request(
        `https://example.com/api/attachments/${ATTACHMENT_ID}?token=${token}`
      ),
    })

    expect(response.status).toBe(401)
    expect(h.getAttachmentTeamContext).not.toHaveBeenCalled()
  })

  it(`401s expired and garbage tokens like any anonymous request`, async () => {
    h.resolveSession.mockResolvedValue(null)
    const { token: expired } = mintAttachmentToken(
      ATTACHMENT_ID,
      `user-1`,
      Date.now() - 60 * 60 * 1000
    )

    for (const token of [expired, `garbage`, `a.b`]) {
      const response = await handler({
        params: { attachmentId: ATTACHMENT_ID },
        request: new Request(
          `https://example.com/api/attachments/${ATTACHMENT_ID}?token=${token}`
        ),
      })
      expect(response.status).toBe(401)
    }
    expect(h.getAttachmentTeamContext).not.toHaveBeenCalled()
  })

  it(`still runs the membership check for a session WITHOUT a token`, async () => {
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
        `https://example.com/api/attachments/${ATTACHMENT_ID}?token=nonsense`
      ),
    })

    expect(response.status).toBe(200)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, `w-1`)
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
