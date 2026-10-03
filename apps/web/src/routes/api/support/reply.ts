import { createFileRoute } from "@tanstack/react-router"
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { comments, issues, issueStatuses, widgetSubmissions } from "@/db/schema"
import { CATEGORY_ANCHOR } from "@/lib/domain"
import { generateTxId } from "@/lib/trpc"
import { jsonResponse } from "@/lib/widget/cors"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"
import {
  MAX_REPORTER_MESSAGE_CHARS,
  findIssueByReporterToken,
  getReporterRateLimiters,
} from "@/lib/reporter/service"
import { escapeReporterText } from "@/lib/reporter-text"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { fireAndForgetReporterReplyNotify } from "@/lib/integrations/notifications"

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

  const { replyIpLimiter, replyIssueLimiter } = getReporterRateLimiters()
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
  if (text.length > MAX_REPORTER_MESSAGE_CHARS) {
    return jsonResponse(400, { error: `Message is too long` })
  }

  const resolved = await findIssueByReporterToken(token)
  if (!resolved) {
    return jsonResponse(404, { error: `Conversation not found` })
  }
  const { issue, submission } = resolved

  const issueLimit = replyIssueLimiter.tryTake(`issue:${issue.id}`)
  if (!issueLimit.ok) {
    return jsonResponse(
      429,
      { error: `Too many replies, try again later` },
      { "Retry-After": String(issueLimit.retryAfterSeconds) }
    )
  }

  const message = await db.transaction(async (tx) => {
    await generateTxId(tx)
    const [comment] = await tx
      .insert(comments)
      .values({
        issueId: issue.id,
        teamId: issue.teamId,
        boardId: issue.boardId,
        authorId: null,
        parentId: null,
        source: `reporter`,
        audience: `reporter`,
        body: escapeReporterText(text),
      })
      .returning({ id: comments.id, createdAt: comments.createdAt })

    // A reply on a completed issue reopens it: status `backlog` (the
    // team's builtin row when it resolves, else NULL → re-anchored by
    // populate_issue_status_id), completedAt cleared, one status_changed
    // event with no actor — the reporter is anonymous, like the issue.
    if (issue.status === CATEGORY_ANCHOR.completed) {
      const [backlog] = await tx
        .select({ id: issueStatuses.id, name: issueStatuses.name })
        .from(issueStatuses)
        .where(
          and(
            eq(issueStatuses.teamId, issue.teamId),
            eq(issueStatuses.builtinKey, `backlog`)
          )
        )
        .limit(1)
      const [previous] = issue.statusId
        ? await tx
            .select({ name: issueStatuses.name })
            .from(issueStatuses)
            .where(eq(issueStatuses.id, issue.statusId))
            .limit(1)
        : []
      await tx
        .update(issues)
        .set({
          status: `backlog`,
          statusId: backlog?.id ?? null,
          completedAt: null,
        })
        .where(eq(issues.id, issue.id))
      await recordIssueEvent(tx, {
        issueId: issue.id,
        teamId: issue.teamId,
        actorUserId: null,
        type: `status_changed`,
        payload: {
          fromStatusId: issue.statusId ?? null,
          toStatusId: backlog?.id ?? null,
          fromName: previous?.name ?? null,
          toName: backlog?.name ?? null,
        },
      })
    }

    await tx
      .update(widgetSubmissions)
      .set({ lastReporterSeenAt: new Date() })
      .where(eq(widgetSubmissions.id, submission.id))

    return comment
  })

  // Subscribers + the assignee get the reporter_reply row/push (digest
  // email later).
  fireAndForgetReporterReplyNotify({ issueId: issue.id, commentId: message.id })

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
