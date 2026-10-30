import { resolveSessionHost } from "@/lib/sessions/host"
import { ASSISTANT_COPY } from "@exp/domain-contract/agent-settings"
import {
  and,
  desc,
  eq,
  gte,
  inArray,
  isNotNull,
  isNull,
  sql,
} from "drizzle-orm"
import { db } from "@/db/connection"
import {
  conversations,
  agents,
  comments,
  messages,
  sessions,
  emailDeliveries,
  issueSubscribers,
  issues,
  boards,
  notifications,
  users,
  widgetSubmissions,
  teamMembers,
  teams,
} from "@/db/schema"
import { sendToUsers } from "@/lib/integrations/fcm"
import { deliveryStatus, sendReporterResolutionEmail } from "@/lib/email"
import { emailEnabled } from "@/lib/email-enabled"
import {
  isResolutionStatus,
  notificationTypeAllowed,
  shouldSendReporterResolution,
} from "@/lib/notification-email-policy"
import {
  getAgentMessageBlocklist,
  getTypePrefsMap,
} from "@/lib/notification-prefs"
import { peekAgentIssueActors } from "@/lib/integrations/pr-actor-claims"
import { recordNotificationFanout } from "@/lib/metrics/registry"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import { deriveSessionTitle } from "@/lib/sessions/brief"
import { resolveMentions } from "@/lib/integrations/mentions"
import { isSealedBody, isSealedMessage } from "@/lib/sessions/sealed"
import { reporterFacingText } from "@/lib/reporter/report-text"
import type { CodingSessionBlocked } from "@exp/db-schema/domain"
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

async function actorName(
  actorUserId: string,
  // #EXP-1383: the agent whose session acted names the notification
  // (`Sloppy commented on EXP-12`); a deleted one falls back to the member.
  actorAgentId?: string | null
): Promise<string> {
  if (actorAgentId) {
    const [agent] = await db
      .select({ name: agents.name })
      .from(agents)
      .where(eq(agents.id, actorAgentId))
      .limit(1)
    if (agent?.name) return agent.name
  }
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
    .where(
      and(
        eq(teamMembers.teamId, teamId),
        inArray(teamMembers.userId, userIds)
      )
    )
  const allowed = new Set(rows.map((r) => r.id))
  return userIds.filter((id) => allowed.has(id))
}

// The active (non-unsubscribed) subscribers of an issue, minus everyone in the
// self-exclusion set (EXP-617: a SET, not one actor — see resolvePrActors).
// Widget-reporter rows (null userId + email) are excluded here — they receive
// the one-way resolution email, not in-app/push notifications. Rows can be
// stale (a removed member's subscriptions); membership is enforced downstream
// in deliver(), the single chokepoint for every fan-out path.
async function subscriberRecipients(
  issueId: string,
  excluded: ReadonlySet<string>
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
  for (const id of excluded) ids.delete(id)
  return [...ids]
}

// Convenience for the single-actor fan-outs (comment, status change, …), whose
// exclusion really is just "the one person who did it".
function selfSet(actorUserId: string | null): ReadonlySet<string> {
  return actorUserId ? new Set([actorUserId]) : EMPTY_EXCLUSION
}

const EMPTY_EXCLUSION: ReadonlySet<string> = new Set<string>()

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
// but this removes the practical double-writes at lowest risk. The probe is
// index-bounded by idx_notifications_user_created (user_id, created_at DESC).
const NOTIFICATION_DEDUPE_WINDOW = `30 seconds`

// EXP-617: the title is part of that probe, which makes the whole racing-pair
// design lean on "both legs resolve the SAME actor so the titles match
// byte-for-byte". That is brittle — any attribution asymmetry (one leg names
// Ada, the other says nobody) sprays two rows for one event, and the anonymous
// one is exactly the one that reaches the person who did the work. PR fan-outs
// therefore dedupe on the EVENT instead: for pr_opened/pr_merged, (issue, type)
// IS the logical identity — applyPrOpenedState/applyPrMergeState are already
// idempotent per issue, so a second legitimate one inside the window cannot
// exist. Every other type keeps the title/body terms, where two genuinely
// different comments on one issue must not collapse into one.
type DedupeScope = `title` | `event`

// Per-type prefs gate PUSH too (EXP-369) — the toggles used to be email-only,
// so a user who muted "Comments" still got every comment on their phone. The
// inbox ROW is always written (the toggles suppress delivery, not the record),
// and the lookup FAILS OPEN: a prefs query error must not silence push.
async function pushRecipients<T extends { userId: string }>(
  delivered: T[],
  type: NotificationType
): Promise<T[]> {
  const typePrefs = await getTypePrefsMap(
    delivered.map((row) => row.userId)
  ).catch(() => new Map())
  return delivered.filter((row) =>
    notificationTypeAllowed(typePrefs.get(row.userId), type)
  )
}

