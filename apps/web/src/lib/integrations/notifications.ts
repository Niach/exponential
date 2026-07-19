import { and, desc, eq, inArray, isNotNull, isNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  emailDeliveries,
  issueSubscribers,
  issues,
  boards,
  supportMessages,
  supportThreads,
  users,
  widgetSubmissions,
  teamMembers,
  teams,
} from "@/db/schema"
import { sendToUser } from "@/lib/integrations/fcm"
import { emailEnabled, sendReporterResolutionEmail } from "@/lib/email"
import {
  isResolutionStatus,
  shouldSendReporterResolution,
} from "@/lib/notification-email-policy"
import type { NotificationType } from "@/lib/domain"

// The canonical push `data.type` discriminator vocabulary (D8/D13). The web
// emits these; the native clients route on them. Every value is now a real
// notification_type (including pr_opened/pr_merged), so the push discriminator
// and the inbox row type stay in lockstep.
export type PushType = NotificationType

interface IssueMeta {
  id: string
  identifier: string
  title: string
  teamId: string
  teamSlug: string
  boardSlug: string
  assigneeId: string | null
}

async function loadIssueMeta(issueId: string): Promise<IssueMeta | null> {
  const [row] = await db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      title: issues.title,
      teamId: boards.teamId,
      teamSlug: teams.slug,
      boardSlug: boards.slug,
      assigneeId: issues.assigneeId,
    })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .innerJoin(teams, eq(teams.id, boards.teamId))
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

// Keep only recipients who are CURRENT members of the issue's team and
// not agents (the widget-helpdesk bot has no inbox). The membership check is
// the security boundary: subscriber/assignee rows can be stale after a member
// is removed, and private issue content must never fan out to an ex-member.
// Exported for tests.
export async function deliverableRecipients(
  teamId: string,
  userIds: string[]
): Promise<string[]> {
  if (userIds.length === 0) return []
  const rows = await db
    .select({ id: teamMembers.userId })
    .from(teamMembers)
    .innerJoin(users, eq(users.id, teamMembers.userId))
    .where(
      and(
        eq(teamMembers.teamId, teamId),
        inArray(teamMembers.userId, userIds),
        eq(users.isAgent, false)
      )
    )
  const allowed = new Set(rows.map((r) => r.id))
  return userIds.filter((id) => allowed.has(id))
}

// The active (non-unsubscribed) subscribers of an issue, minus the actor.
// Widget-reporter rows (null userId + email) are excluded here — they receive
// the one-way resolution email, not in-app/push notifications. Rows can be
// stale (a removed member's subscriptions); membership is enforced downstream
// in deliver(), the single chokepoint for every fan-out path.
async function subscriberRecipients(
  issueId: string,
  actorUserId: string | null
): Promise<string[]> {
  const rows = await db
    .select({ userId: issueSubscribers.userId })
    .from(issueSubscribers)
    .where(
      and(
        eq(issueSubscribers.issueId, issueId),
        eq(issueSubscribers.unsubscribed, false),
        isNotNull(issueSubscribers.userId)
      )
    )
  const ids = new Set(rows.map((r) => r.userId as string))
  if (actorUserId) ids.delete(actorUserId)
  return [...ids]
}

// Fan out one logical event, push-first: the in-app notification row is
// written ALWAYS, then push fires immediately off the deduped delivered set.
// Email is deliberately NOT sent here — the hourly digest sweep
// (lib/notification-email-digest.ts) bundles whatever is still unread ~1h
// later into one email per user, so a notification read in time (the push did
// its job) never produces an email. Push and email are free delivery channels
// — neither is plan-gated. Recipients are de-duped, filtered to CURRENT
// team members, and bot-filtered.
//
// Idempotency: the fire-and-forget callers can run twice for one logical event
// (e.g. concurrent comment creations fanning out to the same subscribers), and
// `notifications` has no unique key to ON CONFLICT on. Adding one would need a
// migration (out of scope here), so the insert dedupes in-query instead:
// INSERT … SELECT … WHERE NOT EXISTS an identical recent row (same recipient,
// issue, type, title and body within a short window). RETURNING tells us which
// rows actually landed, so the push fan-out skips deduped recipients too (the
// digest additionally claims notifications.emailed_at atomically, so a
// notification row can never produce two emails). Two transactions racing in
// the same instant can still both pass the NOT EXISTS check (it can't see
// uncommitted rows) — a unique partial index would close that residual window —
// but this removes the practical double-writes at lowest risk.
const NOTIFICATION_DEDUPE_WINDOW = `30 seconds`

