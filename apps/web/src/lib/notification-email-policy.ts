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

// Local hour the daily digest goes out at when the user never picked one.
export const DEFAULT_DIGEST_HOUR = 8

// The prefs shape the policy functions consume. A missing row (`null` /
// `undefined`) means ALL defaults: email on, every type on, daily digest at
// DEFAULT_DIGEST_HOUR.
export interface EmailPrefsLike {
  emailEnabled: boolean
  // Per-type opt-outs; a type absent from the map defaults to ON.
  typePrefs: Partial<Record<NotificationType, boolean>>
  digest: string
  // Local send hour for the `daily` cadence (0–23). Ignored by `off`.
  digestHour: number
}

export type TypePrefsLike = Partial<Record<NotificationType, boolean>>

export function defaultEmailPrefs(): EmailPrefsLike {
  return {
    emailEnabled: true,
    typePrefs: {},
    digest: `daily`,
    digestHour: DEFAULT_DIGEST_HOUR,
  }
}

// Is this notification type allowed to leave the app at all? CHANNEL-AGNOSTIC
// (EXP-369): the per-type toggles gate push AND email; the master
// `emailEnabled` switch is NOT consulted here — it only silences the digest.
// The in-app inbox row is always written regardless.
export function notificationTypeAllowed(
  typePrefs: TypePrefsLike | null | undefined,
  type: NotificationType
): boolean {
  return typePrefs?.[type] !== false
}

// Clamp a stored/incoming digest hour to a full hour in 0–23; anything else
// (null, NaN, 24, 7.5) resolves to the default rather than throwing — the
// sweep must never die on one bad row.
export function normalizeDigestHour(hour: number | null | undefined): number {
  if (typeof hour !== `number` || !Number.isInteger(hour)) {
    return DEFAULT_DIGEST_HOUR
  }
  if (hour < 0 || hour > 23) return DEFAULT_DIGEST_HOUR
  return hour
}

// ---------------------------------------------------------------------------
// Push-first email digest (item q)
// ---------------------------------------------------------------------------
//
// Push fires immediately on notification create; email never does. A sweep
// bundles notifications that are still UNREAD into ONE digest email per user
// — reading them in time (the push did its job) means no email at all, which
// keeps transactional volume low. `daily` (the default) fires at the user's
// chosen local hour and bundles everything created at or before that send
// point and still unread — rows created after it ride the next day's
// (EXP-399); `off` is the legacy hourly cadence, which keeps the old "unread
// an hour after the push" floor.

// How long a notification must have stayed unread before it qualifies, BY
// CADENCE. Daily has no age floor (EXP-369) — its delay is the send hour
// itself: a row qualifies only once a send point has passed since its
// creation (EXP-399, enforced per-row in planEmailDigest), so a row created
// five minutes before the send hour belongs in today's digest and one created
// five minutes after waits for tomorrow's.
export const DIGEST_MIN_UNREAD_AGE_MS: Record<DigestCadence, number> = {
  off: 60 * 60 * 1000,
  daily: 0,
}
// Backstop floor: rows older than this are never digested. Protects the first
// deploy from emailing months of pre-existing unread backlog. Daily needs
// 48h, not 24h: a row created just after today's send point waits ~24h for
// the next one, and the 22h failure-retry backoff can push that further —
// a 24h backstop would silently drop exactly those rows.
export const DIGEST_MAX_AGE_MS: Record<DigestCadence, number> = {
  off: 24 * 60 * 60 * 1000,
  daily: 48 * 60 * 60 * 1000,
}
// The SQL scan floor — the widest of the per-cadence backstops (cadence is
// only known once prefs are joined in, so the scan takes the superset).
export const DIGEST_SCAN_MAX_AGE_MS = 48 * 60 * 60 * 1000
// Per-user minimum gap between HOURLY (`off`) digest emails. Sits a bit under
// an hour so a sweep firing slightly early never skips a user for a whole
// extra cycle. `daily` has no gap rule — its send point is absolute (see
// lastDailySendPoint).
export const DIGEST_HOURLY_MIN_GAP_MS = 50 * 60 * 1000
// Minimum gap after a FAILED send attempt. The success-based cadence gate
// above never sees failed sends (no sent_at), so without this a failing
// transport — e.g. SES still sandboxed (EXP-114) — would retry at every
// sweep tick (~10min) forever (EXP-227). Failures back off to at most ONE
// retry per day regardless of the user's cadence: a broken transport is an
// ops problem, and hammering it burns send reputation for nothing.
export const DIGEST_FAILED_RETRY_GAP_MS = 22 * 60 * 60 * 1000

