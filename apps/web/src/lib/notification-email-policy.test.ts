import { describe, expect, it } from "vitest"
import {
  buildIssueDeepLinkPath,
  buildUnsubscribeUrl,
  buildSupportDeepLinkPath,
  defaultEmailPrefs,
  digestSendability,
  isDigestDue,
  isDigestRetryDue,
  isDigestSendable,
  isTransientSendError,
  isResolutionStatus,
  lastDailySendPoint,
  normalizeDigestHour,
  notificationTypeAllowed,
  planEmailDigest,
  shouldSendReporterResolution,
  tzOffsetMs,
  zonedHourToUtc,
  type DigestCandidate,
  type EmailPrefsLike,
} from "@/lib/notification-email-policy"
import type { NotificationType } from "@/lib/domain"

describe(`notificationTypeAllowed`, () => {
  it(`defaults to allowed when no prefs row exists (missing row = defaults)`, () => {
    expect(notificationTypeAllowed(null, `issue_comment`)).toBe(true)
    expect(notificationTypeAllowed(undefined, `pr_merged`)).toBe(true)
  })

  it(`defaults every type to on with a fresh prefs row`, () => {
    const prefs = defaultEmailPrefs()
    for (const type of [
      `issue_assigned`,
      `issue_comment`,
      `issue_status_changed`,
      `issue_mention`,
      `pr_opened`,
      `pr_merged`,
      `support_reply`,
    ] as const) {
      expect(notificationTypeAllowed(prefs.typePrefs, type)).toBe(true)
    }
  })

  // EXP-369: the toggles are channel-agnostic now (they gate push too), so the
  // email master switch must NOT reach into them — it only silences the digest.
  it(`ignores the master emailEnabled switch`, () => {
    const prefs = { ...defaultEmailPrefs(), emailEnabled: false }
    expect(notificationTypeAllowed(prefs.typePrefs, `issue_assigned`)).toBe(
      true
    )
    expect(notificationTypeAllowed(prefs.typePrefs, `pr_merged`)).toBe(true)
  })

  it(`per-type opt-out blocks only that type`, () => {
    const typePrefs = { issue_comment: false as const }
    expect(notificationTypeAllowed(typePrefs, `issue_comment`)).toBe(false)
    expect(notificationTypeAllowed(typePrefs, `issue_mention`)).toBe(true)
    expect(notificationTypeAllowed(typePrefs, `pr_opened`)).toBe(true)
  })

  it(`an explicit true in typePrefs stays on`, () => {
    expect(
      notificationTypeAllowed(
        { issue_assigned: true as const },
        `issue_assigned`
      )
    ).toBe(true)
  })
})

describe(`normalizeDigestHour`, () => {
  it(`passes through every full hour`, () => {
    for (const hour of [0, 8, 13, 23]) {
      expect(normalizeDigestHour(hour)).toBe(hour)
    }
  })

  it(`falls back to 8 for out-of-range, fractional and missing values`, () => {
    expect(normalizeDigestHour(24)).toBe(8)
    expect(normalizeDigestHour(-1)).toBe(8)
    expect(normalizeDigestHour(7.5)).toBe(8)
    expect(normalizeDigestHour(Number.NaN)).toBe(8)
    expect(normalizeDigestHour(null)).toBe(8)
    expect(normalizeDigestHour(undefined)).toBe(8)
  })
})

// ---------------------------------------------------------------------------
// Push-first hourly digest (item q)
// ---------------------------------------------------------------------------

const NOW = new Date(`2026-07-07T12:00:00Z`)

function minutesAgo(minutes: number): Date {
  return new Date(NOW.getTime() - minutes * 60 * 1000)
}

function candidate(overrides: {
  id: string
  userId: string
  ageMinutes: number
  type?: NotificationType
  readAt?: Date | null
}): DigestCandidate {
  return {
    notificationId: overrides.id,
    userId: overrides.userId,
    type: overrides.type ?? `issue_comment`,
    createdAt: minutesAgo(overrides.ageMinutes),
    readAt: overrides.readAt ?? null,
  }
}

