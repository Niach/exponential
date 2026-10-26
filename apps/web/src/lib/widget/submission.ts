import { inArray } from "drizzle-orm"
import { db as defaultDb } from "@/db/connection"
import { widgetSubmissions } from "@/db/schema"
import type { WidgetSubmission } from "@/db/schema"

// NEW-19 (FEED-116): the ONE read of an issue's widget_submissions row — the
// members-only "Reported via widget" card (`widgets.submissionForIssue`) and
// the MCP `issues get`/`list` `widget` field both go through it. Batched
// (issue_id is unique, so at most one row per issue), so a list page costs
// ONE query. Access is the caller's job: every caller has already passed the
// issue team's member gate.
export async function loadWidgetSubmissions(
  issueIds: readonly string[],
  database: Pick<typeof defaultDb, `select`> = defaultDb
): Promise<WidgetSubmission[]> {
  if (issueIds.length === 0) return []
  return await database
    .select()
    .from(widgetSubmissions)
    .where(inArray(widgetSubmissions.issueId, [...new Set(issueIds)]))
}

type SubmissionFields = Pick<
  WidgetSubmission,
  | `reporterName`
  | `reporterEmail`
  | `reporterExternalId`
  | `pageUrl`
  | `userAgent`
  | `viewportWidth`
  | `viewportHeight`
  | `screenWidth`
  | `screenHeight`
  | `devicePixelRatio`
  | `customData`
  | `lastReporterSeenAt`
  | `createdAt`
>

/** What the "Reported via widget" card shows, as MCP `issues get` returns it. */
export function widgetSubmissionWire(row: SubmissionFields) {
  return {
    reporter: {
      name: row.reporterName,
      email: row.reporterEmail,
      externalId: row.reporterExternalId,
    },
    customData: row.customData ?? null,
    pageUrl: row.pageUrl,
    viewport:
      row.viewportWidth && row.viewportHeight
        ? {
            width: row.viewportWidth,
            height: row.viewportHeight,
            devicePixelRatio: row.devicePixelRatio,
          }
        : null,
    screen:
      row.screenWidth && row.screenHeight
        ? { width: row.screenWidth, height: row.screenHeight }
        : null,
    userAgent: row.userAgent,
    lastReporterSeenAt: row.lastReporterSeenAt,
    submittedAt: row.createdAt,
  }
}

/** The list row's short form: who reported it and their custom data. */
export function widgetSubmissionSummary(
  row: Pick<WidgetSubmission, `reporterName` | `reporterEmail` | `customData`>
) {
  return {
    reporter: { name: row.reporterName, email: row.reporterEmail },
    customData: row.customData ?? null,
  }
}
