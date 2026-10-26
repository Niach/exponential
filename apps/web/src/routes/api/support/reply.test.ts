import { beforeEach, describe, expect, it, vi } from "vitest"

// NEW-19: the magic-link reply route keeps its contract while the write
// moved to lib/reporter/reply.ts (shared with the MCP reporter reply).

const h = vi.hoisted(() => ({
  findIssueByReporterToken: vi.fn(),
  postReporterReply: vi.fn(),
}))

vi.mock(`@/lib/reporter/service`, () => ({
  MAX_REPORTER_MESSAGE_CHARS: 10_000,
  findIssueByReporterToken: h.findIssueByReporterToken,
  getReporterRateLimiters: () => ({
    replyIpLimiter: { tryTake: () => ({ ok: true }) },
  }),
}))
vi.mock(`@/lib/reporter/reply`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  postReporterReply: h.postReporterReply,
}))

import { Route } from "@/routes/api/support/reply"

type Handler = (args: { request: Request }) => Promise<Response>
const handler = (
  Route as unknown as { options: { server: { handlers: { POST: Handler } } } }
).options.server.handlers.POST

const post = (body: unknown) =>
  handler({
    request: new Request(`https://x.test/api/support/reply`, {
      method: `POST`,
      headers: { "content-type": `application/json` },
      body: JSON.stringify(body),
    }),
  })

beforeEach(() => {
  h.findIssueByReporterToken.mockReset()
  h.postReporterReply.mockReset()
})

describe(`/api/support/reply`, () => {
  it(`refuses an empty message before resolving the token`, async () => {
    const res = await post({ token: `t`, body: `   ` })
    expect(res.status).toBe(400)
    expect(await res.json()).toEqual({ error: `Message is empty` })
    expect(h.findIssueByReporterToken).not.toHaveBeenCalled()
  })

  it(`answers 404 for an unknown token`, async () => {
    h.findIssueByReporterToken.mockResolvedValue(null)
    const res = await post({ token: `t`, body: `hi` })
    expect(res.status).toBe(404)
    expect(h.postReporterReply).not.toHaveBeenCalled()
  })

  it(`writes through the shared reporter reply`, async () => {
    const resolved = { issue: { id: `i1` }, submission: { id: `s1` } }
    h.findIssueByReporterToken.mockResolvedValue(resolved)
    h.postReporterReply.mockResolvedValue({
      ok: true,
      message: { id: `c1`, createdAt: `2026-10-26T12:00:00.000Z` },
    })
    const res = await post({ token: `t`, body: `  hello  ` })
    expect(res.status).toBe(201)
    expect(h.postReporterReply).toHaveBeenCalledWith(resolved, `hello`)
    expect(await res.json()).toEqual({
      ok: true,
      message: { id: `c1`, createdAt: `2026-10-26T12:00:00.000Z` },
    })
  })

  it(`maps the per-issue bucket to 429 + Retry-After`, async () => {
    h.findIssueByReporterToken.mockResolvedValue({ issue: {}, submission: {} })
    h.postReporterReply.mockResolvedValue({ ok: false, retryAfterSeconds: 90 })
    const res = await post({ token: `t`, body: `hi` })
    expect(res.status).toBe(429)
    expect(res.headers.get(`Retry-After`)).toBe(`90`)
  })
})