function plan(
  candidates: DigestCandidate[],
  opts?: {
    prefsByUser?: Map<string, EmailPrefsLike | null>
    lastDigestByUser?: Map<string, Date | null>
    lastFailedByUser?: Map<string, Date | null>
    timezoneByUser?: Map<string, string | null>
    now?: Date
  }
) {
  return planEmailDigest({
    candidates,
    prefsByUser: opts?.prefsByUser ?? new Map(),
    lastDigestByUser: opts?.lastDigestByUser ?? new Map(),
    lastFailedByUser: opts?.lastFailedByUser ?? new Map(),
    timezoneByUser: opts?.timezoneByUser ?? new Map(),
    now: opts?.now ?? NOW,
  })
}

describe(`isDigestSendable`, () => {
  const sendable = {
    email: `member@example.com`,
    emailVerified: true,
    isMember: true,
  }

  it(`allows a verified member address`, () => {
    expect(isDigestSendable(sendable)).toBe(true)
  })

  it(`blocks an addressless recipient`, () => {
    expect(isDigestSendable({ ...sendable, email: null })).toBe(false)
    expect(isDigestSendable({ ...sendable, email: `` })).toBe(false)
  })

  it(`blocks an unverified address`, () => {
    expect(isDigestSendable({ ...sendable, emailVerified: false })).toBe(false)
  })

  it(`blocks a recipient who lost team access (REV2-14: ex-members must not
      be digested content the shape hides from them)`, () => {
    expect(isDigestSendable({ ...sendable, isMember: false })).toBe(false)
  })

  // REV2-52: unverified is a one-click-fixable user state, not a permanent
  // one — claiming those rows silently dropped the digest forever.
  it(`defers an unverified address but claims the permanently unmailable`, () => {
    expect(digestSendability(sendable)).toBe(`send`)
    expect(digestSendability({ ...sendable, emailVerified: false })).toBe(
      `defer`
    )
    expect(digestSendability({ ...sendable, email: null })).toBe(`claim`)
    expect(digestSendability({ ...sendable, isMember: false })).toBe(`claim`)
    // No address AND unverified is still permanently unmailable.
    expect(
      digestSendability({ ...sendable, email: null, emailVerified: false })
    ).toBe(`claim`)
  })
})

// REV2-39: the 22h failure backoff is for a BROKEN transport. Charging it to
// a throttle blip costs the user a day of digests — and any row already ≥2h
// old ages past the 24h backstop before the retry window opens, so it is
// never emailed at all.
describe(`isTransientSendError`, () => {
  it(`treats throttling, timeouts and 5xx as retry-next-sweep`, () => {
    expect(
      isTransientSendError(
        Object.assign(new Error(`Maximum sending rate exceeded`), {
          name: `ThrottlingException`,
        })
      )
    ).toBe(true)
    expect(
      isTransientSendError(
        Object.assign(new Error(`too many requests`), {
          $metadata: { httpStatusCode: 429 },
        })
      )
    ).toBe(true)
    expect(
      isTransientSendError(
        Object.assign(new Error(`internal`), {
          $metadata: { httpStatusCode: 503 },
        })
      )
    ).toBe(true)
    expect(
      isTransientSendError(
        Object.assign(new Error(`connection reset`), { code: `ECONNRESET` })
      )
    ).toBe(true)
    // SMTP 421/45x are explicitly "try again later".
    expect(
      isTransientSendError(
        Object.assign(new Error(`greylisted`), { responseCode: 451 })
      )
    ).toBe(true)
  })

  it(`treats real rejections as persistent (they keep the daily backoff)`, () => {
    expect(
      isTransientSendError(
        Object.assign(new Error(`Email address is not verified`), {
          name: `MessageRejected`,
          $metadata: { httpStatusCode: 400 },
        })
      )
    ).toBe(false)
    expect(
      isTransientSendError(
        Object.assign(new Error(`no such user`), { responseCode: 550 })
      )
    ).toBe(false)
    expect(isTransientSendError(new Error(`credentials missing`))).toBe(false)
    expect(isTransientSendError(null)).toBe(false)
    expect(isTransientSendError(undefined)).toBe(false)
  })
})

