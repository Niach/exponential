import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-1252: a moving session's handoff package route. The device's own
// credentials (API key / bearer) gate it: no session = 401 BEFORE any
// database work; the session rules (source host + pending move for PUT,
// current host for GET, the 64 MiB cap) live in lib/sessions/handoff.ts and
// run against the real Postgres in sessions.real-db.test.ts.

const h = vi.hoisted(() => ({
  resolveSession: vi.fn(),
  putHandoffBlob: vi.fn(),
  getHandoffBlob: vi.fn(),
}))

vi.mock(`@/lib/auth/resolve-bearer`, () => ({ resolveSession: h.resolveSession }))
vi.mock(`@/lib/sessions/handoff`, () => ({
  putHandoffBlob: h.putHandoffBlob,
  getHandoffBlob: h.getHandoffBlob,
}))

import { Route } from "@/routes/api/sessions/$sessionId/handoff"

type Handler = (args: { params: { sessionId: string }; request: Request }) => Promise<Response>
const handlers = (
  Route as unknown as { options: { server: { handlers: { PUT: Handler; GET: Handler } } } }
).options.server.handlers

const SESSION = `3f1c2b7a-5d4e-4f60-8a9b-0c1d2e3f4a5b`
const put = () =>
  handlers.PUT({
    params: { sessionId: SESSION },
    request: new Request(`http://x.test/api/sessions/${SESSION}/handoff`, {
      method: `PUT`,
      body: new Uint8Array([31, 139]),
      headers: { "content-type": `application/gzip` },
    }),
  })
const get = () =>
  handlers.GET({
    params: { sessionId: SESSION },
    request: new Request(`http://x.test/api/sessions/${SESSION}/handoff`),
  })

describe(`/api/sessions/$sessionId/handoff`, () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it(`401s without credentials, touching nothing`, async () => {
    h.resolveSession.mockResolvedValue(null)
    expect((await put()).status).toBe(401)
    expect((await get()).status).toBe(401)
    expect(h.putHandoffBlob).not.toHaveBeenCalled()
    expect(h.getHandoffBlob).not.toHaveBeenCalled()
  })

  it(`hands the caller and the session to the handoff core`, async () => {
    h.resolveSession.mockResolvedValue({ user: { id: `u1` } })
    h.putHandoffBlob.mockResolvedValue(Response.json({ sizeBytes: 2 }, { status: 201 }))
    h.getHandoffBlob.mockResolvedValue(new Response(`gz`, { status: 200 }))
    expect((await put()).status).toBe(201)
    expect(h.putHandoffBlob).toHaveBeenCalledWith(`u1`, SESSION, expect.any(Request))
    expect((await get()).status).toBe(200)
    expect(h.getHandoffBlob).toHaveBeenCalledWith(`u1`, SESSION)
  })

  it(`maps the core's refusals to HTTP`, async () => {
    h.resolveSession.mockResolvedValue({ user: { id: `u1` } })
    h.getHandoffBlob.mockRejectedValue(new TRPCError({ code: `NOT_FOUND`, message: `Session not found` }))
    h.putHandoffBlob.mockRejectedValue(
      new TRPCError({ code: `FORBIDDEN`, message: `Only the device running this session can report for it` })
    )
    expect((await get()).status).toBe(404)
    expect((await put()).status).toBe(403)
  })
})
