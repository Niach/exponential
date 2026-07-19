// Pure email-notification policy — no DB, no transport. Home of the
// push-first digest planner (which notifications get bundled into which
// user's digest email) plus the per-type allow policy and the one-way
// helpdesk resolution guards. Unit-tested in notification-email-policy.test.ts.

import type { IssueStatus, NotificationType } from "@/lib/domain"

// Digest cadence. Email is push-first: NOTHING is emailed per-event anymore.
// `off` means the standard hourly digest of still-unread notifications;
// `daily` (the default) batches at most one digest email per day.
export const digestValues = [`off`, `daily`] as const
export type DigestCadence = (typeof digestValues)[number]

// The prefs shape the policy functions consume. A missing row (`null` /
// `undefined`) means ALL defaults: email on, every type on, daily digest.
export interface EmailPrefsLike {
  emailEnabled: boolean
  // Per-type opt-outs; a type absent from the map defaults to ON.
  typePrefs: Partial<Record<NotificationType, boolean>>
  digest: string
}

export function defaultEmailPrefs(): EmailPrefsLike {
  return { emailEnabled: true, typePrefs: {}, digest: `daily` }
}

// Is email allowed at all for this notification type (ignoring digest)?
// Missing row → defaults → allowed.
export function emailTypeAllowed(
  prefs: EmailPrefsLike | null | undefined,
  type: NotificationType
): boolean {
  if (!prefs) return true
  if (!prefs.emailEnabled) return false
  return prefs.typePrefs[type] !== false
}

// ---------------------------------------------------------------------------
// Push-first email digest (item q)
// ---------------------------------------------------------------------------
//
// Push fires immediately on notification create; email never does. An hourly
// sweep bundles every notification that is still UNREAD ~1h after creation
// into ONE digest email per user — reading the notification in time (the push
// did its job) means no email at all, which keeps transactional volume low.

// A notification only qualifies for email once it has stayed unread this long.
export const DIGEST_MIN_UNREAD_AGE_MS = 60 * 60 * 1000
// Backstop floor: rows older than this are never digested. Protects the first
// deploy from emailing months of pre-existing unread backlog, and bounds the
// sweep's scan window.
export const DIGEST_MAX_AGE_MS = 24 * 60 * 60 * 1000
// Per-user minimum gap between digest emails, by cadence. `off` (hourly) sits
// a bit under an hour so a sweep firing slightly early never skips a user for
// a whole extra cycle; `daily` sits under 24h for the same reason.
const DIGEST_MIN_GAP_MS: Record<DigestCadence, number> = {
  off: 50 * 60 * 1000,
  daily: 22 * 60 * 60 * 1000,
}

// Only an explicit `off` opts into the hourly cadence — a missing row or an
// unrecognised value resolves to the `daily` default.
export function digestCadence(
  prefs: EmailPrefsLike | null | undefined
): DigestCadence {
  return prefs?.digest === `off` ? `off` : `daily`
}

// Is this user due a digest email now, given when their last one was sent?
export function isDigestDue(
  prefs: EmailPrefsLike | null | undefined,
  lastDigestSentAt: Date | null | undefined,
  now: Date
): boolean {
  if (!lastDigestSentAt) return true
  return (
    now.getTime() - lastDigestSentAt.getTime() >=
    DIGEST_MIN_GAP_MS[digestCadence(prefs)]
  )
}

// The minimal row shape the planner needs. The DB runner passes richer rows
// (recipient address, issue deep-link data) through generically.
export interface DigestCandidate {
  notificationId: string
  userId: string
  type: NotificationType
  createdAt: Date
  readAt: Date | null
}

export interface DigestPlan<T extends DigestCandidate> {
  // One digest email per entry; items oldest-first, batches ordered by userId.
  batches: { userId: string; items: T[] }[]
  // Stamp emailed_at WITHOUT sending: rows whose user opted out of email
  // entirely or of that row's type — parity with the old immediate-send
  // world, where such rows simply never produced an email.
  claimOnly: T[]
}

// Pure grouping/gating core of the digest sweep. Rows returned in NEITHER
// bucket are deferred untouched (still too young, already read, outside the
// backstop window, or their user's cadence isn't due yet) — the next sweep
// reconsiders them.
export function planEmailDigest<T extends DigestCandidate>(args: {
  candidates: T[]
  prefsByUser: ReadonlyMap<string, EmailPrefsLike | null>
  lastDigestByUser: ReadonlyMap<string, Date | null>
  now: Date
}): DigestPlan<T> {
  const { candidates, prefsByUser, lastDigestByUser, now } = args

  const byUser = new Map<string, T[]>()
  for (const candidate of candidates) {
    // Read in time → the push worked, no email. Too fresh → wait for the next
    // sweep. Past the backstop → never emailed.
    if (candidate.readAt) continue
    const age = now.getTime() - candidate.createdAt.getTime()
    if (age < DIGEST_MIN_UNREAD_AGE_MS || age > DIGEST_MAX_AGE_MS) continue
    const list = byUser.get(candidate.userId)
    if (list) list.push(candidate)
    else byUser.set(candidate.userId, [candidate])
  }

  const batches: { userId: string; items: T[] }[] = []
  const claimOnly: T[] = []
  const userIds = [...byUser.keys()].sort()
  for (const userId of userIds) {
    const prefs = prefsByUser.get(userId) ?? null
    const allowed: T[] = []
    for (const row of byUser.get(userId)!) {
      if (emailTypeAllowed(prefs, row.type)) allowed.push(row)
      else claimOnly.push(row)
    }
    if (allowed.length === 0) continue
    // Cadence gate: not due yet → leave the rows unclaimed for a later sweep.
    if (!isDigestDue(prefs, lastDigestByUser.get(userId), now)) continue
    allowed.sort((a, b) => a.createdAt.getTime() - b.createdAt.getTime())
    batches.push({ userId, items: allowed })
  }
  return { batches, claimOnly }
}

// ---------------------------------------------------------------------------
// One-way helpdesk (widget reporter) resolution guards
// ---------------------------------------------------------------------------

// Statuses that count as "closed" for the reporter resolution email.
export function isResolutionStatus(status: string): status is IssueStatus {
  return status === `done` || status === `cancelled`
}

// Exactly-once semantics: send only on a close transition when the submission
// was never notified before. resolvedNotifiedAt is set-once and never cleared
// on reopen, so a reopen→re-close does NOT re-email.
export function shouldSendReporterResolution(args: {
  toStatus: string
  resolvedNotifiedAt: Date | null | undefined
}): boolean {
  if (!isResolutionStatus(args.toStatus)) return false
  return args.resolvedNotifiedAt == null
}

// ---------------------------------------------------------------------------
// URL builders
// ---------------------------------------------------------------------------

export function appBaseUrl(): string {
  return (process.env.BETTER_AUTH_URL ?? `http://localhost:5173`).replace(
    /\/$/,
    ``
  )
}

export function buildUnsubscribeUrl(baseUrl: string, token: string): string {
  return `${baseUrl.replace(/\/$/, ``)}/api/email/unsubscribe?token=${encodeURIComponent(token)}`
}

// The stable issue deep link shared by push (identifier) and email (URL):
// /t/{teamSlug}/boards/{boardSlug}/issues/{identifier}
export function buildIssueDeepLinkPath(args: {
  teamSlug: string
  boardSlug: string
  identifier: string
}): string {
  return `/t/${encodeURIComponent(args.teamSlug)}/boards/${encodeURIComponent(args.boardSlug)}/issues/${encodeURIComponent(args.identifier)}`
}
