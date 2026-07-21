// Push-first hourly email digest (item q). Push fires immediately when a
// notification is created (deliver() in integrations/notifications.ts); email
// is the catch-up channel: this sweep finds notifications still UNREAD ~1h
// after creation that were never emailed, bundles them into ONE digest email
// per user, and stamps notifications.emailed_at. The pure gating/grouping
// core lives in notification-email-policy.ts (planEmailDigest — unit-tested);
// this module is the DB + transport shell plus the in-process scheduler
// started from server-bun.ts.

import { and, eq, gt, inArray, isNull, lte, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  emailDeliveries,
  issues,
  notifications,
  boards,
  users,
  teams,
} from "@/db/schema"
import {
  emailEnabled,
  sendNotificationDigestEmail,
  type DigestEmailItem,
} from "@/lib/email"
import { getEmailPrefsMap } from "@/lib/notification-prefs"
import {
  DIGEST_MAX_AGE_MS,
  DIGEST_MIN_UNREAD_AGE_MS,
  appBaseUrl,
  buildIssueDeepLinkPath,
  buildUnsubscribeUrl,
  planEmailDigest,
  type EmailPrefsLike,
} from "@/lib/notification-email-policy"

// Sweep cadence. Every sweep re-evaluates the pending set; the per-user
// cadence gate inside planEmailDigest keeps actual emails at most ~hourly
// (or ~daily), so a tighter sweep interval only reduces how far past the 1h
// unread window delivery lands.
const SWEEP_INTERVAL_MS = 10 * 60 * 1000
const INITIAL_DELAY_MS = 60 * 1000
// Per-sweep scan cap — a safety valve against pathological backlogs. Rows
// beyond the cap stay pending and are picked up by the next sweep (oldest
// first, so nothing starves).
const SCAN_LIMIT = 2000

// One sweep pass, injectable clock for tests/manual runs. Returns counts for
// the caller's logging. Never throws for per-recipient failures; a thrown
// error here means the scan/claim itself failed.
export async function runEmailDigestSweep(
  now: Date = new Date()
): Promise<{ emailsSent: number; notificationsClaimed: number }> {
  // No transport → leave rows unclaimed, exactly like the reporter-resolution
  // guard: configuring email later still digests anything inside the 24h
  // backstop window, and older rows age out naturally.
  if (!emailEnabled) return { emailsSent: 0, notificationsClaimed: 0 }

  const minAgeCutoff = new Date(now.getTime() - DIGEST_MIN_UNREAD_AGE_MS)
  const maxAgeFloor = new Date(now.getTime() - DIGEST_MAX_AGE_MS)

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
      isAgent: users.isAgent,
      issueIdentifier: issues.identifier,
      teamSlug: teams.slug,
      boardSlug: boards.slug,
    })
    .from(notifications)
    .innerJoin(users, eq(users.id, notifications.userId))
    .leftJoin(issues, eq(issues.id, notifications.issueId))
    .leftJoin(boards, eq(boards.id, issues.boardId))
    .leftJoin(teams, eq(teams.id, boards.teamId))
    .where(
      and(
        isNull(notifications.readAt),
        isNull(notifications.emailedAt),
        lte(notifications.createdAt, minAgeCutoff),
        gt(notifications.createdAt, maxAgeFloor)
      )
    )
    .orderBy(notifications.createdAt)
    .limit(SCAN_LIMIT)
  if (rows.length === 0) return { emailsSent: 0, notificationsClaimed: 0 }

  // Bot/addressless/unverified recipients can never be emailed — claim their
  // rows outright so they don't rescan forever (deliver() filters bots, so
  // these are stragglers at most). Unverified addresses are excluded because
  // digest content must never go to an address the account holder hasn't
  // proven they own.
  const unmailable = rows.filter(
    (row) => row.isAgent || !row.email || !row.emailVerified
  )
  const candidates = rows.filter(
    (row) => !row.isAgent && row.email && row.emailVerified
  )

  const userIds = [...new Set(candidates.map((row) => row.userId))]
  const prefs = await getEmailPrefsMap(userIds)
  const prefsByUser = new Map<string, EmailPrefsLike | null>(
    [...prefs.entries()].map(([userId, p]) => [userId, p])
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

  await Promise.all(
    plan.batches.map(async (batch) => {
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
              status: result.delivered ? `sent` : `failed`,
              provider: result.provider,
              providerMessageId: result.messageId,
              sentAt: result.delivered ? now : null,
              error: result.delivered ? null : `no email transport configured`,
            })
            .where(eq(emailDeliveries.id, ledger.id))
          if (result.delivered) emailsSent += 1
        } catch (sendErr) {
          await db
            .update(emailDeliveries)
            .set({ status: `failed`, error: String(sendErr).slice(0, 1000) })
            .where(eq(emailDeliveries.id, ledger.id))
          throw sendErr
        }
      } catch (err) {
        console.error(`[digest] email to ${to} failed:`, err)
        // Un-claim this batch so a later sweep retries — a transient
        // transport error must not permanently swallow the digest. The failed
        // ledger row above feeds the failure backoff, so the retry waits a
        // full day rather than firing at every sweep. Rows read in the
        // meantime stay claimed (the push did its job late).
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
    })
  )

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