async function deliver(args: {
  issue: IssueMeta
  recipientIds: string[]
  type: NotificationType
  pushType: PushType
  title: string
  body: string | null
  dedupeScope?: DedupeScope
}): Promise<void> {
  const recipients = await deliverableRecipients(args.issue.teamId, [
    ...new Set(args.recipientIds),
  ])
  if (recipients.length === 0) return

  const now = new Date()

  const contentMatch =
    args.dedupeScope === `event`
      ? sql``
      : sql`and existing.title = ${args.title}
        and existing.body is not distinct from ${args.body}::text`

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
        ${contentMatch}
        and existing.created_at > now() - interval '${sql.raw(NOTIFICATION_DEDUPE_WINDOW)}'
    )
    returning id, user_id
  `)
  const delivered = inserted.rows.map((row) => ({
    notificationId: row.id as string,
    userId: row.user_id as string,
  }))
  recordNotificationFanout({
    requested: recipients.length,
    inserted: delivered.length,
  })
  if (delivered.length === 0) return

  // Push only — email waits for the digest sweep. sendToUsers batches the
  // token lookup into one query and caps + times out the relay POSTs (REV2-3);
  // push failures never throw out of deliver().
  //
  // EXP-264: the payload carries enough to route a tap WITHOUT waiting for the
  // Electric shapes to catch up — teamSlug/boardSlug complete the issue's deep
  // link, and each recipient's own notificationId lets the tapped client mark
  // exactly that inbox row read.
  const pushable = await pushRecipients(delivered, args.type)
  if (pushable.length === 0) return
  await sendToUsers(
    pushable.map((d) => ({
      userId: d.userId,
      data: { notificationId: d.notificationId },
    })),
    {
      title: args.title,
      body: args.body ?? args.issue.title,
      data: {
        type: args.pushType,
        issueId: args.issue.id,
        identifier: args.issue.identifier,
        teamSlug: args.issue.teamSlug,
        boardSlug: args.issue.boardSlug,
      },
    }
  ).catch((err) => {
    console.error(`[notify] push fan-out failed:`, err)
  })
}

/**
 * Notify the new assignee that an issue was assigned to them (targeted). Writes
 * an `issue_assigned` row + push + email.
 */
export function fireAndForgetAssignmentNotify(args: {
  issueId: string
  actorUserId: string
  actorAgentId?: string | null
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
      const name = await actorName(actorUserId, args.actorAgentId)
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
 * (EXP-53: widget feedback submissions). There is no actor to exclude — a
 * widget-filed issue has a null creator. Writes `issue_created` rows + push;
 * email follows via the digest sweep like every other type.
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
  actorAgentId?: string | null
  commentBodyText: string
  mentionedUserIds?: string[]
  attachmentCount?: number
}): void {
  const {
    issueId,
    actorUserId,
    commentBodyText,
    mentionedUserIds = [],
    attachmentCount = 0,
  } = args

  void (async () => {
    try {
      const issue = await loadIssueMeta(issueId)
      if (!issue) return

      const mentioned = new Set(mentionedUserIds)
      mentioned.delete(actorUserId)
      const subscribers = await subscriberRecipients(
        issueId,
        selfSet(actorUserId)
      )
      const commentRecipients = subscribers.filter((id) => !mentioned.has(id))

      const name = await actorName(actorUserId, args.actorAgentId)
      const previewSource = commentBodyText.trim()
      const preview =
        previewSource.length > 140
          ? `${previewSource.slice(0, 139)}…`
          : previewSource
      // Attachment-only comments (EXP-554) have no text to preview.
      const attachmentFallback =
        attachmentCount > 0
          ? `Sent ${attachmentCount} attachment${attachmentCount === 1 ? `` : `s`}`
          : ``
      const body = preview || attachmentFallback || issue.title

      if (mentioned.size > 0) {
        await deliver({
          issue,
          recipientIds: [...mentioned],
          type: `issue_mention`,
          pushType: `issue_mention`,
          title: `${name} mentioned you in ${issue.identifier}`,
          body,
        })
      }
      if (commentRecipients.length > 0) {
        await deliver({
          issue,
          recipientIds: commentRecipients,
          type: `issue_comment`,
          pushType: `issue_comment`,
          title: `${name} commented on ${issue.identifier}`,
          body,
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
  actorAgentId?: string | null
  mentionedUserIds: string[]
}): void {
  void (async () => {
    try {
      const mentioned = new Set(args.mentionedUserIds)
      mentioned.delete(args.actorUserId)
      if (mentioned.size === 0) return

      const issue = await loadIssueMeta(args.issueId)
      if (!issue) return

      const name = await actorName(args.actorUserId, args.actorAgentId)
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
  actorAgentId?: string | null
  fromStatus: string
  toStatus: string
  // EXP-314: the precise status-row pair + display-name snapshot. The ids
  // keep a custom→custom move within one category (same anchor enum) from
  // being suppressed as a no-op; toName carries the custom status's name
  // into the copy.
  fromStatusId?: string | null
  toStatusId?: string | null
  toName?: string | null
}): void {
  const { issueId, actorUserId, fromStatus, toStatus } = args
  if (
    fromStatus === toStatus &&
    (args.fromStatusId ?? null) === (args.toStatusId ?? null)
  )
    return

  void (async () => {
    try {
      const issue = await loadIssueMeta(issueId)
      if (!issue) return
      const recipients = await subscriberRecipients(
        issueId,
        selfSet(actorUserId)
      )
      if (recipients.length === 0) return

      const name = await actorName(actorUserId, args.actorAgentId)
      await deliver({
        issue,
        recipientIds: recipients,
        type: `issue_status_changed`,
        pushType: `issue_status_changed`,
        title: `${name} changed ${issue.identifier} to ${args.toName ?? toStatus.replace(/_/g, ` `)}`,
        body: issue.title,
      })
    } catch (err) {
      console.error(`[notify] status change failed:`, err)
    }
  })()
}

// Fallback attribution for webhook/cron PR events, which carry no mapped app
// user (EXP-463): the starter of the session that opened a PR closing the
// issue (EXP-1291: `pull_requests.session_id`), else of a thread holding the
// issue, is the human whose agent produced the PR. Without this, a null actor
// makes the exclusion below a no-op and the auto-subscribed creator gets
// pushed about their OWN agent's PR. Ordered PR openers first, then most
// recent: the HEAD is the naming candidate, and EVERY row contributes its
// starter AND its host to the self-exclusion set. A genuinely out-of-band PR
// with no session stays anonymous.
const PR_SESSION_CANDIDATE_LIMIT = 4

interface PrSessionCandidate {
  userId: string
  /** NEW-1: the account hosting it (derived from the devices rows), when
   *  that is someone else. */
  hostUserId: string | null
}

async function loadPrSessionCandidates(
  issue: IssueMeta
): Promise<PrSessionCandidate[]> {
  const openers = sql`SELECT pr.session_id FROM pull_requests pr
    JOIN pull_request_issues pri ON pri.pull_request_id = pr.id
    WHERE pri.issue_id = ${issue.id} AND pr.session_id IS NOT NULL`
  const rows = await db
    .select({
      userId: sessions.userId,
      deviceId: sessions.deviceId,
      teamId: sessions.teamId,
    })
    .from(sessions)
    .where(
      // The PR's opener, or any session of a thread holding the issue.
      sql`${sessions.id} IN (${openers}) OR ${sessions.rootId} IN (SELECT s.root_id FROM session_issues si JOIN sessions s ON s.id = si.session_id WHERE si.issue_id = ${issue.id})`
    )
    // The PR openers outrank the threads that only hold the issue.
    .orderBy(sql`(${sessions.id} IN (${openers})) DESC`, desc(sessions.createdAt))
    .limit(PR_SESSION_CANDIDATE_LIMIT)
  const out: PrSessionCandidate[] = []
  for (const row of rows) {
    const host = await resolveSessionHost(row)
    out.push({ userId: row.userId, hostUserId: host && host !== row.userId ? host : null })
  }
  return out
}

/**
 * Who to NAME and who to KEEP QUIET for one PR event (EXP-617).
 *
 * These are two different questions and they used to share one variable, which
 * is what made every attribution miss a self-notification: naming has to be
 * exactly right or the copy lies, while excluding only has to be a SUPERSET of
 * the people who plausibly caused this. Coupling them let the strict
 * requirement dictate the loose one — a null actor made exclusion a complete
 * no-op and the anonymous fan-out reached the very person whose agent had just
 * done the work. The exclusion set can only ever REMOVE recipients, never
 * misroute one, so it is safe to feed it evidence too weak for a title.
 *
 * Naming ladder, strongest per-event evidence first:
 *   1. `actorUserId` — the in-app mutation's own leg, or the webhook's
 *      consumed claim (pr-actor-claims.ts). `actorViaAgent` still swaps a
 *      shared-server HOST to the session's requester (EXP-432).
 *   2. `githubActorUserId` — `sender`/`merged_by` mapped through
 *      github-identity.ts. Ranked ABOVE the session heuristic on purpose: it
 *      is a fact about THIS pull request, which retires the old wart where a
 *      teammate merging on github.com was credited to the session owner. Only
 *      names a CURRENT member of the issue's team; an outside contributor is
 *      still excluded, just not printed. This rung is the one that actually
 *      fixes EXP-617: the reported notification came from a PR opened by the
 *      reporter's own GitHub account on github.com, where every other rung
 *      here is structurally blind — there is no claim, no session, and no
 *      server call to attribute.
 *   3. The coding-session owner — a correlation heuristic, unchanged.
 *   4. Nobody: the anonymous title.
 *
 * Exclusion is the union of the named actor, the raw pre-swap actor, the
 * mapped GitHub actor, every session candidate consulted, and
 * `peekAgentIssueActors` (an agent that WROTE to this issue under someone's
 * MCP credential in the last half hour — the rung that covers a second issue
 * filed and implemented inside another issue's session). Note what is NOT in
 * there: `issues.creator_id`. "I filed it, a teammate implemented it, here is
 * the PR" is the most valuable notification this product sends.
 *
 * Session rows are consulted only when they can change the outcome — never for
 * a plain human actor, so a host merging a requester's PR from the web still
 * notifies the requester.
 */
export async function resolvePrActors(
  issue: IssueMeta,
  args: {
    actorUserId?: string | null
    actorViaAgent?: boolean
    githubActorUserId?: string | null
  }
): Promise<{ actorUserId: string | null; excluded: Set<string> }> {
  const excluded = new Set<string>()
  if (args.actorUserId) excluded.add(args.actorUserId)
  if (args.githubActorUserId) excluded.add(args.githubActorUserId)
  for (const id of peekAgentIssueActors(issue.id)) excluded.add(id)

  let actorUserId: string | null = args.actorUserId ?? null

  if (actorUserId && args.actorViaAgent) {
    const [head] = await loadPrSessionCandidates(issue)
    if (head) {
      excluded.add(head.userId)
      if (head.hostUserId) excluded.add(head.hostUserId)
      if (head.hostUserId != null && head.hostUserId === actorUserId) {
        actorUserId = head.userId
      }
    }
  }

  if (!actorUserId && args.githubActorUserId) {
    const [member] = await deliverableRecipients(issue.teamId, [
      args.githubActorUserId,
    ])
    if (member) actorUserId = member
  }

  if (!actorUserId) {
    const candidates = await loadPrSessionCandidates(issue)
    for (const candidate of candidates) {
      excluded.add(candidate.userId)
      if (candidate.hostUserId) excluded.add(candidate.hostUserId)
    }
    actorUserId = candidates[0]?.userId ?? null
  }

  if (actorUserId) excluded.add(actorUserId)
  return { actorUserId, excluded }
}

/**
 * PR lifecycle fan-out (pr_opened from the MCP open_pr tool + the webhook's
 * out-of-band linking; pr_merged from applyPrMergeState's idempotent guard).
 * Targets the assignee + active subscribers, minus everyone who plausibly
 * caused the event — the away/phone flow's "PR opened" / "it's merged" on all
 * three channels. See resolvePrActors above for who gets named and who gets
 * kept quiet; `githubActorUserId` is the webhook/poller's `sender`/`merged_by`
 * already mapped to an app user by the caller.
 *
 * Deliberate residual gap (EXP-617): an out-of-band PR from a GitHub account
 * nobody has ever connected to this instance stays anonymous, and if its
 * author subscribes to the issue they still hear about their own PR. That is
 * now bounded by "has never connected GitHub here" rather than by "no coding
 * session happened to exist".
 */
export function fireAndForgetPrNotify(args: {
  issueId: string
  type: `pr_opened` | `pr_merged`
  actorUserId?: string | null
  actorViaAgent?: boolean
  githubActorUserId?: string | null
}): void {
  const { issueId, type } = args

  void (async () => {
    try {
      const issue = await loadIssueMeta(issueId)
      if (!issue) return

      const { actorUserId, excluded } = await resolvePrActors(issue, args)

      const recipients = new Set(await subscriberRecipients(issueId, excluded))
      if (issue.assigneeId && !excluded.has(issue.assigneeId)) {
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
        // The racing legs may still disagree on the title (one names Ada, the
        // other nobody) — dedupe on the event so a teammate never gets two.
        dedupeScope: `event`,
      })
    } catch (err) {
      console.error(`[notify] pr ${type} failed:`, err)
    }
  })()
}

// Issue-less sibling of deliver(): the fan-out for events that have no
// backing issue (`agent_message`, `session_blocked`). Same dedupe-insert
// shape with `issue_id IS NULL` (the trigger-denormalized board_id stays
// NULL too, so the notifications shape's `board_id IS NULL` arm keeps these
// rows synced) and the same push-first delivery; the push payload carries no
// issue keys — natives route on `type` alone. The row DOES carry the team id
// (synced) so every client's inbox can route the notification to the right
// team.
async function deliverToTeam(args: {
  teamId: string
  recipientIds: string[]
  type: NotificationType
  title: string
  body: string | null
  pushData: Record<string, string>
  /** EXP-980: the run a `session_blocked` row routes to. */
  sessionId?: string
  /** EXP-933: the issue an `agent_message` opens (its Results). The board
   *  columns then fill from the issue trigger (trash-scoped like any issue
   *  row); callers check the issue belongs to `teamId`. */
  issueId?: string | null
}): Promise<TeamDelivery> {
  const recipients = await deliverableRecipients(args.teamId, [
    ...new Set(args.recipientIds),
  ])
  const outcome: TeamDelivery = {
    members: recipients,
    delivered: [],
  }
  if (recipients.length === 0) return outcome

  const now = new Date()

  const inserted = await db.execute(sql`
    insert into notifications (user_id, issue_id, team_id, session_id, type, title, body, pushed_at)
    select
      r.user_id,
      ${args.issueId ?? null}::uuid,
      ${args.teamId}::uuid,
      ${args.sessionId ?? null}::uuid,
      ${args.type}::notification_type,
      ${args.title},
      ${args.body},
      ${now}::timestamptz
    from unnest(${sql.param(recipients)}::text[]) as r(user_id)
    where not exists (
      select 1
      from notifications existing
      where existing.user_id = r.user_id
        and existing.issue_id is not distinct from ${args.issueId ?? null}::uuid
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
  recordNotificationFanout({
    requested: recipients.length,
    inserted: delivered.length,
  })
  outcome.delivered = delivered.map((d) => d.userId)
  if (delivered.length === 0) return outcome

  // Same EXP-264 contract as deliver(): each recipient's push carries the id
  // of the row that was actually written for them.
  const pushable = await pushRecipients(delivered, args.type)
  if (pushable.length === 0) return outcome
  await sendToUsers(
    pushable.map((d) => ({
      userId: d.userId,
      data: { notificationId: d.notificationId },
    })),
    {
      title: args.title,
      body: args.body ?? args.title,
      data: { type: args.type, ...args.pushData },
    }
  ).catch((err) => {
    console.error(`[notify] push fan-out failed:`, err)
  })
  return outcome
}

