import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { comments, issues, issueStatuses, widgetSubmissions } from "@/db/schema"
import { CATEGORY_ANCHOR } from "@/lib/domain"
import { generateTxId } from "@/lib/trpc"
import { escapeReporterText } from "@/lib/reporter-text"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { fireAndForgetReporterReplyNotify } from "@/lib/integrations/notifications"
import {
  MAX_REPORTER_MESSAGE_CHARS,
  getReporterRateLimiters,
  type ResolvedReporterIssue,
} from "./service"

// SLOP-4 → NEW-19: the ONE reporter-reply write. The anonymous magic-link
// route (/api/support/reply) and the signed-in reporter's MCP
// `comments create` both land here. The reply is a COMMENT on the issue:
// author_id NULL, source `reporter`, audience `reporter`, body escaped ONCE
// into literal GFM (lib/reporter-text.ts) so every member client renders it
// as the words typed and no mention/ref resolver ever runs on it. A reply on
// a completed issue reopens it. The per-issue bucket keeps a leaked token (or
// a looping agent) from turning an issue into a spam pipe.

export type ReporterReplyText =
  | { ok: true; text: string }
  | { ok: false; error: string }

/** Trim + bound the reporter's words (empty and over-long are refused). */
export function validateReporterReplyText(body: unknown): ReporterReplyText {
  const text = typeof body === `string` ? body.trim() : ``
  if (text.length === 0) return { ok: false, error: `Message is empty` }
  if (text.length > MAX_REPORTER_MESSAGE_CHARS) {
    return { ok: false, error: `Message is too long` }
  }
  return { ok: true, text }
}

export type ReporterReplyResult =
  | { ok: true; message: { id: string; createdAt: Date } }
  | { ok: false; retryAfterSeconds: number }

export async function postReporterReply(
  resolved: Pick<ResolvedReporterIssue, `issue` | `submission`>,
  text: string
): Promise<ReporterReplyResult> {
  const { issue, submission } = resolved
  const { replyIssueLimiter } = getReporterRateLimiters()
  const issueLimit = replyIssueLimiter.tryTake(`issue:${issue.id}`)
  if (!issueLimit.ok) {
    return { ok: false, retryAfterSeconds: issueLimit.retryAfterSeconds }
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
    // event with no actor — the reporter is not a member, like the issue.
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

  return { ok: true, message }
}