// REV2-51: issue-less support_reply rows rendered as unlinked text while the
// prefs copy promised deep links.
describe(`buildSupportDeepLinkPath`, () => {
  it(`points at the team's Support inbox`, () => {
    expect(buildSupportDeepLinkPath(`acme`)).toBe(`/t/acme/support`)
  })

  it(`encodes the slug`, () => {
    expect(buildSupportDeepLinkPath(`a c/me`)).toBe(`/t/a%20c%2Fme/support`)
  })
})

// ---------------------------------------------------------------------------
// Timezone math for the daily send point (EXP-369)
// ---------------------------------------------------------------------------

describe(`tzOffsetMs`, () => {
  it(`reads the live offset, DST included`, () => {
    const summer = new Date(`2026-07-07T12:00:00Z`)
    const winter = new Date(`2026-01-15T12:00:00Z`)
    expect(tzOffsetMs(`Europe/Berlin`, summer)).toBe(2 * 60 * 60 * 1000) // CEST
    expect(tzOffsetMs(`Europe/Berlin`, winter)).toBe(60 * 60 * 1000) // CET
    expect(tzOffsetMs(`America/New_York`, summer)).toBe(-4 * 60 * 60 * 1000)
    expect(tzOffsetMs(`America/New_York`, winter)).toBe(-5 * 60 * 60 * 1000)
    expect(tzOffsetMs(`UTC`, summer)).toBe(0)
  })

  it(`falls back to UTC for a missing or unknown zone — never throws`, () => {
    const at = new Date(`2026-07-07T12:00:00Z`)
    expect(tzOffsetMs(null, at)).toBe(0)
    expect(tzOffsetMs(undefined, at)).toBe(0)
    expect(tzOffsetMs(``, at)).toBe(0)
    expect(tzOffsetMs(`Mars/Olympus_Mons`, at)).toBe(0)
  })
})

describe(`zonedHourToUtc`, () => {
  it(`inverts a local wall clock to the UTC instant`, () => {
    expect(zonedHourToUtc(`Europe/Berlin`, 2026, 7, 7, 8).toISOString()).toBe(
      `2026-07-07T06:00:00.000Z`
    )
    expect(zonedHourToUtc(`Europe/Berlin`, 2026, 1, 15, 8).toISOString()).toBe(
      `2026-01-15T07:00:00.000Z`
    )
    expect(zonedHourToUtc(`UTC`, 2026, 7, 7, 0).toISOString()).toBe(
      `2026-07-07T00:00:00.000Z`
    )
    expect(zonedHourToUtc(null, 2026, 7, 7, 23).toISOString()).toBe(
      `2026-07-07T23:00:00.000Z`
    )
  })

  // 2026-03-08, America/New_York: 02:00 local never happens (the clock jumps
  // 02:00 EST → 03:00 EDT at 07:00Z). The digest still goes out that day, at
  // the first instant after the gap.
  it(`resolves a DST spring-forward gap to the instant after the gap`, () => {
    expect(
      zonedHourToUtc(`America/New_York`, 2026, 3, 8, 2).toISOString()
    ).toBe(`2026-03-08T07:00:00.000Z`)
    // The hours around the gap are unaffected.
    expect(
      zonedHourToUtc(`America/New_York`, 2026, 3, 8, 1).toISOString()
    ).toBe(`2026-03-08T06:00:00.000Z`)
    expect(
      zonedHourToUtc(`America/New_York`, 2026, 3, 8, 3).toISOString()
    ).toBe(`2026-03-08T07:00:00.000Z`)
  })

  // 2026-11-01, America/New_York: 01:00 local happens twice (05:00Z EDT and
  // 06:00Z EST). Pick the FIRST deterministically — a digest must not drift.
  it(`resolves a DST fall-back overlap to the first occurrence`, () => {
    expect(
      zonedHourToUtc(`America/New_York`, 2026, 11, 1, 1).toISOString()
    ).toBe(`2026-11-01T05:00:00.000Z`)
  })
})