// What deliverToTeam did, for callers that answer synchronously (EXP-801):
// `members` = the requested ids that passed the membership guard, `delivered`
// = those that got a NEW row (the rest hit the dedupe window).
interface TeamDelivery {
  members: string[]
  delivered: string[]
}

/** Per-recipient result of one `sendAgentMessage` call. Every requested id
 * lands in exactly one bucket. */
export interface AgentMessageOutcome {
  delivered: string[]
  /** Turned off "messages from teammates' agents" — nothing was written. */
  declined: string[]
  /** Not (or no longer) a member of the team. */
  notMembers: string[]
  /** An identical message reached them inside the dedupe window. */
  deduped: string[]
}

/**
 * EXP-801: an agent messages team members (or its own user) over MCP —
 * `exponential_notifications_send`. Synchronous, unlike the fire-and-forget
 * fan-outs: the agent gets told who received it. An inbox row
 * (`agent_message`, team-scoped; EXP-933: with `issueId`
 * it rides that issue and opens its Results face ×4) + the usual push-first
 * delivery (per-type prefs still mute push; email follows via the digest).
 * The recipient's `allow_agent_messages` pref is a BLOCK, not a mute: a
 * declined recipient gets no row at all. The sender's own user always
 * passes — asking your own agent to ping you is the whole point.
 */
