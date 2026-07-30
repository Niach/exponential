// Push-first email digest (item q). Push fires immediately when a
// notification is created (deliver() in integrations/notifications.ts); email
// is the catch-up channel: this sweep finds notifications still UNREAD that
// were never emailed, bundles them into ONE digest email per user, and stamps
// notifications.emailed_at. WHEN a user's bundle goes out is the plan's job:
// `daily` fires at the user's local send hour (users.timezone +
// user_notification_prefs.digest_hour), the legacy `off` cadence hourly. The
// pure gating/grouping core lives in notification-email-policy.ts
// (planEmailDigest — unit-tested); this module is the DB + transport shell
// plus the in-process scheduler started from server-bun.ts.

import { and, eq, gt, inArray, isNull, or, sql } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { db } from "@/db/connection"
import {
  emailDeliveries,
  issues,
  notifications,
  boards,
  teamMembers,
  users,
  teams,
} from "@/db/schema"
import {
  deliveryStatus,
  sendNotificationDigestEmail,
  type DigestEmailItem,
} from "@/lib/email"
import { emailEnabled } from "@/lib/email-enabled"
import { getEmailPrefsMap } from "@/lib/notification-prefs"
import {
  DIGEST_SCAN_MAX_AGE_MS,
  appBaseUrl,
  buildIssueDeepLinkPath,
  buildSupportDeepLinkPath,
  buildUnsubscribeUrl,
  digestSendability,
  isTransientSendError,
  planEmailDigest,
  type EmailPrefsLike,
} from "@/lib/notification-email-policy"

// The row's OWN team (notifications.team_id), distinct from the team reached
// through its board — issue-less helpdesk rows only have the former.
const notificationTeams = alias(teams, `notification_teams`)

// Sweep cadence. Every sweep re-evaluates the pending set; the per-user
// cadence gate inside planEmailDigest keeps actual emails at most ~hourly
// (or one per local send hour), so a tighter sweep interval only reduces how
// far past a user's send point delivery lands. Send times are restricted to
// FULL HOURS precisely so a 10-minute sweep resolves them.
const SWEEP_INTERVAL_MS = 10 * 60 * 1000
const INITIAL_DELAY_MS = 60 * 1000
// Per-sweep scan cap — a safety valve against pathological backlogs. Rows
// beyond the cap stay pending and are picked up by the next sweep (oldest
// first, so nothing starves).
const SCAN_LIMIT = 2000
// How many digest emails go out at once (REV2-39). An unbounded Promise.all
// over up to SCAN_LIMIT per-user batches burst hundreds of concurrent
// SendEmail calls (SES production default is ~14/s) and hundreds of
// concurrent non-pooled SMTP connections, then charged every throttled
// recipient a full day of backoff. A sweep has a 10-minute budget before the
// next tick and the per-user cadence gates make ordering irrelevant, so a
// small pool is free.
const SEND_CONCURRENCY = 5