// Only an explicit `off` opts into the hourly cadence — a missing row or an
// unrecognised value resolves to the `daily` default. Takes the structural
// subset it actually reads so callers holding a partial prefs row (the
// settings routers) can share this ONE rule instead of re-deriving it.
export function digestCadence(
  prefs: { digest?: string } | null | undefined
): DigestCadence {
  return prefs?.digest === `off` ? `off` : `daily`
}

// ---------------------------------------------------------------------------
// Timezone math (no dependency — plain Intl)
// ---------------------------------------------------------------------------
//
// The daily digest fires at a LOCAL hour, so the sweep has to invert
// "local wall clock → UTC instant" for every user's zone. Intl is the only
// tz database in the runtime; everything below is built on formatToParts.
// Every entry point tolerates a null/unknown/garbage zone by falling back to
// UTC — a bad `users.timezone` value must never take the sweep down.

const MAX_CACHED_FORMATTERS = 512
// `null` memoizes a zone Intl rejected, so a garbage value costs one throw.
const formatterCache = new Map<string, Intl.DateTimeFormat | null>()

function formatterFor(timezone: string): Intl.DateTimeFormat | null {
  const cached = formatterCache.get(timezone)
  if (cached !== undefined) return cached
  let formatter: Intl.DateTimeFormat | null = null
  try {
    formatter = new Intl.DateTimeFormat(`en-US`, {
      timeZone: timezone,
      hourCycle: `h23`,
      year: `numeric`,
      month: `2-digit`,
      day: `2-digit`,
      hour: `2-digit`,
      minute: `2-digit`,
      second: `2-digit`,
    })
  } catch {
    // RangeError: unknown IANA name.
    formatter = null
  }
  if (formatterCache.size >= MAX_CACHED_FORMATTERS) formatterCache.clear()
  formatterCache.set(timezone, formatter)
  return formatter
}

interface ZonedParts {
  year: number
  month: number // 1-12
  day: number
  hour: number
  minute: number
  second: number
}

// The wall clock in `timezone` at `date`. Unknown zone → UTC.
function zonedParts(
  timezone: string | null | undefined,
  date: Date
): ZonedParts {
  const formatter = timezone ? formatterFor(timezone) : null
  if (!formatter) {
    return {
      year: date.getUTCFullYear(),
      month: date.getUTCMonth() + 1,
      day: date.getUTCDate(),
      hour: date.getUTCHours(),
      minute: date.getUTCMinutes(),
      second: date.getUTCSeconds(),
    }
  }
  const parts: Record<string, string> = {}
  for (const part of formatter.formatToParts(date)) {
    if (part.type !== `literal`) parts[part.type] = part.value
  }
  return {
    year: Number(parts.year),
    month: Number(parts.month),
    day: Number(parts.day),
    // h23 still emits "24" for midnight in some ICU versions.
    hour: Number(parts.hour) % 24,
    minute: Number(parts.minute),
    second: Number(parts.second),
  }
}

// Offset of `timezone` from UTC at the instant `date`, in ms (east positive:
// Europe/Berlin in summer → +7200000). Unknown/null zone → 0 (UTC).
export function tzOffsetMs(
  timezone: string | null | undefined,
  date: Date
): number {
  if (!timezone) return 0
  if (!formatterFor(timezone)) return 0
  const p = zonedParts(timezone, date)
  const asUtc = Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute, p.second)
  // formatToParts truncates to whole seconds — compare against the same.
  return asUtc - Math.floor(date.getTime() / 1000) * 1000
}

