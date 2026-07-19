import { createFileRoute } from "@tanstack/react-router"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { supportMessages, supportThreads } from "@/db/schema"
import { jsonResponse } from "@/lib/widget/cors"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"
import {
  MAX_SUPPORT_MESSAGE_CHARS,
  findThreadByToken,
  getSupportRateLimiters,
} from "@/lib/helpdesk/service"
import { fireAndForgetSupportThreadNotify } from "@/lib/integrations/notifications"

// Anonymous reporter reply on a helpdesk conversation (magic-link token in
// the JSON body — see thread.ts for the query-string rationale). Revoked
// (closed) threads stay readable but reject replies; per-IP and per-thread
// buckets keep a leaked token from becoming a spam pipe.
async function handleReply(request: Request): Promise<Response> {
  const contentLength = Number.parseInt(
    request.headers.get(`content-length`) ?? ``,
    10
  )
  if (Number.isFinite(contentLength) && contentLength > 64 * 1024) {
    return jsonResponse(413, { error: `Request too large` })
  }

  const { replyIpLimiter, replyThreadLimiter } = getSupportRateLimiters()
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
  const text = typeof body === `string` ? body.trim() : ``
  if (text.length === 0) {
    return jsonResponse(400, { error: `Message is empty` })
  }
  if (text.length > MAX_SUPPORT_MESSAGE_CHARS) {
    return jsonResponse(400, { error: `Message is too long` })
  }

  const thread = await findThreadByToken(token)
  if (!thread) {
    return jsonResponse(404, { error: `Conversation not found` })
  }
  if (thread.tokenRevokedAt !== null) {
    return jsonResponse(409, { error: `This conversation is closed` })
  }

  const threadLimit = replyThreadLimiter.tryTake(`thread:${thread.id}`)
  if (!threadLimit.ok) {
    return jsonResponse(
      429,
      { error: `Too many replies, try again later` },
      { "Retry-After": String(threadLimit.retryAfterSeconds) }
    )
  }

  const [message] = await db
    .insert(supportMessages)
    .values({
      threadId: thread.id,
      authorUserId: null,
      direction: `inbound`,
      visibility: `public`,
      body: text,
    })
    .returning({ id: supportMessages.id, createdAt: supportMessages.createdAt })
  await db
    .update(supportThreads)
    .set({ lastReporterSeenAt: new Date(), updatedAt: new Date() })
    .where(eq(supportThreads.id, thread.id))

  // Members get the support_reply push/inbox fan-out (digest email later).
  fireAndForgetSupportThreadNotify({ threadId: thread.id, kind: `reply` })

  return jsonResponse(201, {
    ok: true,
    message: { id: message.id, createdAt: message.createdAt },
  })
}

export const Route = createFileRoute(`/api/support/reply`)({
  server: {
    handlers: {
      POST: ({ request }) => handleReply(request),
    },
  },
})