export async function sendAgentMessage(args: {
  teamId: string
  senderUserId: string
  recipientIds: string[]
  title: string
  body: string | null
  /** EXP-933: the issue whose Results the row opens; null = opens nothing. */
  issueId?: string | null
}): Promise<AgentMessageOutcome> {
  const requested = [...new Set(args.recipientIds)]
  const [team] = await db
    .select({ slug: teams.slug })
    .from(teams)
    .where(eq(teams.id, args.teamId))
    .limit(1)
  if (!team) throw new Error(`Team not found`)
  const issue = args.issueId ? await loadIssueMeta(args.issueId) : null
  if (args.issueId && (!issue || issue.teamId !== args.teamId)) {
    throw new Error(`The target issue is not in this team.`)
  }

  const blocked = await getAgentMessageBlocklist(
    requested.filter((id) => id !== args.senderUserId)
  )
  const declined = requested.filter((id) => blocked.has(id))
  const candidates = requested.filter((id) => !blocked.has(id))

  const name = await actorName(args.senderUserId)
  const result = await deliverToTeam({
    teamId: args.teamId,
    recipientIds: candidates,
    type: `agent_message`,
    // The row carries no actor column, so the sentence names the sender —
    // the reader must know whose agent is talking (and whom to mute).
    title: `${name}'s agent: ${args.title}`,
    body: args.body,
    issueId: issue?.id ?? null,
    // EXP-933: a targeted message deep-links the tap to the issue's Results
    // face, with the same issue keys deliver() pushes (EXP-264).
    pushData: issue
      ? {
          teamId: args.teamId,
          teamSlug: team.slug,
          issueId: issue.id,
          identifier: issue.identifier,
          boardSlug: issue.boardSlug,
          face: `results`,
        }
      : { teamId: args.teamId, teamSlug: team.slug },
  })
  const members = new Set(result.members)
  const delivered = new Set(result.delivered)
  return {
    delivered: candidates.filter((id) => delivered.has(id)),
    declined,
    notMembers: candidates.filter((id) => !members.has(id)),
    deduped: candidates.filter((id) => members.has(id) && !delivered.has(id)),
  }
}