// The UTC instant of `hour:00:00` local time on the given local calendar date.
// Two-pass offset inversion (the offset depends on the answer), then a
// validity check for the two DST anomalies:
//   spring-forward gap — the requested local hour never happens; we return the
//     first instant AFTER the gap, so a 02:00 digest still goes out that day.
//   fall-back overlap — the hour happens twice; we deterministically return
//     the FIRST occurrence (the later one would delay the digest an hour).
export function zonedHourToUtc(
  timezone: string | null | undefined,
  year: number,
  month: number,
  day: number,
  hour: number
): Date {
  const naive = Date.UTC(year, month - 1, day, hour, 0, 0)
  if (!timezone || !formatterFor(timezone)) return new Date(naive)
  const first = naive - tzOffsetMs(timezone, new Date(naive))
  const second = naive - tzOffsetMs(timezone, new Date(first))
  if (zonedParts(timezone, new Date(second)).hour === hour) {
    return new Date(second)
  }
  // DST gap: neither pass lands on the requested wall clock. The later
  // candidate is the instant the clock jumped TO.
  return new Date(Math.max(first, second))
}

// The most recent daily send point at or before `now` — the boundary the
// daily cadence gate compares the last successful send against.
export function lastDailySendPoint(
  timezone: string | null | undefined,
  digestHour: number | null | undefined,
  now: Date
): Date {
  const hour = normalizeDigestHour(digestHour)
  const today = zonedParts(timezone, now)
  const point = zonedHourToUtc(
    timezone,
    today.year,
    today.month,
    today.day,
    hour
  )
  if (point.getTime() <= now.getTime()) return point
  // Today's send point is still ahead — the last one was yesterday's. Plain
  // UTC date arithmetic on the LOCAL calendar date (no zone involved yet).
  const prev = new Date(
    Date.UTC(today.year, today.month - 1, today.day) - 24 * 60 * 60 * 1000
  )
  return zonedHourToUtc(
    timezone,
    prev.getUTCFullYear(),
    prev.getUTCMonth() + 1,
    prev.getUTCDate(),
    hour
  )
}

// The next daily send point strictly AFTER `now` — the instant the user's
// next digest is scheduled for. Nothing in the sweep consumes this; it exists
// so the settings panels can show the schedule the sweep will actually use
// (EXP-452). Deriving it from lastDailySendPoint (rather than re-deriving the
// hour math) is deliberate: a panel that computed its own send point could
// disagree with the sweep, which is precisely the failure this surfaces.
export function nextDailySendPoint(
  timezone: string | null | undefined,
  digestHour: number | null | undefined,
  now: Date
): Date {
  const last = lastDailySendPoint(timezone, digestHour, now)
  // Walk forward one LOCAL day from the last point. Adding 24h to the instant
  // would drift across a DST boundary (23h/25h days); re-resolving the hour on
  // the next local calendar date keeps it at the chosen wall clock.
  const parts = zonedParts(timezone, last)
  const nextDay = new Date(
    Date.UTC(parts.year, parts.month - 1, parts.day) + 24 * 60 * 60 * 1000
  )
  return zonedHourToUtc(
    timezone,
    nextDay.getUTCFullYear(),
    nextDay.getUTCMonth() + 1,
    nextDay.getUTCDate(),
    normalizeDigestHour(digestHour)
  )
}

// Is this user due a digest email now, given when their last one was sent?
// The daily arm gates only on the LAST SEND vs the send point — "no send
// point has passed since a row was created" is the per-row filter in
// planEmailDigest (EXP-399), which is also what keeps the never-sent
// `!lastDigestSentAt → true` arm from firing instantly. Accepted quirks of
// the send-hour model (deliberate, not bugs): a notification created a
// minute before the send hour gets push and email near-simultaneously;
// moving digestHour later in the same day can produce a second digest that
// day; and a failed-send retry lands at whatever hour the backoff expires
// rather than the chosen one.
export function isDigestDue(
  prefs: EmailPrefsLike | null | undefined,
  timezone: string | null | undefined,
  lastDigestSentAt: Date | null | undefined,
  now: Date
): boolean {
  if (!lastDigestSentAt) return true
  if (digestCadence(prefs) === `off`) {
    return (
      now.getTime() - lastDigestSentAt.getTime() >= DIGEST_HOURLY_MIN_GAP_MS
    )
  }
  const sendPoint = lastDailySendPoint(timezone, prefs?.digestHour, now)
  return lastDigestSentAt.getTime() < sendPoint.getTime()
}

