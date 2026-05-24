import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues, notifications, projects, users } from "@/db/schema"
import { sendToUser } from "@/lib/integrations/fcm"

/**
 * Sends a push notification to the assignee (if any, and not the actor)
 * after an issue create/update. Fire-and-forget: failures are logged.
 */
export function fireAndForgetAssignmentNotify(args: {
  issueId: string
  actorUserId: string
  newAssigneeId: string | null | undefined
  previousAssigneeId?: string | null
}): void {
  const { issueId, actorUserId, newAssigneeId, previousAssigneeId } = args
  if (!newAssigneeId) return
  if (newAssigneeId === actorUserId) return
  if (newAssigneeId === previousAssigneeId) return

  void (async () => {
    try {
      const [issue] = await db
        .select({
          id: issues.id,
          identifier: issues.identifier,
          title: issues.title,
          projectId: issues.projectId,
        })
        .from(issues)
        .where(eq(issues.id, issueId))
        .limit(1)
      if (!issue) return

      const [project] = await db
        .select({ name: projects.name })
        .from(projects)
        .where(eq(projects.id, issue.projectId))
        .limit(1)

      const [actor] = await db
        .select({ name: users.name, email: users.email })
        .from(users)
        .where(eq(users.id, actorUserId))
        .limit(1)

      const actorName = actor?.name || actor?.email || `Someone`
      const title = `${actorName} assigned you ${issue.identifier}`
      const body = `${project?.name ? `${project.name}: ` : ``}${issue.title}`

      await sendToUser(newAssigneeId, {
        title,
        body,
        data: {
          type: `assigned`,
          issueId: issue.id,
          identifier: issue.identifier,
        },
      })
    } catch (err) {
      console.error(`[notify] assignment push failed:`, err)
    }
  })()
}

/**
 * Notify the issue's creator and current assignee that someone commented.
 * Writes one notification row per recipient (so the in-app bell works) and
 * fires a push via the relay. The commenter never gets notified of their
 * own comment.
 */
export function fireAndForgetCommentNotify(args: {
  issueId: string
  actorUserId: string
  commentBodyText: string
}): void {
  const { issueId, actorUserId, commentBodyText } = args

  void (async () => {
    try {
      const [issue] = await db
        .select({
          id: issues.id,
          identifier: issues.identifier,
          title: issues.title,
          projectId: issues.projectId,
          creatorId: issues.creatorId,
          assigneeId: issues.assigneeId,
        })
        .from(issues)
        .where(eq(issues.id, issueId))
        .limit(1)
      if (!issue) return

      const recipientIds = new Set<string>()
      if (issue.creatorId && issue.creatorId !== actorUserId) {
        recipientIds.add(issue.creatorId)
      }
      if (issue.assigneeId && issue.assigneeId !== actorUserId) {
        recipientIds.add(issue.assigneeId)
      }
      if (recipientIds.size === 0) return

      const [actor] = await db
        .select({ name: users.name, email: users.email })
        .from(users)
        .where(eq(users.id, actorUserId))
        .limit(1)

      const actorName = actor?.name || actor?.email || `Someone`
      const title = `${actorName} commented on ${issue.identifier}`
      const previewSource = commentBodyText.trim()
      const preview =
        previewSource.length > 140
          ? `${previewSource.slice(0, 139)}…`
          : previewSource
      const body = preview || issue.title

      const recipients = [...recipientIds]
      await db.insert(notifications).values(
        recipients.map((userId) => ({
          userId,
          issueId: issue.id,
          type: `issue_comment` as const,
          title,
          body,
          pushedAt: new Date(),
        }))
      )

      await Promise.all(
        recipients.map((userId) =>
          sendToUser(userId, {
            title,
            body,
            data: {
              type: `issue_comment`,
              issueId: issue.id,
              identifier: issue.identifier,
            },
          })
        )
      )
    } catch (err) {
      console.error(`[notify] comment push failed:`, err)
    }
  })()
}
