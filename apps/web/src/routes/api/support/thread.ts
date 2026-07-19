import { createFileRoute } from "@tanstack/react-router"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { supportMessages, supportThreads, teams } from "@/db/schema"
import { jsonResponse } from "@/lib/widget/cors"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"
import {
  findThreadByToken,
  getSupportRateLimiters,
} from "@/lib/helpdesk/service"

// Anonymous read of a helpdesk conversation via the emailed magic-link token
// (the /support/$token page's data source). POST with the token in the BODY —
// keeping it out of query strings keeps it out of proxy access logs. Strictly
// same-origin (the page is ours), so no CORS. Only public-visibility messages
// leave the server; member identities are reduced to "Support" (reporters
// aren't members, so real names would leak the roster).
async function handleThreadRead(request: Request): Promise<Response> {
  const { readLimiter } = getSupportRateLimiters()
  const limit = readLimiter.tryTake(`ip:${clientIpFromRequest(request)}`)
  if (!limit.ok) {
    return jsonResponse(
      429,
      { error: `Too many requests, try again later` },
      { "Retry-After": String(limit.retryAfterSeconds) }
    )
  }

  let token: unknown
  try {
    const body = (await request.json()) as { token?: unknown }
    token = body.token
  } catch {
    return jsonResponse(400, { error: `Expected a JSON body` })
  }
  if (typeof token !== `string`) {
    return jsonResponse(400, { error: `Missing token` })
  }

  const thread = await findThreadByToken(token)
  if (!thread) {
    // One indistinguishable answer for unknown and malformed tokens.
    return jsonResponse(404, { error: `Conversation not found` })
  }

  const [context] = await db
    .select({ teamName: teams.name })
    .from(teams)
    .where(eq(teams.id, thread.teamId))
    .limit(1)

  const messages = await db
    .select({
      id: supportMessages.id,
      direction: supportMessages.direction,
      visibility: supportMessages.visibility,
      body: supportMessages.body,
      createdAt: supportMessages.createdAt,
    })
    .from(supportMessages)
    .where(eq(supportMessages.threadId, thread.id))
    .orderBy(supportMessages.createdAt)

  // Reading stamps the reporter's read receipt (best-effort).
  void (async () => {
    try {
      await db
        .update(supportThreads)
        .set({ lastReporterSeenAt: new Date() })
        .where(eq(supportThreads.id, thread.id))
    } catch {
      // read receipt only — never fail the read
    }
  })()

  return jsonResponse(200, {
    subject: thread.title,
    boardName: null,
    teamName: context?.teamName ?? null,
    closed: thread.status === `resolved`,
    reporterName: thread.reporterName,
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

export const Route = createFileRoute(`/api/support/thread`)({
  server: {
    handlers: {
      POST: ({ request }) => handleThreadRead(request),
    },
  },
})