// Failure-backoff gate: is a digest attempt allowed now, given the user's
// last successful send and last FAILED attempt? A failure with no success
// since defers the user a full day; once a send succeeds again, only the
// success-based cadence gate governs.
export function isDigestRetryDue(
  lastSentAt: Date | null | undefined,
  lastFailedAt: Date | null | undefined,
  now: Date
): boolean {
  if (!lastFailedAt) return true
  // Recovered: a send succeeded after the failure — no backoff needed.
  if (lastSentAt && lastSentAt.getTime() > lastFailedAt.getTime()) return true
  return now.getTime() - lastFailedAt.getTime() >= DIGEST_FAILED_RETRY_GAP_MS
}

// Per-row sendability gate applied to the sweep's scan BEFORE planning
// (REV2-14). A row may only produce email when the recipient address is
// present AND verified (digest content must never go to an address the
// account holder hasn't proven they own) AND the recipient still holds
// membership of the row's team — the sweep mirrors the notifications shape's
// scoping: teamMembers.remove leaves pending unread rows behind, the shape
// hides them from the ex-member (so they can never be read in-app), and
// emailing them would deliver issue titles + previews after access was
// revoked. Rows failing this gate are claimed WITHOUT sending, like the
// pref-opted-out claimOnly bucket, so they don't rescan forever.
export interface DigestRecipientLike {
  email: string | null
  emailVerified: boolean
  isMember: boolean
}

// Three outcomes, not two (REV2-52):
//   send  — mailable now
//   claim — permanently unmailable (no address, or access revoked): stamp
//           emailed_at without sending so the row stops rescanning forever
//   defer — unmailable ONLY because the address isn't verified yet. Leave the
//           row untouched, exactly like the no-transport guard: verifying
//           inside the 24h backstop still digests recent items, and anything
//           older ages out by itself. Claiming these was a silent permanent
//           drop for a state the user can fix in one click.
export type DigestSendability = `send` | `defer` | `claim`

export function digestSendability(row: DigestRecipientLike): DigestSendability {
  if (!row.email || !row.isMember) return `claim`
  if (!row.emailVerified) return `defer`
  return `send`
}

export function isDigestSendable(row: DigestRecipientLike): boolean {
  return digestSendability(row) === `send`
}

// Transient-vs-persistent send failure (REV2-39). The 22h
// DIGEST_FAILED_RETRY_GAP_MS backoff exists for a BROKEN transport (SES still
// sandboxed, bad credentials) — applying it to a throttle blip or a dropped
// connection costs the user their whole day's digest, and rows already ≥2h
// old at failure time age past the 24h backstop before the retry window
// opens, i.e. they are never emailed at all. Errors matching this stay
// eligible for the very next sweep.
export function isTransientSendError(error: unknown): boolean {
  const parts: string[] = []
  if (error && typeof error === `object`) {
    const e = error as Record<string, unknown>
    for (const key of [`name`, `code`, `message`]) {
      if (typeof e[key] === `string`) parts.push(e[key] as string)
    }
    const status =
      (e.$metadata as { httpStatusCode?: number } | undefined)
        ?.httpStatusCode ??
      (typeof e.statusCode === `number` ? e.statusCode : undefined)
    // 429 = throttled, 5xx = provider-side outage — both clear by themselves.
    if (status !== undefined && (status === 429 || status >= 500)) return true
    // SMTP reply codes 421/450/451/452 are explicitly "try again later"; any
    // other 4xx (550 aside) is a real rejection.
    if (
      typeof e.responseCode === `number` &&
      [421, 450, 451, 452].includes(e.responseCode)
    ) {
      return true
    }
  } else if (typeof error === `string`) {
    parts.push(error)
  }
  const haystack = parts.join(` `).toLowerCase()
  return [
    `throttl`,
    `too many requests`,
    `rate exceeded`,
    `maximum sending rate`,
    `timeout`,
    `timedout`,
    `etimedout`,
    `econnreset`,
    `econnrefused`,
    `epipe`,
    `eai_again`,
    `socket hang up`,
    `network`,
    `service unavailable`,
  ].some((needle) => haystack.includes(needle))
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
  // Stamp emailed_at WITHOUT sending: rows whose user turned the digest off
  // entirely (master switch) or opted out of that row's type — parity with
  // the old immediate-send world, where such rows never produced an email.
  claimOnly: T[]
}

