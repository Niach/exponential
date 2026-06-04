import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  issueSubscribers,
  issues,
  notifications,
  projects,
  users,
} from "@/db/schema"
import { sendToUser } from "@/lib/integrations/fcm"
import { canUsePush } from "@/lib/billing"
import type { NotificationType } from "@/lib/domain"

// The canonical push `data.type` discriminator vocabulary (D8/D13). The web
// emits these; the native clients route on them. Notification rows use the
// notification_type enum (the first four); plan/PR types ride only on the push
// payload (their inbox surface derives from issue columns).
export type PushType =
  | NotificationType
  | `plan_awaiting_approval`
  | `pr_opened`
  | `pr_merged`
  | `agent_error`

interface IssueMeta {
  id: string
  identifier: string
  title: string
  workspaceId: string
}

async function loadIssueMeta(issueId: string): Promise<IssueMeta | null> {
  const [row] = await db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      title: issues.title,
      workspaceId: projects.workspaceId,
    })
    .from(issues)
    .innerJoin(projects, eq(projects.id, issues.projectId))
    .where(eq(issues.id, issueId))
    .limit(1)
  return row ?? null
}

async function actorName(actorUserId: string): Promise<string> {
  const [actor] = await db
    .select({ name: users.name, email: users.email })
    .from(users)
    .where(eq(users.id, actorUserId))
    .limit(1)
  return actor?.name || actor?.email || `Someone`
}

// Drop agent users from a recipient set — agents have no inbox.
async function withoutAgents(userIds: string[]): Promise<string[]> {
  if (userIds.length === 0) return []
  const rows = await db
    .select({ id: users.id })
    .from(users)
    .where(and(inArray(users.id, userIds), eq(users.isAgent, true)))
  const agents = new Set(rows.map((r) => r.id))
  return userIds.filter((id) => !agents.has(id))
}

// The active (non-unsubscribed) subscribers of an issue, minus the actor.
async function subscriberRecipients(
  issueId: string,
  actorUserId: string
): Promise<string[]> {
  const rows = await db
    .select({ userId: issueSubscribers.userId })
    .from(issueSubscribers)
    .where(
      and(
        eq(issueSubscribers.issueId, issueId),
        eq(issueSubscribers.unsubscribed, false)
      )
    )
  const ids = new Set(rows.map((r) => r.userId))
  ids.delete(actorUserId)
  return [...ids]
}

// Write the in-app notification row(s) ALWAYS, and fire a push only when the
// workspace plan allows it (the row-write is decoupled from canUsePush so the
// inbox works on free plans; D7). Recipients are de-duped and agent-filtered.
async function deliver(args: {
  issue: IssueMeta
  recipientIds: string[]
  type: NotificationType
  pushType: PushType
  title: string
  body: string | null
}): Promise<void> {
  const recipients = await withoutAgents([...new Set(args.recipientIds)])
  if (recipients.length === 0) return

  const canPush = await canUsePush(args.issue.workspaceId)
  const now = new Date()

  await db.insert(notifications).values(
    recipients.map((userId) => ({
      userId,
      issueId: args.issue.id,
      type: args.type,
      title: args.title,
      body: args.body,
      pushedAt: canPush ? now : null,
    }))
  )

  if (!canPush) return
  await Promise.all(
    recipients.map((userId) =>
      sendToUser(userId, {
        title: args.title,
        body: args.body ?? args.issue.title,
        data: {
          type: args.pushType,
          issueId: args.issue.id,
          identifier: args.issue.identifier,
        },
      })
    )
  )
}

/**
 * Notify the new assignee that an issue was assigned to them (targeted). Writes
 * an `issue_assigned` row (previously this was push-only) + a plan-gated push.
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
      const issue = await loadIssueMeta(issueId)
      if (!issue) return
      const name = await actorName(actorUserId)
      await deliver({
        issue,
        recipientIds: [newAssigneeId],
        type: `issue_assigned`,
        pushType: `issue_assigned`,
        title: `${name} assigned you ${issue.identifier}`,
        body: issue.title,
      })
    } catch (err) {
      console.error(`[notify] assignment failed:`, err)
    }
  })()
}

/**
 * Notify on a new comment. Mentioned users (minus the actor) get an
 * `issue_mention`; the issue's other subscribers (minus the actor, minus the
 * mentioned) get an `issue_comment`. Mention wins so nobody is double-notified.
 */
export function fireAndForgetCommentNotify(args: {
  issueId: string
  actorUserId: string
  commentBodyText: string
  mentionedUserIds?: string[]
}): void {
  const { issueId, actorUserId, commentBodyText, mentionedUserIds = [] } = args

  void (async () => {
    try {
      const issue = await loadIssueMeta(issueId)
      if (!issue) return

      const mentioned = new Set(mentionedUserIds)
      mentioned.delete(actorUserId)
      const subscribers = await subscriberRecipients(issueId, actorUserId)
      const commentRecipients = subscribers.filter((id) => !mentioned.has(id))

      const name = await actorName(actorUserId)
      const previewSource = commentBodyText.trim()
      const preview =
        previewSource.length > 140
          ? `${previewSource.slice(0, 139)}…`
          : previewSource

      if (mentioned.size > 0) {
        await deliver({
          issue,
          recipientIds: [...mentioned],
          type: `issue_mention`,
          pushType: `issue_mention`,
          title: `${name} mentioned you in ${issue.identifier}`,
          body: preview || issue.title,
        })
      }
      if (commentRecipients.length > 0) {
        await deliver({
          issue,
          recipientIds: commentRecipients,
          type: `issue_comment`,
          pushType: `issue_comment`,
          title: `${name} commented on ${issue.identifier}`,
          body: preview || issue.title,
        })
      }
    } catch (err) {
      console.error(`[notify] comment failed:`, err)
    }
  })()
}

/**
 * Notify subscribers (minus the actor) that an issue's status changed.
 */
export function fireAndForgetStatusChangeNotify(args: {
  issueId: string
  actorUserId: string
  fromStatus: string
  toStatus: string
}): void {
  const { issueId, actorUserId, fromStatus, toStatus } = args
  if (fromStatus === toStatus) return

  void (async () => {
    try {
      const issue = await loadIssueMeta(issueId)
      if (!issue) return
      const recipients = await subscriberRecipients(issueId, actorUserId)
      if (recipients.length === 0) return

      const name = await actorName(actorUserId)
      await deliver({
        issue,
        recipientIds: recipients,
        type: `issue_status_changed`,
        pushType: `issue_status_changed`,
        title: `${name} changed ${issue.identifier} to ${toStatus.replace(/_/g, ` `)}`,
        body: issue.title,
      })
    } catch (err) {
      console.error(`[notify] status change failed:`, err)
    }
  })()
}
