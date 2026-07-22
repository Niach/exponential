import { createFileRoute } from "@tanstack/react-router"
import { and, eq, gte } from "drizzle-orm"
import { db } from "@/db/connection"
import { supportMessages, supportThreads } from "@/db/schema"
import { jsonResponse } from "@/lib/widget/cors"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"
import { parsePollSince } from "@/lib/helpdesk/presence"
import {
  findThreadByToken,
  getSupportRateLimiters,
} from "@/lib/helpdesk/service"

// Incremental poll behind the live support chat (EXP-237): the
// /support/$token page calls this every ~5s while its tab is visible and gets
// back only the public messages at or after its `since` cursor (the newest
// createdAt it holds). Same posture as its sibling api/support/thread.ts —
// POST with the token in the BODY, same-origin only, one indistinguishable
// 404 for anything that doesn't resolve. Every poll also stamps
// last_reporter_seen_at: that heartbeat is what lets helpdesk.reply suppress
// the "new reply" email while the reporter is watching the page live.
async function handlePoll(request: Request): Promise<Response> {
  const { pollLimiter } = getSupportRateLimiters()
  const limit = pollLimiter.tryTake(`ip:${clientIpFromRequest(request)}`)
  if (!limit.ok) {
    return jsonResponse(
      429,
      { error: `Too many requests, try again later` },
      { "Retry-After": String(limit.retryAfterSeconds) }
    )
  }

  let token: unknown
  let sinceRaw: unknown
  try {
    const body = (await request.json()) as { token?: unknown; since?: unknown }
    token = body.token
    sinceRaw = body.since
  } catch {
    return jsonResponse(400, { error: `Expected a JSON body` })
  }
  if (typeof token !== `string`) {
    return jsonResponse(400, { error: `Missing token` })
  }

  const thread = await findThreadByToken(token)
  if (!thread) {
    return jsonResponse(404, { error: `Conversation not found` })
  }

  // gte, not gt: the client's cursor is a JSON ISO string truncated to
  // milliseconds while Postgres stores microseconds, so a strict > could skip
  // a message sharing the cursor's millisecond. The overlap row this returns
  // is deduped client-side by id.
  const since = parsePollSince(sinceRaw)
  const messages = await db
    .select({
      id: supportMessages.id,
      direction: supportMessages.direction,
      visibility: supportMessages.visibility,
      body: supportMessages.body,
      createdAt: supportMessages.createdAt,
    })
    .from(supportMessages)
    .where(
      since
        ? and(
            eq(supportMessages.threadId, thread.id),
            gte(supportMessages.createdAt, since)
          )
        : eq(supportMessages.threadId, thread.id)
    )
    .orderBy(supportMessages.createdAt)

  // The presence heartbeat (best-effort, like thread.ts's read receipt).
  void (async () => {
    try {
      await db
        .update(supportThreads)
        .set({ lastReporterSeenAt: new Date() })
        .where(eq(supportThreads.id, thread.id))
    } catch {
      // heartbeat only — never fail the poll
    }
  })()

  return jsonResponse(200, {
    closed: thread.status === `resolved`,
    messages: messages
      .filter((m) => m.visibility === `public`)
      .map((m) => ({
        id: m.id,
        direction: m.direction,
        body: m.body,
        createdAt: m.createdAt,
      })),
  })
}

export const Route = createFileRoute(`/api/support/poll`)({
  server: {
    handlers: {
      POST: ({ request }) => handlePoll(request),
    },
  },
})