// Pure grouping/gating core of the digest sweep. Rows returned in NEITHER
// bucket are deferred untouched (still too young, already read, outside the
// backstop window, or their user's cadence isn't due yet) — the next sweep
// reconsiders them.
export function planEmailDigest<T extends DigestCandidate>(args: {
  candidates: T[]
  prefsByUser: ReadonlyMap<string, EmailPrefsLike | null>
  // Last SUCCESSFUL digest per user — drives the hourly/daily cadence gate.
  lastDigestByUser: ReadonlyMap<string, Date | null>
  // Last FAILED digest attempt per user — drives the daily failure backoff
  // so a broken transport can't retry at every sweep (EXP-227).
  lastFailedByUser: ReadonlyMap<string, Date | null>
  // Each user's IANA zone (users.timezone) — the clock their daily send hour
  // is read in. A missing/null entry means UTC.
  timezoneByUser: ReadonlyMap<string, string | null>
  now: Date
}): DigestPlan<T> {
  const {
    candidates,
    prefsByUser,
    lastDigestByUser,
    lastFailedByUser,
    timezoneByUser,
    now,
  } = args

  const byUser = new Map<string, T[]>()
  for (const candidate of candidates) {
    // Read in time → the push worked, no email. The age window depends on the
    // recipient's cadence, so it is applied per-user below.
    if (candidate.readAt) continue
    const list = byUser.get(candidate.userId)
    if (list) list.push(candidate)
    else byUser.set(candidate.userId, [candidate])
  }

  const batches: { userId: string; items: T[] }[] = []
  const claimOnly: T[] = []
  const userIds = [...byUser.keys()].sort()
  for (const userId of userIds) {
    const prefs = prefsByUser.get(userId) ?? null
    const cadence = digestCadence(prefs)
    const minAge = DIGEST_MIN_UNREAD_AGE_MS[cadence]
    const maxAge = DIGEST_MAX_AGE_MS[cadence]
    // Daily rows ride the NEXT send point after their creation (EXP-399): a
    // row created after today's send point defers to tomorrow's. Without this
    // per-row gate the cadence check alone made any fresh notification email
    // instantly whenever the user's last digest predated today's send point —
    // the steady state, since digests only go out when something is pending
    // (and a never-sent user was due at every sweep).
    const sendPoint =
      cadence === `daily`
        ? lastDailySendPoint(
            timezoneByUser.get(userId) ?? null,
            prefs?.digestHour,
            now
          )
        : null
    const allowed: T[] = []
    for (const row of byUser.get(userId)!) {
      // Too fresh (hourly floor / no daily send point passed yet) → wait for
      // a later sweep. Past the backstop → never emailed. Both stay
      // UNCLAIMED (deferred), like before.
      const age = now.getTime() - row.createdAt.getTime()
      if (age < minAge || age > maxAge) continue
      if (sendPoint && row.createdAt.getTime() > sendPoint.getTime()) continue
      // The master switch no longer gates the per-type toggles (EXP-369) —
      // it silences the digest, so its rows are claimed here rather than
      // rescanned until they age out.
      if (
        prefs?.emailEnabled === false ||
        !notificationTypeAllowed(prefs?.typePrefs, row.type)
      ) {
        claimOnly.push(row)
        continue
      }
      allowed.push(row)
    }
    if (allowed.length === 0) continue
    // Cadence gate: not due yet → leave the rows unclaimed for a later sweep.
    if (
      !isDigestDue(
        prefs,
        timezoneByUser.get(userId) ?? null,
        lastDigestByUser.get(userId),
        now
      )
    )
      continue
    // Failure backoff: an unrecovered failed attempt defers this user a full
    // day — at most one retry per day, never one per sweep tick.
    if (
      !isDigestRetryDue(
        lastDigestByUser.get(userId),
        lastFailedByUser.get(userId),
        now
      )
    )
      continue
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

// The team's Support inbox — the link target for issue-less `support_reply`
// digest items (REV2-51). Those rows carry notifications.team_id precisely so
// clients can route them here; before this they rendered as unlinked text
// while the prefs page promised "deep links straight to each issue".
export function buildSupportDeepLinkPath(teamSlug: string): string {
  return `/t/${encodeURIComponent(teamSlug)}/support`
}