async function deliver(args: {
  issue: IssueMeta
  recipientIds: string[]
  type: NotificationType
  pushType: PushType
  title: string
  body: string | null
}): Promise<void> {
  const recipients = await deliverableRecipients(args.issue.teamId, [
    ...new Set(args.recipientIds),
  ])
  if (recipients.length === 0) return

  const now = new Date()

  const inserted = await db.execute(sql`
    insert into notifications (user_id, issue_id, type, title, body, pushed_at)
    select
      r.user_id,
      ${args.issue.id}::uuid,
      ${args.type}::notification_type,
      ${args.title},
      ${args.body},
      ${now}::timestamptz
    from unnest(${sql.param(recipients)}::text[]) as r(user_id)
    where not exists (
      select 1
      from notifications existing
      where existing.user_id = r.user_id
        and existing.issue_id = ${args.issue.id}::uuid
        and existing.type = ${args.type}::notification_type
        and existing.title = ${args.title}
        and existing.body is not distinct from ${args.body}::text
        and existing.created_at > now() - interval '${sql.raw(NOTIFICATION_DEDUPE_WINDOW)}'
    )
    returning id, user_id
  `)
  const delivered = inserted.rows.map((row) => ({
    notificationId: row.id as string,
    userId: row.user_id as string,
  }))
  if (delivered.length === 0) return

  // Push only — email waits for the digest sweep. Per-recipient push failures
  // never throw out of deliver().
  await Promise.all(
    delivered.map((d) =>
      sendToUser(d.userId, {
        title: args.title,
        body: args.body ?? args.issue.title,
        data: {
          type: args.pushType,
          issueId: args.issue.id,
          identifier: args.issue.identifier,
        },
      }).catch((err) => {
        console.error(`[notify] push to ${d.userId} failed:`, err)
      })
    )
  )
}

/**
 * Notify the new assignee that an issue was assigned to them (targeted). Writes
 * an `issue_assigned` row + push + email.
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
 * Notify every human member of the issue's team that a new issue landed
 * (EXP-53: widget feedback submissions). There is no actor to exclude — the
 * creator is the isAgent widget-helpdesk bot, which deliver()'s membership
 * filter drops anyway. Writes `issue_created` rows + push; email follows via
 * the digest sweep like every other type.
 */