// EXP-1005: the per-profile wall-notification window.
const SESSION_BLOCKED_THROTTLE_MS = 60 * 60 * 1000

function blockedProfileKey(row: {
  deviceId: string | null
  cli: string | null
  account: string | null
}): string {
  return JSON.stringify([row.deviceId ?? null, row.cli ?? null, row.account ?? `system`])
}

/**
 * EXP-980: a session hit a wall (`sessions.blocked` went null → set). Its
 * STARTER is told: a walled session still reads live, so without a push
 * nobody notices until they look. One issue-less row carrying `session_id` +
 * `team_id` (the inbox row and the push both route to the session),
 * push-first, muted per type by the usual prefs. Best-effort, never throws:
 * the caller is the device's `sessions.report` write.
 */
export async function notifySessionBlocked(
  sessionId: string,
  blocked: CodingSessionBlocked
): Promise<void> {
  try {
    const [session] = await db
      .select({
        id: sessions.id,
        rootId: sessions.rootId,
        title: sessions.title,
        userId: sessions.userId,
        teamId: sessions.teamId,
        teamSlug: teams.slug,
        deviceId: sessions.deviceId,
        cli: sessions.cli,
        account: sessions.account,
      })
      .from(sessions)
      .innerJoin(teams, eq(teams.id, sessions.teamId))
      .where(eq(sessions.id, sessionId))
      .limit(1)
    if (!session) return

    // EXP-1005 throttle: at most ONE wall notification per PROFILE (device ×
    // agent × account) per hour — parallel sessions on one login hit the
    // same wall together, and one row says it all.
    const since = new Date(Date.now() - SESSION_BLOCKED_THROTTLE_MS)
    const recent = await db
      .select({
        createdAt: notifications.createdAt,
        deviceId: sessions.deviceId,
        cli: sessions.cli,
        account: sessions.account,
      })
      .from(notifications)
      .innerJoin(sessions, eq(notifications.sessionId, sessions.id))
      .where(
        and(
          eq(notifications.userId, session.userId),
          eq(notifications.type, `session_blocked`),
          gte(notifications.createdAt, since)
        )
      )
    const profile = blockedProfileKey(session)
    const throttled = recent.some(
      (row) =>
        new Date(row.createdAt).getTime() >= since.getTime() &&
        blockedProfileKey(row) === profile
    )
    if (throttled) return

    const name = await deriveSessionTitle(session)
    const rateLimited = (blocked.kind ?? `rate_limit`) === `rate_limit`
    await deliverToTeam({
      teamId: session.teamId,
      recipientIds: [session.userId],
      type: `session_blocked`,
      title: rateLimited ? `${name} hit a rate limit` : `${name} is blocked`,
      // The countdown is as of NOW; the row's own timestamp dates it.
      body: blockedBadgeLabel(blocked, new Date()),
      pushData: {
        sessionId,
        teamId: session.teamId,
        teamSlug: session.teamSlug,
      },
      sessionId,
    })
  } catch (err) {
    console.error(`[notify] session_blocked failed:`, err)
  }
}

