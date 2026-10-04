import { formatDistanceToNowStrict } from "date-fns"
import type { Comment, User } from "@/db/schema"
import fixture from "@exp/domain-contract/fixtures/reporter-reply.json"
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

/** The muted caption after the time (fixture `captions`): "reporter" (the
 *  widget reporter wrote it), then "to reporter" (a reply that was emailed
 *  out), then "via MCP" (an agent posted it). Every part that applies shows,
 *  joined by the fixture separator; none = null. */
export function commentCaption(
  comment: Pick<Comment, `source` | `audience`>
): string | null {
  const parts: Array<string> = []
  if (comment.source === `reporter`) {
    parts.push(REPORTER_REPLY_COPY.reporterCaption)
  } else if (comment.audience === `reporter`) {
    parts.push(REPORTER_REPLY_COPY.toReporterCaption)
  }
  if (comment.source === `mcp`) parts.push(fixture.captions.viaMcp)
  return parts.length ? parts.join(fixture.captions.separator) : null
}