// One sweep pass, injectable clock for tests/manual runs. Returns counts for
// the caller's logging. Never throws for per-recipient failures; a thrown
// error here means the scan/claim itself failed.
export async function runEmailDigestSweep(
  now: Date = new Date()
): Promise<{ emailsSent: number; notificationsClaimed: number }> {
  // No transport → leave rows unclaimed, exactly like the reporter-resolution
  // guard: configuring email later still digests anything inside the
  // backstop window, and older rows age out naturally.
  if (!emailEnabled) return { emailsSent: 0, notificationsClaimed: 0 }

  // The SQL floor is the WIDEST per-cadence backstop — the per-user age window
  // (and the daily cadence's zero minimum age) is applied inside
  // planEmailDigest, where the recipient's prefs are known.
  const maxAgeFloor = new Date(now.getTime() - DIGEST_SCAN_MAX_AGE_MS)

  const rows = await db
    .select({
      notificationId: notifications.id,
      userId: notifications.userId,
      type: notifications.type,
      title: notifications.title,
      body: notifications.body,
      createdAt: notifications.createdAt,
      readAt: notifications.readAt,
      email: users.email,
      emailVerified: users.emailVerified,
      // The clock the daily send hour is read in; NULL → UTC.
      timezone: users.timezone,
      issueIdentifier: issues.identifier,
      teamSlug: teams.slug,
      boardSlug: boards.slug,
      // REV2-51: issue-less helpdesk rows carry their own team_id — the
      // column that exists so they can be routed to the team's Support
      // surface. Without it they rendered as unlinked text.
      notificationTeamSlug: notificationTeams.slug,
      // REV2-14: mirror of the notifications shape's membership scoping —
      // the recipient must still be a member of the row's team (boards.team_id
      // for issue-anchored rows, the app-written notifications.team_id for
      // issue-less helpdesk support_reply rows). A row with no team identity
      // at all passes, matching the shape's defensive "board_id IS NULL" arm.
      isMember: sql<boolean>`(coalesce(${boards.teamId}, ${notifications.teamId}) is null or exists (select 1 from ${teamMembers} where ${teamMembers.userId} = ${notifications.userId} and ${teamMembers.teamId} = coalesce(${boards.teamId}, ${notifications.teamId})))`,
    })
    .from(notifications)
    .innerJoin(users, eq(users.id, notifications.userId))
    .leftJoin(issues, eq(issues.id, notifications.issueId))
    // Join on the trigger-denormalized notifications.board_id — the exact
    // column the shape scopes on (issues.move re-points it, so it never
    // diverges from the issue's live board).
    .leftJoin(boards, eq(boards.id, notifications.boardId))
    .leftJoin(teams, eq(teams.id, boards.teamId))
    .leftJoin(notificationTeams, eq(notificationTeams.id, notifications.teamId))
    .where(
      and(
        isNull(notifications.readAt),
        isNull(notifications.emailedAt),
        gt(notifications.createdAt, maxAgeFloor),
        // Trashed-board rows are hidden in-app for the 48h trash window
        // (REV-109) — guaranteed still-unread — and their deep links dead-end;
        // digesting them would email exactly what the trash window hides.
        // Leave them UNCLAIMED (filtered from the scan, not claim-only): a
        // restore inside the backstop window lets them digest late, and a
        // purge cascade-deletes them.
        or(isNull(notifications.boardId), isNull(boards.deletedAt))
      )
    )
    .orderBy(notifications.createdAt)
    .limit(SCAN_LIMIT)
  if (rows.length === 0) return { emailsSent: 0, notificationsClaimed: 0 }

  // Addressless recipients can never be emailed, and rows whose recipient
  // lost team access since creation must not be (REV2-14 — the email-leg twin
  // of REV-8's create-time fan-out recheck: teamMembers.remove leaves pending
  // unread rows behind, and the shape hides them from the ex-member, so they
  // can never be marked read in-app). Claim those outright so they don't
  // rescan forever. An UNVERIFIED address is different (REV2-52): it is a
  // one-click-fixable user state, so those rows are left untouched to digest
  // late if the user verifies inside the backstop window.
  const unmailable = rows.filter((row) => digestSendability(row) === `claim`)
  const candidates = rows.filter((row) => digestSendability(row) === `send`)

  const userIds = [...new Set(candidates.map((row) => row.userId))]
  const prefs = await getEmailPrefsMap(userIds)
  const prefsByUser = new Map<string, EmailPrefsLike | null>(
    [...prefs.entries()].map(([userId, p]) => [userId, p])
  )
  // users.timezone rides the scan rows (the users join already exists).
  const timezoneByUser = new Map<string, string | null>(
    candidates.map((row) => [row.userId, row.timezone])
  )

  // Cadence gate input: when did each user last get a digest (sent_at, stamped
  // on success) — and when did their last FAILED attempt happen (ledger row
  // with no sent_at)? The failed aggregate feeds the daily failure backoff:
  // without it a failing transport (SES sandboxed, EXP-114) would retry at
  // every sweep tick, flooding the ledger with failed rows (EXP-227).
  const lastRows =
    userIds.length > 0
      ? await db
          .select({
            userId: emailDeliveries.userId,
            lastSentAt: sql<Date | string | null>`max(${emailDeliveries.sentAt})`,
            lastFailedAt: sql<
              Date | string | null
            >`max(${emailDeliveries.createdAt}) filter (where ${emailDeliveries.sentAt} is null)`,
          })
          .from(emailDeliveries)
          .where(
            and(
              eq(emailDeliveries.kind, `digest`),
              inArray(emailDeliveries.userId, userIds)
            )
          )
          .groupBy(emailDeliveries.userId)
      : []
  const lastDigestByUser = new Map<string, Date | null>(
    lastRows.map((row) => [
      row.userId as string,
      row.lastSentAt ? new Date(row.lastSentAt) : null,
    ])
  )
  const lastFailedByUser = new Map<string, Date | null>(
    lastRows.map((row) => [
      row.userId as string,
      row.lastFailedAt ? new Date(row.lastFailedAt) : null,
    ])
  )

  const plan = planEmailDigest({
    candidates,
    prefsByUser,
    lastDigestByUser,
    lastFailedByUser,
    timezoneByUser,
    now,
  })

  const idsToClaim = [
    ...unmailable.map((row) => row.notificationId),
    ...plan.claimOnly.map((row) => row.notificationId),
    ...plan.batches.flatMap((batch) =>
      batch.items.map((item) => item.notificationId)
    ),
  ]
  if (idsToClaim.length === 0) return { emailsSent: 0, notificationsClaimed: 0 }

  // Atomic claim: only rows still unread and unclaimed make it into an email.
  // Concurrent sweeps (e.g. two web instances) race here and get disjoint
  // sets, so a notification can never be emailed twice — and a row read since
  // the scan escapes the email entirely (the push did its job late).
  const claimed = await db
    .update(notifications)
    .set({ emailedAt: now })
    .where(
      and(
        inArray(notifications.id, idsToClaim),
        isNull(notifications.emailedAt),
        isNull(notifications.readAt)
      )
    )
    .returning({ id: notifications.id })
  const claimedIds = new Set(claimed.map((row) => row.id))

  const base = appBaseUrl()
  let emailsSent = 0

  const sendBatch = async (batch: (typeof plan.batches)[number]) => {
    const items = batch.items.filter((item) =>
      claimedIds.has(item.notificationId)
    )
    if (items.length === 0) return
    const recipientPrefs = prefs.get(batch.userId)
    if (!recipientPrefs) return // unreachable: getEmailPrefsMap mints rows
    const to = items[0].email

    try {
      // Ledger row per digest email (kind='digest', no notification_id —
      // one email covers many rows; per-notification idempotency is the
      // emailed_at claim above).
      const [ledger] = await db
        .insert(emailDeliveries)
        .values({
          userId: batch.userId,
          toEmail: to,
          kind: `digest`,
        })
        .returning({ id: emailDeliveries.id })

      try {
        const digestItems: DigestEmailItem[] = items.map((item) => ({
          title: item.title,
          body: item.body,
          url:
            item.teamSlug && item.boardSlug && item.issueIdentifier
              ? `${base}${buildIssueDeepLinkPath({
                  teamSlug: item.teamSlug,
                  boardSlug: item.boardSlug,
                  identifier: item.issueIdentifier,
                })}`
              : // Issue-less helpdesk rows link to the team's Support
                // inbox (REV2-51) — the surface the row's team_id exists
                // to route to.
                item.type === `support_reply` && item.notificationTeamSlug
                ? `${base}${buildSupportDeepLinkPath(item.notificationTeamSlug)}`
                : null,
        }))
        const result = await sendNotificationDigestEmail({
          to,
          items: digestItems,
          appUrl: base,
          unsubscribeUrl: buildUnsubscribeUrl(
            base,
            recipientPrefs.unsubscribeToken
          ),
        })
        await db
          .update(emailDeliveries)
          .set({
            status: deliveryStatus(result),
            provider: result.provider,
            providerMessageId: result.messageId,
            sentAt: result.delivered ? now : null,
            error: result.delivered
              ? null
              : result.suppressed
                ? `recipient suppressed (bounce/complaint on record)`
                : `no email transport configured`,
          })
          .where(eq(emailDeliveries.id, ledger.id))
        if (result.delivered) emailsSent += 1
      } catch (sendErr) {
        if (isTransientSendError(sendErr)) {
          // A throttle blip or dropped connection is NOT a broken
          // transport (REV2-39). Dropping the ledger row keeps it out of
          // the lastFailedAt aggregate, so this user stays eligible for the
          // next 10-minute sweep instead of losing a full day — which for
          // rows already ≥2h old meant never being emailed at all (their
          // retry window would land past the age backstop). Nothing was
          // sent, so no delivery record is lost.
          await db
            .delete(emailDeliveries)
            .where(eq(emailDeliveries.id, ledger.id))
        } else {
          await db
            .update(emailDeliveries)
            .set({ status: `failed`, error: String(sendErr).slice(0, 1000) })
            .where(eq(emailDeliveries.id, ledger.id))
        }
        throw sendErr
      }
    } catch (err) {
      console.error(`[digest] email to ${to} failed:`, err)
      // Un-claim this batch so a later sweep retries — a transient
      // transport error must not permanently swallow the digest. A
      // PERSISTENT failure also left a failed ledger row above, which feeds
      // the daily backoff so the retry waits a day rather than firing at
      // every sweep tick. Rows read in the meantime stay claimed (the push
      // did its job late).
      try {
        await db
          .update(notifications)
          .set({ emailedAt: null })
          .where(
            and(
              inArray(
                notifications.id,
                items.map((item) => item.notificationId)
              ),
              isNull(notifications.readAt)
            )
          )
      } catch (unclaimErr) {
        console.error(`[digest] un-claim failed:`, unclaimErr)
      }
    }
  }

  // Bounded worker pool over the batch queue (REV2-39) — never one in-flight
  // send per due user.
  let cursor = 0
  const workers = Array.from(
    { length: Math.min(SEND_CONCURRENCY, plan.batches.length) },
    async () => {
      while (cursor < plan.batches.length) {
        const batch = plan.batches[cursor++]
        await sendBatch(batch)
      }
    }
  )
  await Promise.all(workers)

  return { emailsSent, notificationsClaimed: claimedIds.size }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  try {
    const result = await runEmailDigestSweep()
    if (result.emailsSent > 0 || result.notificationsClaimed > 0) {
      console.log(
        `[digest] sent ${result.emailsSent} digest email(s), claimed ${result.notificationsClaimed} notification(s)`
      )
    }
  } catch (err) {
    console.error(`[digest] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process digest scheduler — call once at boot (server-bun.ts),
// mirroring bootstrapSelfHosted's interval pattern. Double-start-guarded
// within the process; across instances the atomic emailed_at claim keeps
// per-notification exactly-once (two replicas could at worst split one user's
// pending items across two emails in the same hour). Fire-and-forget.
export function startEmailDigestScheduler(): void {
  if (started) return
  started = true
  // No transport configured for this process' lifetime — nothing to send.
  if (!emailEnabled) return
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