/**
 * #EXP-1439: a member's Assistant asks them for a DECISION (a question or
 * plan card in its conversation): the one moment its chat pushes. One
 * issue-less `agent_message` row routed to the conversation, push-first,
 * muted per type by the usual prefs. Best-effort, never throws.
 */
export async function notifyAssistantDecision(sessionId: string, text: string | null | undefined): Promise<void> {
  try {
    const [session] = await db
      .select({ userId: sessions.userId, teamId: sessions.teamId, teamSlug: teams.slug })
      .from(sessions)
      .innerJoin(teams, eq(teams.id, sessions.teamId))
      .where(eq(sessions.id, sessionId))
      .limit(1)
    if (!session) return
    // EXP-1458: a sealed card's text is never read (generic copy).
    const body =
      typeof text === `string` && !isSealedBody(text)
        ? text.trim().split(`\n`)[0]!.slice(0, 200)
        : SEALED_MESSAGE_NOTIFY_BODY
    await deliverToTeam({
      teamId: session.teamId,
      recipientIds: [session.userId],
      type: `agent_message`,
      title: ASSISTANT_COPY.decisionPush,
      body: body || null,
      pushData: { sessionId, teamId: session.teamId, teamSlug: session.teamSlug },
      sessionId,
    })
  } catch (err) {
    console.error(`[notify] assistant decision failed:`, err)
  }
}