describe(`lastDailySendPoint`, () => {
  it(`returns today's send point once it has passed`, () => {
    expect(lastDailySendPoint(`UTC`, 8, NOW).toISOString()).toBe(
      `2026-07-07T08:00:00.000Z`
    )
    expect(lastDailySendPoint(`Europe/Berlin`, 8, NOW).toISOString()).toBe(
      `2026-07-07T06:00:00.000Z`
    )
  })

  it(`falls back to yesterday's when today's is still ahead`, () => {
    expect(lastDailySendPoint(`UTC`, 23, NOW).toISOString()).toBe(
      `2026-07-06T23:00:00.000Z`
    )
  })

  it(`uses the LOCAL calendar date, not the UTC one`, () => {
    // 23:30Z is already 2026-07-08 01:30 in Berlin, so "today" locally is the
    // 8th — whose 08:00 send point is still ahead → the 7th's.
    const lateUtc = new Date(`2026-07-07T23:30:00Z`)
    expect(lastDailySendPoint(`Europe/Berlin`, 8, lateUtc).toISOString()).toBe(
      `2026-07-07T06:00:00.000Z`
    )
  })

  it(`handles midnight, and month/year rollover`, () => {
    expect(
      lastDailySendPoint(
        `UTC`,
        0,
        new Date(`2026-07-07T00:00:00Z`)
      ).toISOString()
    ).toBe(`2026-07-07T00:00:00.000Z`)
    expect(
      lastDailySendPoint(
        `UTC`,
        8,
        new Date(`2026-08-01T02:00:00Z`)
      ).toISOString()
    ).toBe(`2026-07-31T08:00:00.000Z`)
    expect(
      lastDailySendPoint(
        `UTC`,
        8,
        new Date(`2026-01-01T02:00:00Z`)
      ).toISOString()
    ).toBe(`2025-12-31T08:00:00.000Z`)
  })

  it(`still lands on a send point across both DST transitions`, () => {
    expect(
      lastDailySendPoint(
        `America/New_York`,
        2,
        new Date(`2026-03-08T12:00:00Z`)
      ).toISOString()
    ).toBe(`2026-03-08T07:00:00.000Z`)
    expect(
      lastDailySendPoint(
        `America/New_York`,
        1,
        new Date(`2026-11-01T12:00:00Z`)
      ).toISOString()
    ).toBe(`2026-11-01T05:00:00.000Z`)
  })

  it(`treats a null/garbage zone as UTC and an out-of-range hour as 8`, () => {
    expect(lastDailySendPoint(null, 8, NOW).toISOString()).toBe(
      `2026-07-07T08:00:00.000Z`
    )
    expect(lastDailySendPoint(`Mars/Olympus_Mons`, 8, NOW).toISOString()).toBe(
      `2026-07-07T08:00:00.000Z`
    )
    expect(lastDailySendPoint(`UTC`, 24, NOW).toISOString()).toBe(
      `2026-07-07T08:00:00.000Z`
    )
    expect(lastDailySendPoint(`UTC`, null, NOW).toISOString()).toBe(
      `2026-07-07T08:00:00.000Z`
    )
  })
})

