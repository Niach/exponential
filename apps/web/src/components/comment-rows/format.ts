import { formatDistanceToNowStrict } from "date-fns"
import type { Comment, User } from "@/db/schema"
import { displayUserName } from "@/lib/user-display"
import {
  REPORTER_REPLY_COPY,
  reporterDisplayName,
} from "@/components/reporter-reply-copy"

export function relativeTime(date: Date | string | null | undefined): string {
  if (!date) return ``
  const value = typeof date === `string` ? new Date(date) : date
  if (Number.isNaN(value.getTime())) return ``
  return formatDistanceToNowStrict(value, { addSuffix: true })
}

/**
 * SLOP-4 (fixture `reporter-reply.json`): the name a comment card carries.
 * A `reporter` comment has no users row — it names the submission's reporter
 * (else "Anonymous visitor"); a member comment names its synced author; a
 * member whose row is gone (left the team, deleted) reads "Former member".
 */
export function authorLabel(
  comment: Pick<Comment, `authorId` | `source`>,
  author: User | undefined,
  reporterName: string | null | undefined
): string {
  if (comment.source === `reporter`) return reporterDisplayName(reporterName)
  if (!author) return REPORTER_REPLY_COPY.formerMemberName
  return displayUserName(author, comment.authorId)
}

/** The muted caption after the time — exactly one of "via MCP" (an agent
 *  posted it), "reporter" (the widget reporter wrote it) or "to reporter" (a
 *  member's reply that was emailed out), else none. */
export function commentCaption(
  comment: Pick<Comment, `source` | `audience`>
): string | null {
  if (comment.source === `reporter`) return REPORTER_REPLY_COPY.reporterCaption
  if (comment.source === `mcp`) return `via MCP`
  if (comment.audience === `reporter`) return REPORTER_REPLY_COPY.toReporterCaption
  return null
}
