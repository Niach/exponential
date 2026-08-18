import { db } from "@/db/connection"
import { issues, issueSubscribers, widgetSubmissions } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { getSoleHumanMemberId } from "@/lib/team-membership"
import { ensureSubscribed } from "@/lib/integrations/subscriptions"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { fireAndForgetNewIssueNotify } from "@/lib/integrations/notifications"
import { loadWidgetConfigByKey, WidgetRequestError } from "./service"

// EXP-496: server-side intake for the MCP `exponential_report_bug` tool — the
// vendor-instance sibling of createWidgetSubmission (widget/service.ts), minus
// everything browser-shaped (origin/CORS, screenshots, form toggles, labels).
// It files onto the board of the instance's own feedback widget config (the
// same hardcoded key the sidebar Feedback button uses), so it only exists
// where that widget exists. Kept in its own module so mcp/tools.ts can be
// tested with ONE vi.mock instead of inheriting service.ts's email/helpdesk
// import graph.
export async function createAgentBugReport(args: {
  widgetKey: string
  reporter: { email: string; name: string | null }
  title: string
  description: string
  userAgent: string | null
}): Promise<{ issueId: string; identifier: string }> {
  const config = await loadWidgetConfigByKey(args.widgetKey)

  // Same gate as the widget submit path: a missing, trashed or archived
  // feedback board rejects new writes; restoring or unarchiving brings a
  // hidden board back automatically.
  const boardId = config.boardId
  if (
    boardId == null ||
    config.boardDeletedAt != null ||
    config.boardArchivedAt != null
  ) {
    throw new WidgetRequestError(403, `This feedback board is unavailable`)
  }

  const soleMemberId = await getSoleHumanMemberId(config.teamId)

  const result = await db.transaction(async (tx) => {
    await generateTxId(tx)
    // Widget parity: no user creator (the reporter is not a member of the
    // vendor team), `source: agent` drives the "Agent" origin chip.
    const [issue] = await tx
      .insert(issues)
      .values({
        boardId,
        // populate_issue_board_context overwrites with board-derived truth;
        // passed to satisfy the NOT NULL insert contract.
        teamId: config.teamId,
        title: args.title,
        status: `backlog`,
        priority: `none`,
        description: args.description || null,
        assigneeId: soleMemberId,
        creatorId: null,
        source: `agent`,
      })
      .returning({
        id: issues.id,
        identifier: issues.identifier,
        status: issues.status,
        statusId: issues.statusId,
        priority: issues.priority,
      })

    // EXP-530: `created` event (timeline-suppressed; feeds automation event
    // triggers). No actor — widget parity, the reporter is not a member.
    await recordIssueEvent(tx, {
      issueId: issue.id,
      teamId: config.teamId,
      actorUserId: null,
      type: `created`,
      payload: {
        status: issue.status,
        statusId: issue.statusId,
        priority: issue.priority,
        source: `agent`,
      },
    })

    // EXP-50 parity: subscribe the auto-assigned solo member without an
    // assignment notification — the post-commit fan-out already reaches them.
    if (soleMemberId) {
      await ensureSubscribed(tx, {
        issueId: issue.id,
        userId: soleMemberId,
        teamId: config.teamId,
        source: `assignee`,
      })
    }

    // The MCP caller is a real signed-in user of THIS instance but (usually)
    // not a member of the vendor team, so they ride the widget_reporter rails:
    // email-only subscriber row → they get the resolution email when the
    // issue closes; member fan-out ignores null-userId rows.
    await tx.insert(issueSubscribers).values({
      issueId: issue.id,
      userId: null,
      email: args.reporter.email,
      teamId: config.teamId,
      boardId,
      source: `widget_reporter`,
      unsubscribed: false,
    })

    await tx.insert(widgetSubmissions).values({
      widgetConfigId: config.id,
      issueId: issue.id,
      reporterEmail: args.reporter.email,
      reporterName: args.reporter.name,
      userAgent: args.userAgent,
      customData: { via: `mcp` },
    })

    return { issueId: issue.id, identifier: issue.identifier }
  })

  // After commit (the notification loads the issue row itself): fan out
  // issue_created to the vendor team's members. Never fails the report.
  fireAndForgetNewIssueNotify({ issueId: result.issueId })

  return result
}
