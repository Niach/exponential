// NEW-19 (FEED-144): the REPORTER's view of an issue over MCP. A signed-in
// user who filed a report on a team they are not a member of (the vendor
// feedback board `report_bug` writes to, or any widget whose reporter email
// is their verified account email) reaches the same conversation the
// /support/$token page shows: the report itself and the comments with
// audience `reporter`. The registry decides WHO is a reporter
// (lib/reporter/service.ts `findIssueForReporter`, only after the member gate
// failed); this file owns WHAT they see. Never team comments, labels,
// assignee, relations or any member's data (team replies read "Support").
import { and, asc, desc, eq } from "drizzle-orm"
import fixture from "@exp/domain-contract/fixtures/reporter-reply.json"
import { db } from "@/db/connection"
import { comments } from "@/db/schema"
import { stripImageEmbeds } from "@/lib/reporter/report-text"
import type { ResolvedReporterIssue } from "@/lib/reporter/service"

export interface ReporterComment {
  id: string
  /** "Support" on a team reply (the /support page's rule: real names would
   *  leak the roster); the reporter's own name (else the fixture's anonymous
   *  label) on theirs. */
  author: string
  fromReporter: boolean
  body: string
  createdAt: Date
}

/** reporter-reply.json `authorNames`: a reporter comment carries the
 *  submission's reporterName, else `anonymousName`. */
function reporterName(name: string | null | undefined): string {
  return name?.trim() || fixture.copy.anonymousName
}

/** The issue's reporter-audience comments, oldest first unless `newestFirst`. */
export async function listReporterComments(
  resolved: Pick<ResolvedReporterIssue, `issue` | `submission`>,
  opts: { limit: number; offset?: number; newestFirst?: boolean }
): Promise<ReporterComment[]> {
  const rows = await db
    .select({
      id: comments.id,
      source: comments.source,
      body: comments.body,
      createdAt: comments.createdAt,
    })
    .from(comments)
    .where(
      and(
        eq(comments.issueId, resolved.issue.id),
        eq(comments.audience, `reporter`)
      )
    )
    .orderBy(
      opts.newestFirst ? desc(comments.createdAt) : asc(comments.createdAt)
    )
    .limit(opts.limit)
    .offset(opts.offset ?? 0)
  const ownName = reporterName(resolved.submission.reporterName)
  return rows.map((row) => {
    const fromReporter = row.source === `reporter`
    return {
      id: row.id,
      author: fromReporter ? ownName : `Support`,
      fromReporter,
      body: row.body,
      createdAt: row.createdAt,
    }
  })
}

/** `issues get` for the reporter: the report and its reporter conversation. */
export async function reporterIssueView(
  resolved: ResolvedReporterIssue,
  commentsLimit: number
) {
  const { issue } = resolved
  return {
    id: issue.id,
    identifier: issue.identifier,
    title: issue.title,
    description: stripImageEmbeds(issue.description ?? ``),
    statusCategory: resolved.statusCategory,
    teamName: resolved.teamName,
    createdAt: issue.createdAt,
    updatedAt: issue.updatedAt,
    // Says why the view is narrow: the caller filed it, they are no member.
    access: `reporter` as const,
    recentComments: await listReporterComments(resolved, {
      limit: commentsLimit,
      newestFirst: true,
    }),
  }
}