describe(`isDigestDue`, () => {
  it(`is due with no prior digest, whatever the cadence`, () => {
    expect(isDigestDue(null, `UTC`, null, NOW)).toBe(true)
    expect(
      isDigestDue(
        { ...defaultEmailPrefs(), digest: `daily` },
        `UTC`,
        undefined,
        NOW
      )
    ).toBe(true)
  })

  it(`hourly (off) cadence: not due right after a digest, due ~an hour later`, () => {
    const prefs = { ...defaultEmailPrefs(), digest: `off` }
    expect(isDigestDue(prefs, `UTC`, minutesAgo(10), NOW)).toBe(false)
    expect(isDigestDue(prefs, `UTC`, minutesAgo(55), NOW)).toBe(true)
  })

  it(`daily cadence: due once per local send point, not per elapsed gap`, () => {
    // NOW is 12:00Z; the send point is today 08:00Z.
    const prefs = { ...defaultEmailPrefs(), digest: `daily`, digestHour: 8 }
    // Sent AFTER today's send point → already had today's digest.
    expect(isDigestDue(prefs, `UTC`, minutesAgo(60 * 3), NOW)).toBe(false)
    // Sent BEFORE it (only 5h ago — the old 22h gap would have deferred this).
    expect(isDigestDue(prefs, `UTC`, minutesAgo(60 * 5), NOW)).toBe(true)
  })

  it(`daily cadence reads the send hour in the user's OWN timezone`, () => {
    const prefs = { ...defaultEmailPrefs(), digest: `daily`, digestHour: 8 }
    const now = new Date(`2026-07-20T10:00:00Z`)
    const lastSent = new Date(`2026-07-19T20:00:00Z`)
    // UTC: today's 08:00Z point has passed and predates the last send → due.
    expect(isDigestDue(prefs, `UTC`, lastSent, now)).toBe(true)
    // New York: 08:00 local is 12:00Z, still ahead — the last point was
    // yesterday 12:00Z, before which nothing was sent → not due.
    expect(isDigestDue(prefs, `America/New_York`, lastSent, now)).toBe(false)
  })

  it(`defaults to daily at hour 8 for a missing row`, () => {
    expect(isDigestDue(null, `UTC`, minutesAgo(60 * 3), NOW)).toBe(false)
    expect(isDigestDue(null, `UTC`, minutesAgo(60 * 5), NOW)).toBe(true)
    expect(
      isDigestDue(defaultEmailPrefs(), null, minutesAgo(60 * 3), NOW)
    ).toBe(false)
  })
})

describe(`isDigestRetryDue`, () => {
  it(`is due with no prior failure`, () => {
    expect(isDigestRetryDue(null, null, NOW)).toBe(true)
    expect(isDigestRetryDue(minutesAgo(60), undefined, NOW)).toBe(true)
  })

  it(`backs a failed attempt off a full day, not one sweep tick (EXP-227)`, () => {
    expect(isDigestRetryDue(null, minutesAgo(10), NOW)).toBe(false)
    expect(isDigestRetryDue(null, minutesAgo(60), NOW)).toBe(false)
    expect(isDigestRetryDue(null, minutesAgo(60 * 12), NOW)).toBe(false)
    expect(isDigestRetryDue(null, minutesAgo(60 * 23), NOW)).toBe(true)
  })

  it(`a success AFTER the failure clears the backoff (transport recovered)`, () => {
    expect(isDigestRetryDue(minutesAgo(30), minutesAgo(60), NOW)).toBe(true)
  })

  it(`a success BEFORE the failure does not clear the backoff`, () => {
    expect(isDigestRetryDue(minutesAgo(60 * 5), minutesAgo(30), NOW)).toBe(
      false
    )
  })
})