export function fireAndForgetNewIssueNotify(args: { issueId: string }): void {
  void (async () => {
    try {
      const issue = await loadIssueMeta(args.issueId)
      if (!issue) return

      const memberRows = await db
        .select({ userId: teamMembers.userId })
        .from(teamMembers)
        .where(eq(teamMembers.teamId, issue.teamId))
      if (memberRows.length === 0) return

      await deliver({
        issue,
        recipientIds: memberRows.map((row) => row.userId),
        type: `issue_created`,
        pushType: `issue_created`,
        title: `New feedback: ${issue.identifier}`,
        body: issue.title,
      })
    } catch (err) {
      console.error(`[notify] new issue failed:`, err)
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
 * Notify members @-mentioned in an issue DESCRIPTION (create, or the newly
 * added mentions of an edit — the caller diffs old vs new so re-saving a
 * description never re-notifies). The description-side twin of the mention
 * half of fireAndForgetCommentNotify: mention recipients only, no subscriber
 * fan-out (editing a description is not a new comment).
 */
export function fireAndForgetIssueMentionNotify(args: {
  issueId: string
  actorUserId: string
  mentionedUserIds: string[]
}): void {
  void (async () => {
    try {
      const mentioned = new Set(args.mentionedUserIds)
      mentioned.delete(args.actorUserId)
      if (mentioned.size === 0) return

      const issue = await loadIssueMeta(args.issueId)
      if (!issue) return

      const name = await actorName(args.actorUserId)
      await deliver({
        issue,
        recipientIds: [...mentioned],
        type: `issue_mention`,
        pushType: `issue_mention`,
        title: `${name} mentioned you in ${issue.identifier}`,
        body: issue.title,
      })
    } catch (err) {
      console.error(`[notify] issue mention failed:`, err)
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

/**
 * PR lifecycle fan-out (pr_opened from the MCP open_pr tool + the webhook's
 * out-of-band linking; pr_merged from applyPrMergeState's idempotent guard).
 * Targets the assignee + active subscribers, minus the actor — the away/phone
 * flow's "PR opened" / "it's merged" on all three channels. actorUserId is
 * null for webhook/cron-driven merges with no mapped app user.
 */
export function fireAndForgetPrNotify(args: {
  issueId: string
  type: `pr_opened` | `pr_merged`
  actorUserId?: string | null
}): void {
  const { issueId, type } = args
  const actorUserId = args.actorUserId ?? null

  void (async () => {
    try {
      const issue = await loadIssueMeta(issueId)
      if (!issue) return

      const recipients = new Set(
        await subscriberRecipients(issueId, actorUserId)
      )
      if (issue.assigneeId && issue.assigneeId !== actorUserId) {
        // Respect an explicit unsubscribe: an assignee who muted the issue
        // must not be re-added over their opt-out.
        const [optedOut] = await db
          .select({ userId: issueSubscribers.userId })
          .from(issueSubscribers)
          .where(
            and(
              eq(issueSubscribers.issueId, issueId),
              eq(issueSubscribers.userId, issue.assigneeId),
              eq(issueSubscribers.unsubscribed, true)
            )
          )
          .limit(1)
        if (!optedOut) {
          recipients.add(issue.assigneeId)
        }
      }
      if (recipients.size === 0) return

      const name = actorUserId ? await actorName(actorUserId) : null
      const title =
        type === `pr_opened`
          ? name
            ? `${name} opened a pull request for ${issue.identifier}`
            : `A pull request was opened for ${issue.identifier}`
          : name
            ? `${name} merged the pull request for ${issue.identifier}`
            : `The pull request for ${issue.identifier} was merged`

      await deliver({
        issue,
        recipientIds: [...recipients],
        type,
        pushType: type,
        title,
        body: issue.title,
      })
    } catch (err) {
      console.error(`[notify] pr ${type} failed:`, err)
    }
  })()
}

// Issue-less sibling of deliver(): the fan-out for events that have no
// backing issue (standalone helpdesk tickets). Same dedupe-insert shape with
// `issue_id IS NULL` (the trigger-denormalized board_id stays NULL too, so
// the notifications shape's `board_id IS NULL` arm keeps these rows synced)
// and the same push-first delivery; the push payload carries no issue keys —
// natives route on `type` alone. The row DOES carry the team id (synced) so
// every client's inbox can route the notification to the right team's
// Support surface.
async function deliverToTeam(args: {
  teamId: string
  recipientIds: string[]
  type: NotificationType
  title: string
  body: string | null
  pushData: Record<string, string>
}): Promise<void> {
  const recipients = await deliverableRecipients(args.teamId, [
    ...new Set(args.recipientIds),
  ])
  if (recipients.length === 0) return

  const now = new Date()

  const inserted = await db.execute(sql`
    insert into notifications (user_id, issue_id, team_id, type, title, body, pushed_at)
    select
      r.user_id,
      null,
      ${args.teamId}::uuid,
      ${args.type}::notification_type,
      ${args.title},
      ${args.body},
      ${now}::timestamptz
    from unnest(${sql.param(recipients)}::text[]) as r(user_id)
    where not exists (
      select 1
      from notifications existing
      where existing.user_id = r.user_id
        and existing.issue_id is null
        and existing.type = ${args.type}::notification_type
        and existing.title = ${args.title}
        and existing.body is not distinct from ${args.body}::text
        and existing.created_at > now() - interval '${sql.raw(NOTIFICATION_DEDUPE_WINDOW)}'
    )
    returning id, user_id
  `)
  const delivered = inserted.rows.map((row) => ({
    userId: row.user_id as string,
  }))
  if (delivered.length === 0) return

  await Promise.all(
    delivered.map((d) =>
      sendToUser(d.userId, {
        title: args.title,
        body: args.body ?? args.title,
        data: { type: args.type, ...args.pushData },
      }).catch((err) => {
        console.error(`[notify] push to ${d.userId} failed:`, err)
      })
    )
  )
}

/**
 * Helpdesk: a new ticket arrived or the external reporter replied. Broadcast
 * to every human team member (the support inbox is a shared surface and
 * there is no actor to exclude). The preview is reporter-authored UNTRUSTED
 * text: it is written as a plain string and the digest email escapes bodies,
 * so no extra sanitizing is needed here beyond truncation.
 */
export function fireAndForgetSupportThreadNotify(args: {
  threadId: string
  kind: `created` | `reply`
}): void {
  void (async () => {
    try {
      const [thread] = await db
        .select({
          id: supportThreads.id,
          teamId: supportThreads.teamId,
          title: supportThreads.title,
          reporterName: supportThreads.reporterName,
          reporterEmail: supportThreads.reporterEmail,
        })
        .from(supportThreads)
        .where(eq(supportThreads.id, args.threadId))
        .limit(1)
      if (!thread) return

      const memberRows = await db
        .select({ userId: teamMembers.userId })
        .from(teamMembers)
        .where(eq(teamMembers.teamId, thread.teamId))
      if (memberRows.length === 0) return

      // Preview: the latest public inbound message (the reporter's words).
      const [latest] = await db
        .select({ body: supportMessages.body })
        .from(supportMessages)
        .where(
          and(
            eq(supportMessages.threadId, thread.id),
            eq(supportMessages.direction, `inbound`),
            eq(supportMessages.visibility, `public`)
          )
        )
        .orderBy(desc(supportMessages.createdAt))
        .limit(1)
      const previewSource = (latest?.body ?? thread.title).trim()
      const preview =
        previewSource.length > 140
          ? `${previewSource.slice(0, 139)}…`
          : previewSource

      const who = thread.reporterName || thread.reporterEmail
      await deliverToTeam({
        teamId: thread.teamId,
        recipientIds: memberRows.map((row) => row.userId),
        type: `support_reply`,
        title:
          args.kind === `created`
            ? `New support ticket from ${who}`
            : `${who} replied on a support ticket`,
        body: preview || thread.title,
        pushData: { threadId: thread.id },
      })
    } catch (err) {
      console.error(`[notify] support ${args.kind} failed:`, err)
    }
  })()
}

/**
 * One-way helpdesk (§6.4): when a widget-reported issue is closed
 * (done/cancelled), email the external reporter(s) a CLEAN resolution notice —
 * no internal metadata, no in-app/push rows (reporters have no account).
 *
 * Exactly once per close: widget_submissions.resolvedNotifiedAt is claimed
 * atomically (set-once, never cleared on reopen), so a reopen→re-close never
 * re-emails. With no email transport configured the send is skipped WITHOUT
 * claiming the flag, so configuring email later still allows the notice on the
 * next close. Never throws.
 */
export function fireAndForgetReporterResolution(args: {
  issueId: string
  toStatus: string
}): void {
  const { issueId, toStatus } = args
  if (!isResolutionStatus(toStatus)) return

  void (async () => {
    try {
      // Self-host optionality (§6.6): no transport → skip silently, leave the
      // flag unset.
      if (!emailEnabled) return

      const [submission] = await db
        .select({
          id: widgetSubmissions.id,
          resolvedNotifiedAt: widgetSubmissions.resolvedNotifiedAt,
        })
        .from(widgetSubmissions)
        .where(eq(widgetSubmissions.issueId, issueId))
        .limit(1)
      if (!submission) return
      if (
        !shouldSendReporterResolution({
          toStatus,
          resolvedNotifiedAt: submission.resolvedNotifiedAt,
        })
      ) {
        return
      }

      const reporterRows = await db
        .select({ email: issueSubscribers.email })
        .from(issueSubscribers)
        .where(
          and(
            eq(issueSubscribers.issueId, issueId),
            eq(issueSubscribers.source, `widget_reporter`),
            eq(issueSubscribers.unsubscribed, false),
            isNotNull(issueSubscribers.email)
          )
        )
      const emails = [...new Set(reporterRows.map((r) => r.email as string))]
      if (emails.length === 0) return

      // Atomic set-once claim — concurrent closers race on the NULL check.
      const claimed = await db
        .update(widgetSubmissions)
        .set({ resolvedNotifiedAt: new Date() })
        .where(
          and(
            eq(widgetSubmissions.id, submission.id),
            isNull(widgetSubmissions.resolvedNotifiedAt)
          )
        )
        .returning({ id: widgetSubmissions.id })
      if (claimed.length === 0) return

      const issue = await loadIssueMeta(issueId)
      const issueTitle = issue?.title ?? `Your report`

      await Promise.all(
        emails.map(async (email) => {
          try {
            const result = await sendReporterResolutionEmail({
              to: email,
              issueTitle,
            })
            await db.insert(emailDeliveries).values({
              userId: null,
              toEmail: email,
              issueId,
              kind: `widget_resolution`,
              status: result.delivered ? `sent` : `failed`,
              provider: result.provider,
              providerMessageId: result.messageId,
              sentAt: result.delivered ? new Date() : null,
            })
          } catch (err) {
            console.error(
              `[notify] reporter resolution email to ${email} failed:`,
              err
            )
          }
        })
      )
    } catch (err) {
      console.error(`[notify] reporter resolution failed:`, err)
    }
  })()
}