/**
 * FEED-154: another account withdrew a member's share of their machine (a
 * team owner's `devices.dropShare`, an instance admin's drop, or the
 * machine's key holder proving a squat, an instance admin's key reset):
 * the device OWNER is told instead
 * of the share vanishing silently. One issue-less, team-scoped row
 * (`agent_message`, the inbox's generic person-readable row; it opens
 * nothing), push-first like every row. A self-unshare tells nobody. Best
 * effort, never throws.
 */
export async function notifyDeviceShareRemoved(args: {
  ownerId: string
  actorId: string
  teamId: string
  deviceLabel: string
  /** How many hosted sessions the withdrawal ended. */
  endedCount: number
  /** `key`: the machine's key holder proved the share was not the box's.
   *  `admin`: an instance admin reset the machine's key (no name: the admin
   *  is no teammate). */
  cause: `dropped` | `key` | `admin`
}): Promise<void> {
  if (args.ownerId === args.actorId) return
  try {
    const [team] = await db
      .select({ slug: teams.slug, name: teams.name })
      .from(teams)
      .where(eq(teams.id, args.teamId))
      .limit(1)
    if (!team) return
    const title =
      args.cause === `key`
        ? `Your share of ${args.deviceLabel} with ${team.name} was removed: another account proved it holds this machine`
        : args.cause === `admin`
          ? `An instance admin reset the key of ${args.deviceLabel}, which removed your share with ${team.name}`
          : `${await actorName(args.actorId)} removed your share of ${args.deviceLabel} with ${team.name}`
    const body =
      args.endedCount === 0
        ? null
        : args.endedCount === 1
          ? `1 session running on it for a teammate ended.`
          : `${args.endedCount} sessions running on it for teammates ended.`
    await deliverToTeam({
      teamId: args.teamId,
      recipientIds: [args.ownerId],
      type: `agent_message`,
      title,
      body,
      pushData: { teamId: args.teamId, teamSlug: team.slug },
    })
  } catch (err) {
    console.error(`[notify] device share removed failed:`, err)
  }
}

/** EXP-1458: a SEALED message's push / inbox body: generic copy, never a
 *  preview and never the envelope. */
export const SEALED_MESSAGE_NOTIFY_BODY = `New message in a session`

/** EXP-1458: the reply path's re-notify check over `mention_user_ids`
 *  (a sealed body has no `@email` to diff, `addsMention`'s twin): `after`
 *  names someone `before` did not (null `before` = a new message). */
export function addsMentionUserIds(
  before: readonly string[] | null | undefined,
  after: readonly string[] | null | undefined
): boolean {
  if (!after?.length) return false
  if (before === null || before === undefined) return true
  const had = new Set(before)
  return after.some((id) => !had.has(id))
}

/**
 * EXP-1291: `@email` mentions inside a Conversation message (a person's or
 * the agent's) notify the mentioned members who can SEE the session (a
 * private session's only reader is its starter). One issue-less row carrying
 * `session_id` + `team_id`, typed `agent_message` (the inbox's
 * session-routed type); an agent author's mentions honour the recipients'
 * `allow_agent_messages` block like every agent message.
 * EXP-1458: a SEALED message is never read: its recipients are its
 * `mention_user_ids` (the writer's) and the body is generic copy; a
 * plaintext one parses its `@email`s, unioned with `mention_user_ids`.
 */
export function fireAndForgetSessionMessageNotify(args: { messageId: string }): void {
  void notifySessionMessage(args.messageId).catch((err) => {
    console.error(`[notify] session mention failed:`, err)
  })
}

