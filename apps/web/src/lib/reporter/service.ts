import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, issues, teams, widgetSubmissions } from "@/db/schema"
import type { Issue, WidgetSubmission } from "@/db/schema"
import { verifyReporterToken } from "@/lib/reporter/token"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { boardVisible } from "@/lib/board-visibility"
import { TokenBucketLimiter, envInt } from "@/lib/widget/rate-limit"

// SLOP-4: shared bits of the reporter conversation — the anonymous
// /api/support/* routes, the widget submit's confirmation email and the
// member reply email all meet here.

// Reporter messages are plain text rendered small — a generous cap that still
// shuts the door on megabyte bodies.
export const MAX_REPORTER_MESSAGE_CHARS = 10_000

// The reporter's one way back into the conversation: the magic-link page.
// The token is a credential — never log or persist the URL.
export function reporterConversationUrl(token: string): string {
  return `${appBaseUrl()}/support/${token}`
}

// What the anonymous reporter endpoints need about a report: the issue, its
// widget_submissions row (reporter identity + read receipt) and the team's
// display name — ONE join per request.
export interface ResolvedReporterIssue {
  issue: Issue
  submission: WidgetSubmission
  teamName: string | null
}

// Resolve a magic-link token to its issue: verify the HMAC by recompute
// (rejecting garbage before any DB work), then load the issue it names. Only
// a widget-filed issue (one with a submission row) resolves — a token forged
// for a member-created issue answers 404 like any other, and so does one on
// a trashed or archived board (no read, reply or reopen). Returns null for
// anything that doesn't resolve — callers answer 404 without distinguishing
// why.
export async function findIssueByReporterToken(
  token: string
): Promise<ResolvedReporterIssue | null> {
  const issueId = verifyReporterToken(token)
  if (!issueId) return null
  const [row] = await db
    .select({
      issue: issues,
      submission: widgetSubmissions,
      teamName: teams.name,
    })
    .from(widgetSubmissions)
    .innerJoin(issues, eq(issues.id, widgetSubmissions.issueId))
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .leftJoin(teams, eq(teams.id, issues.teamId))
    .where(and(eq(widgetSubmissions.issueId, issueId), boardVisible()))
    .limit(1)
  if (!row) return null
  return { issue: row.issue, submission: row.submission, teamName: row.teamName }
}

// ---------------------------------------------------------------------------
// Anonymous-endpoint rate limiting (same in-process token buckets as the
// widget; per-replica by design — see lib/widget/rate-limit.ts).
// ---------------------------------------------------------------------------

let readLimiter: TokenBucketLimiter | null = null
let replyIpLimiter: TokenBucketLimiter | null = null
let replyIssueLimiter: TokenBucketLimiter | null = null

export function getReporterRateLimiters() {
  // Reads happen on every page load (and once per picture) — generous.
  // Replies are strict per IP AND per issue (a stolen token must not turn an
  // issue into a spam pipe).
  readLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`SUPPORT_RATE_LIMIT_READ_BURST`, 30),
    refillPerHour: envInt(`SUPPORT_RATE_LIMIT_READ_HOURLY`, 300),
  })
  replyIpLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`SUPPORT_RATE_LIMIT_REPLY_IP_BURST`, 5),
    refillPerHour: envInt(`SUPPORT_RATE_LIMIT_REPLY_IP_HOURLY`, 30),
  })
  replyIssueLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`SUPPORT_RATE_LIMIT_REPLY_THREAD_BURST`, 5),
    refillPerHour: envInt(`SUPPORT_RATE_LIMIT_REPLY_THREAD_HOURLY`, 30),
  })
  return { readLimiter, replyIpLimiter, replyIssueLimiter }
}
