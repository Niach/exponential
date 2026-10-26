import { createFileRoute } from "@tanstack/react-router"
import { jsonResponse } from "@/lib/widget/cors"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"
import {
  findIssueByReporterToken,
  getReporterRateLimiters,
} from "@/lib/reporter/service"
import {
  postReporterReply,
  validateReporterReplyText,
} from "@/lib/reporter/reply"

// SLOP-4: anonymous reporter reply on a widget report (magic-link token in
// the JSON body — see thread.ts for the query-string rationale). The reply
// is a COMMENT on the issue: author_id NULL, source `reporter`, audience
// `reporter`, body escaped ONCE into literal GFM (lib/reporter-text.ts) so
// every member client renders it as the words typed and no mention/ref
// resolver ever runs on it. A reply on a completed issue reopens it. Per-IP
// and per-issue buckets keep a leaked token from becoming a spam pipe.
async function handleReply(request: Request): Promise<Response> {
  const contentLength = Number.parseInt(
    request.headers.get(`content-length`) ?? ``,
    10
  )
  if (Number.isFinite(contentLength) && contentLength > 64 * 1024) {
    return jsonResponse(413, { error: `Request too large` })
  }

  const { replyIpLimiter } = getReporterRateLimiters()
  const ipLimit = replyIpLimiter.tryTake(
    `ip:${clientIpFromRequest(request)}`
  )
  if (!ipLimit.ok) {
    return jsonResponse(
      429,
      { error: `Too many replies, try again later` },
      { "Retry-After": String(ipLimit.retryAfterSeconds) }
    )
  }

  let token: unknown
  let body: unknown
  try {
    const parsed = (await request.json()) as {
      token?: unknown
      body?: unknown
    }
    token = parsed.token
    body = parsed.body
  } catch {
    return jsonResponse(400, { error: `Expected a JSON body` })
  }
  if (typeof token !== `string`) {
    return jsonResponse(400, { error: `Missing token` })
  }
  const validated = validateReporterReplyText(body)
  if (!validated.ok) {
    return jsonResponse(400, { error: validated.error })
  }

  const resolved = await findIssueByReporterToken(token)
  if (!resolved) {
    return jsonResponse(404, { error: `Conversation not found` })
  }

  // NEW-19: the write itself (per-issue bucket, comment, reopen, read
  // receipt, notify) is shared with the signed-in reporter's MCP reply.
  const result = await postReporterReply(resolved, validated.text)
  if (!result.ok) {
    return jsonResponse(
      429,
      { error: `Too many replies, try again later` },
      { "Retry-After": String(result.retryAfterSeconds) }
    )
  }

  return jsonResponse(201, {
    ok: true,
    message: { id: result.message.id, createdAt: result.message.createdAt },
  })
}

export const Route = createFileRoute(`/api/support/reply`)({
  server: {
    handlers: {
      POST: ({ request }) => handleReply(request),
    },
  },
})