/** The awaitable body of `fireAndForgetSessionMessageNotify`. */
export async function notifySessionMessage(messageId: string): Promise<void> {
  const [row] = await db
    .select({
      body: messages.body,
      card: messages.card,
      mentionUserIds: messages.mentionUserIds,
      authorKind: messages.authorKind,
      authorId: messages.authorId,
      sessionId: sessions.id,
      rootId: sessions.rootId,
      title: sessions.title,
      teamId: sessions.teamId,
      teamSlug: teams.slug,
      shared: sessions.shared,
      userId: sessions.userId,
    })
    .from(messages)
    .innerJoin(conversations, eq(conversations.id, messages.conversationId))
    .innerJoin(sessions, eq(sessions.id, conversations.sessionId))
    .innerJoin(teams, eq(teams.id, sessions.teamId))
    .where(eq(messages.id, messageId))
    .limit(1)
  if (!row || row.authorKind === `system`) return
  const sealed = isSealedMessage(row)
  const listed = row.mentionUserIds ?? []
  if (listed.length === 0 && (sealed || !row.body.includes(`@`))) return

  const parsed = sealed ? [] : await db.transaction((tx) => resolveMentions(tx, row.body, row.teamId))
  // Listed ids pass the same membership guard (deliverToTeam) as parsed ones.
  const mentioned = [...new Set([...parsed, ...listed])]
  const actor = row.authorKind === `user` ? row.authorId : row.userId
  let recipients = mentioned.filter((id) => id !== row.authorId && (row.shared || id === row.userId))
  if (row.authorKind === `agent`) {
    const blocked = await getAgentMessageBlocklist(recipients.filter((id) => id !== row.userId))
    recipients = recipients.filter((id) => !blocked.has(id))
  }
  if (recipients.length === 0) return

  const who = actor ? await actorName(actor) : `Someone`
  const thread = await deriveSessionTitle({
    id: row.sessionId,
    rootId: row.rootId,
    title: row.title,
  })
  const source = row.body.trim()
  await deliverToTeam({
    teamId: row.teamId,
    recipientIds: recipients,
    type: `agent_message`,
    title:
      row.authorKind === `agent`
        ? `${who}'s agent mentioned you in ${thread}`
        : `${who} mentioned you in ${thread}`,
    body: sealed ? SEALED_MESSAGE_NOTIFY_BODY : source.length > 140 ? `${source.slice(0, 139)}…` : source,
    pushData: { sessionId: row.sessionId, teamId: row.teamId, teamSlug: row.teamSlug },
    sessionId: row.sessionId,
  })
}

/**
 * SLOP-4: the widget reporter of an issue answered through their magic-link
 * page (a comment with source `reporter`, audience `reporter`). ISSUE-scoped
 * like `issue_comment` — subscribers + the assignee (else every member) get a `reporter_reply`
 * row + push routed to the issue; there is no actor to exclude. The preview
 * is reporter-authored UNTRUSTED text, server-escaped GFM: it is written as
 * a plain string and the digest email escapes bodies, so no extra sanitizing
 * is needed here beyond truncation.
 */
export function fireAndForgetReporterReplyNotify(args: {
  issueId: string
  commentId: string
}): void {
  void (async () => {
    try {
      const issue = await loadIssueMeta(args.issueId)
      if (!issue) return

      const [comment] = await db
        .select({ body: comments.body })
        .from(comments)
        .where(eq(comments.id, args.commentId))
        .limit(1)
      if (!comment) return

      const [submission] = await db
        .select({ reporterName: widgetSubmissions.reporterName })
        .from(widgetSubmissions)
        .where(eq(widgetSubmissions.issueId, args.issueId))
        .limit(1)

      const recipients = new Set(
        await subscriberRecipients(args.issueId, EMPTY_EXCLUSION)
      )
      if (issue.assigneeId) recipients.add(issue.assigneeId)
      // Nobody follows the issue yet (a fresh unassigned widget issue, or a
      // migrated helpdesk thread): fall back to every team member, the same
      // set a new widget issue reaches, so the reply never lands in a void.
      if (recipients.size === 0) {
        const memberRows = await db
          .select({ userId: teamMembers.userId })
          .from(teamMembers)
          .where(eq(teamMembers.teamId, issue.teamId))
        for (const row of memberRows) recipients.add(row.userId)
      }
      if (recipients.size === 0) return

      const previewSource = comment.body.trim()
      const preview =
        previewSource.length > 140
          ? `${previewSource.slice(0, 139)}…`
          : previewSource
      const who = submission?.reporterName?.trim() || `Anonymous visitor`
      await deliver({
        issue,
        recipientIds: [...recipients],
        type: `reporter_reply`,
        pushType: `reporter_reply`,
        title: `${who} replied on ${issue.identifier}`,
        body: preview || issue.title,
      })
    } catch (err) {
      console.error(`[notify] reporter reply failed:`, err)
    }
  })()
}

/**
 * One-way resolution notice (§6.4): when a widget-reported issue is closed
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
              // FEED-159: the title is member-editable text the reporter
              // receives: its `@<email>` mentions read "@Support" like every
              // other reporter egress (reporterFacingText).
              issueTitle: reporterFacingText(issueTitle, email),
            })
            await db.insert(emailDeliveries).values({
              userId: null,
              toEmail: email,
              issueId,
              kind: `widget_resolution`,
              status: deliveryStatus(result),
              provider: result.provider,
              providerMessageId: result.messageId,
              subject: result.subject,
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