describe(`planEmailDigest`, () => {
  // NOW is 12:00Z and the default daily prefs read hour 8 in UTC, so the
  // current send point in these tests is today 08:00Z.

  // EXP-369/EXP-399: the daily cadence has NO minimum unread age — the send
  // hour is the delay: everything created at or before the send point and
  // still unread is bundled; rows created after it ride the next day's.
  it(`daily bundles every still-unread row created by the send point`, () => {
    const result = plan([
      candidate({ id: `n-old`, userId: `u1`, ageMinutes: 60 * 11 }),
      candidate({ id: `n-just-in-time`, userId: `u1`, ageMinutes: 245 }),
      candidate({ id: `n-after-point`, userId: `u1`, ageMinutes: 90 }),
      candidate({
        id: `n-read`,
        userId: `u1`,
        ageMinutes: 60 * 11,
        readAt: NOW,
      }),
    ])
    expect(result.batches).toHaveLength(1)
    expect(result.batches[0].items.map((i) => i.notificationId)).toEqual([
      `n-old`,
      `n-just-in-time`,
    ])
    // Post-send-point and read rows are dropped, never claimed:
    expect(result.claimOnly).toHaveLength(0)
  })

  // EXP-399 regression: a fresh notification must NOT email at the next sweep
  // just because the user's cadence gate says "due" — which it always did for
  // a never-sent user, and in the steady state too (digests only go out when
  // something is pending, so the last send usually predates today's point).
  it(`daily defers rows created after the send point — no instant email`, () => {
    const row = () => [candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })]

    // Never-sent user: due per the cadence gate, but the row waits.
    const neverSent = plan(row())
    expect(neverSent.batches).toHaveLength(0)
    expect(neverSent.claimOnly).toHaveLength(0)

    // Last digest before today's send point (the steady state): same.
    const stale = plan(row(), {
      lastDigestByUser: new Map([[`u1`, minutesAgo(60 * 26)]]),
    })
    expect(stale.batches).toHaveLength(0)
    expect(stale.claimOnly).toHaveLength(0)

    // The row rides the NEXT send point: at tomorrow's 08:00Z sweep it goes.
    const due = plan(row(), { now: new Date(`2026-07-08T08:05:00Z`) })
    expect(due.batches).toHaveLength(1)
    expect(due.batches[0].items.map((i) => i.notificationId)).toEqual([`n1`])
  })

  // The EXP-369 quirk survives: created five minutes before the send hour
  // still means push and email near-simultaneously.
  it(`daily sends a row created just before the send hour the same day`, () => {
    // Row created 07:55Z, sweep at 08:05Z.
    const result = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 245 })],
      { now: new Date(`2026-07-07T08:05:00Z`) }
    )
    expect(result.batches).toHaveLength(1)
  })

  it(`the daily send-point filter reads the user's OWN timezone`, () => {
    // Row created 10:30Z. In New York (hour 8 local = 12:00Z) the current
    // send point at 13:00Z is 12:00Z — AFTER the row's creation — so the row
    // goes out; in UTC the point is 08:00Z and the row defers.
    const now = new Date(`2026-07-07T13:00:00Z`)
    const row = () => [candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })]
    const newYork = plan(row(), {
      now,
      timezoneByUser: new Map([[`u1`, `America/New_York`]]),
    })
    expect(newYork.batches).toHaveLength(1)
    const utc = plan(row(), { now })
    expect(utc.batches).toHaveLength(0)
  })

  it(`the legacy hourly cadence keeps the 1h unread floor`, () => {
    const prefsByUser = new Map<string, EmailPrefsLike | null>([
      [`u1`, { ...defaultEmailPrefs(), digest: `off` }],
    ])
    const result = plan(
      [
        candidate({ id: `n-old`, userId: `u1`, ageMinutes: 90 }),
        candidate({ id: `n-fresh`, userId: `u1`, ageMinutes: 30 }),
      ],
      { prefsByUser }
    )
    expect(result.batches[0].items.map((i) => i.notificationId)).toEqual([
      `n-old`,
    ])
    expect(result.claimOnly).toHaveLength(0)
  })

  // The backstop is cadence-split: daily needs 48h because a row created just
  // after today's send point waits ~24h for the next one (and a failed-send
  // retry can push it further).
  it(`backstops daily at 48h and the hourly cadence at 24h`, () => {
    const hourly = new Map<string, EmailPrefsLike | null>([
      [`u1`, { ...defaultEmailPrefs(), digest: `off` }],
    ])
    const row = () => [
      candidate({ id: `n-30h`, userId: `u1`, ageMinutes: 60 * 30 }),
    ]
    expect(plan(row()).batches).toHaveLength(1)
    expect(plan(row(), { prefsByUser: hourly }).batches).toHaveLength(0)

    const ancient = [
      candidate({ id: `n-ancient`, userId: `u1`, ageMinutes: 60 * 49 }),
    ]
    expect(plan(ancient).batches).toHaveLength(0)
    expect(plan(ancient).claimOnly).toHaveLength(0)
  })

  it(`groups into ONE batch per user, items oldest-first, batches by userId`, () => {
    const result = plan([
      candidate({ id: `b-newer`, userId: `u2`, ageMinutes: 250 }),
      candidate({ id: `a-1`, userId: `u1`, ageMinutes: 270 }),
      candidate({ id: `b-older`, userId: `u2`, ageMinutes: 300 }),
    ])
    expect(result.batches.map((b) => b.userId)).toEqual([`u1`, `u2`])
    expect(result.batches[1].items.map((i) => i.notificationId)).toEqual([
      `b-older`,
      `b-newer`,
    ])
  })

  it(`missing prefs row means defaults: emailed`, () => {
    const result = plan([
      candidate({ id: `n1`, userId: `u1`, ageMinutes: 300 }),
    ])
    expect(result.batches).toHaveLength(1)
  })

  it(`master email switch off → rows are claimed without an email`, () => {
    const result = plan(
      [
        candidate({ id: `n1`, userId: `u1`, ageMinutes: 250 }),
        candidate({ id: `n2`, userId: `u1`, ageMinutes: 300 }),
      ],
      {
        prefsByUser: new Map([
          [`u1`, { ...defaultEmailPrefs(), emailEnabled: false }],
        ]),
      }
    )
    expect(result.batches).toHaveLength(0)
    expect(result.claimOnly.map((r) => r.notificationId).sort()).toEqual([
      `n1`,
      `n2`,
    ])
  })

  it(`per-type opt-out claims that type but still emails the rest`, () => {
    const result = plan(
      [
        candidate({
          id: `n-status`,
          userId: `u1`,
          ageMinutes: 300,
          type: `issue_status_changed`,
        }),
        candidate({
          id: `n-mention`,
          userId: `u1`,
          ageMinutes: 300,
          type: `issue_mention`,
        }),
      ],
      {
        prefsByUser: new Map([
          [
            `u1`,
            {
              ...defaultEmailPrefs(),
              typePrefs: { issue_status_changed: false as const },
            },
          ],
        ]),
      }
    )
    expect(result.batches).toHaveLength(1)
    expect(result.batches[0].items.map((i) => i.notificationId)).toEqual([
      `n-mention`,
    ])
    expect(result.claimOnly.map((r) => r.notificationId)).toEqual([`n-status`])
  })

  it(`cadence gate defers (does NOT claim) rows for users not yet due`, () => {
    const result = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 300 })],
      { lastDigestByUser: new Map([[`u1`, minutesAgo(10)]]) }
    )
    // Deferred entirely — the next sweep reconsiders it once the user is due.
    expect(result.batches).toHaveLength(0)
    expect(result.claimOnly).toHaveLength(0)
  })

  it(`failure backoff defers (does NOT claim) after a failed attempt`, () => {
    // A failed send leaves lastDigestByUser empty (no sent_at) but records a
    // failed attempt — the user must NOT retry at the next sweep tick, or
    // even the next hour: at most one retry per day (EXP-227).
    const deferred = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 300 })],
      { lastFailedByUser: new Map([[`u1`, minutesAgo(10)]]) }
    )
    expect(deferred.batches).toHaveLength(0)
    expect(deferred.claimOnly).toHaveLength(0)

    const stillDeferred = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 300 })],
      { lastFailedByUser: new Map([[`u1`, minutesAgo(60 * 12)]]) }
    )
    expect(stillDeferred.batches).toHaveLength(0)
  })

  it(`retries a day after a failed attempt`, () => {
    const result = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 300 })],
      { lastFailedByUser: new Map([[`u1`, minutesAgo(60 * 23)]]) }
    )
    expect(result.batches).toHaveLength(1)
  })

  it(`a stale failure with a success since does not defer`, () => {
    // Transport recovered: failed 5h ago, then a digest went out 30min ago —
    // only the success cadence gate governs (here: hourly, due at 55min).
    const prefs = new Map<string, EmailPrefsLike | null>([
      [`u1`, { ...defaultEmailPrefs(), digest: `off` }],
    ])
    const result = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })],
      {
        prefsByUser: prefs,
        lastDigestByUser: new Map([[`u1`, minutesAgo(55)]]),
        lastFailedByUser: new Map([[`u1`, minutesAgo(60 * 5)]]),
      }
    )
    expect(result.batches).toHaveLength(1)
  })

  it(`daily cadence bundles a day of unread rows into one email once due`, () => {
    const prefs = new Map<string, EmailPrefsLike | null>([
      [`u1`, { ...defaultEmailPrefs(), digest: `daily` }],
    ])
    const notDue = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 300 })],
      {
        prefsByUser: prefs,
        lastDigestByUser: new Map([[`u1`, minutesAgo(60 * 3)]]),
      }
    )
    expect(notDue.batches).toHaveLength(0)

    const due = plan(
      [
        candidate({ id: `n1`, userId: `u1`, ageMinutes: 300 }),
        candidate({ id: `n2`, userId: `u1`, ageMinutes: 60 * 12 }),
      ],
      {
        prefsByUser: prefs,
        lastDigestByUser: new Map([[`u1`, minutesAgo(60 * 23)]]),
      }
    )
    expect(due.batches).toHaveLength(1)
    expect(due.batches[0].items).toHaveLength(2)
  })
})

