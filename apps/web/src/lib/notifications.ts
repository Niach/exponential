import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues, projects, users } from "@/db/schema"
import { sendToUser } from "@/lib/fcm"

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
