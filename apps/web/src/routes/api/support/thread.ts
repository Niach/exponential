import { createFileRoute } from "@tanstack/react-router"
import { and, asc, eq, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, comments, widgetSubmissions } from "@/db/schema"
import { CATEGORY_ANCHOR } from "@/lib/domain"
import { jsonResponse } from "@/lib/widget/cors"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"
import {
  findIssueByReporterToken,
  getReporterRateLimiters,
} from "@/lib/reporter/service"
import { isAcceptedImageContentType } from "@/lib/storage/issue-attachments"
import { stripImageEmbeds } from "@/lib/reporter/report-text"

// SLOP-4: anonymous read of a widget report's conversation via the emailed
// magic-link token (the /support/$token page's data source). POST with the
// token in the BODY — keeping it out of query strings keeps it out of proxy
// access logs. Strictly same-origin (the page is ours), so no CORS. What
// leaves the server: the report (title + description text + the reporter's
// own pictures) and the comments with audience `reporter`; member identities
// are reduced to "Support" (reporters aren't members, so real names would
// leak the roster). Team-audience comments never leave.
async function handleThreadRead(request: Request): Promise<Response> {
  const { readLimiter } = getReporterRateLimiters()
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

  const resolved = await findIssueByReporterToken(token)
  if (!resolved) {
    // One indistinguishable answer for unknown and malformed tokens.
    return jsonResponse(404, { error: `Conversation not found` })
  }
  const { issue, submission } = resolved

  const rows = await db
    .select({
      id: comments.id,
      source: comments.source,
      body: comments.body,
      createdAt: comments.createdAt,
    })
    .from(comments)
    .where(
      and(eq(comments.issueId, issue.id), eq(comments.audience, `reporter`))
    )
    .orderBy(asc(comments.createdAt))

  // The reporter's own pictures: null uploader (the widget submit writes
  // them so) AND an image type — never a member's upload.
  const pictures = await db
    .select({
      id: attachments.id,
      filename: attachments.filename,
      contentType: attachments.contentType,
      width: attachments.width,
      height: attachments.height,
    })
    .from(attachments)
    .where(
      and(
        eq(attachments.issueId, issue.id),
        isNull(attachments.uploaderId),
        isNull(attachments.commentId)
      )
    )
    .orderBy(asc(attachments.createdAt))

  // Reading stamps the reporter's read receipt (best-effort).
  void (async () => {
    try {
      await db
        .update(widgetSubmissions)
        .set({ lastReporterSeenAt: new Date() })
        .where(eq(widgetSubmissions.id, submission.id))
    } catch {
      // read receipt only — never fail the read
    }
  })()

  return jsonResponse(200, {
    subject: issue.title,
    teamName: resolved.teamName,
    // The anchor enum names the category: `done` ⇔ completed. A reply
    // reopens, so the page keeps its reply box either way.
    status: issue.status === CATEGORY_ANCHOR.completed ? `resolved` : `open`,
    reporterName: submission.reporterName,
    report: {
      title: issue.title,
      description: stripImageEmbeds(issue.description ?? ``),
      createdAt: issue.createdAt,
    },
    attachments: pictures.filter((row) =>
      isAcceptedImageContentType(row.contentType)
    ),
    messages: rows.map((row) => ({
      id: row.id,
      direction: row.source === `reporter` ? `inbound` : `outbound`,
      body: row.body,
      createdAt: row.createdAt,
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