describe(`reporter resolution guards`, () => {
  it(`only done/cancelled count as resolution statuses`, () => {
    expect(isResolutionStatus(`done`)).toBe(true)
    expect(isResolutionStatus(`cancelled`)).toBe(true)
    expect(isResolutionStatus(`backlog`)).toBe(false)
    expect(isResolutionStatus(`todo`)).toBe(false)
    expect(isResolutionStatus(`in_progress`)).toBe(false)
    expect(isResolutionStatus(`duplicate`)).toBe(false)
  })

  it(`sends on first close`, () => {
    expect(
      shouldSendReporterResolution({
        toStatus: `done`,
        resolvedNotifiedAt: null,
      })
    ).toBe(true)
    expect(
      shouldSendReporterResolution({
        toStatus: `cancelled`,
        resolvedNotifiedAt: undefined,
      })
    ).toBe(true)
  })

  it(`is exactly-once: reopen→re-close does NOT re-email (flag stays set)`, () => {
    expect(
      shouldSendReporterResolution({
        toStatus: `done`,
        resolvedNotifiedAt: new Date(`2026-01-01T00:00:00Z`),
      })
    ).toBe(false)
  })

  it(`never sends on non-closing transitions`, () => {
    expect(
      shouldSendReporterResolution({
        toStatus: `in_progress`,
        resolvedNotifiedAt: null,
      })
    ).toBe(false)
  })
})

describe(`url builders`, () => {
  it(`builds the unsubscribe URL with an encoded token`, () => {
    expect(buildUnsubscribeUrl(`https://app.example.com`, `tok en/1`)).toBe(
      `https://app.example.com/api/email/unsubscribe?token=tok%20en%2F1`
    )
  })

  it(`tolerates a trailing slash on the base URL`, () => {
    expect(buildUnsubscribeUrl(`https://app.example.com/`, `abc`)).toBe(
      `https://app.example.com/api/email/unsubscribe?token=abc`
    )
  })

  it(`builds the issue deep link shared by push and email`, () => {
    expect(
      buildIssueDeepLinkPath({
        teamSlug: `metric`,
        boardSlug: `web`,
        identifier: `MET-12`,
      })
    ).toBe(`/t/metric/boards/web/issues/MET-12`)
  })
})
