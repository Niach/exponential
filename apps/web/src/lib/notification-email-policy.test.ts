import { describe, expect, it } from "vitest"
import {
  buildIssueDeepLinkPath,
  buildUnsubscribeUrl,
  defaultEmailPrefs,
  emailTypeAllowed,
  isDigestDue,
  isDigestRetryDue,
  isDigestSendable,
  isResolutionStatus,
  planEmailDigest,
  shouldSendReporterResolution,
  type DigestCandidate,
  type EmailPrefsLike,
} from "@/lib/notification-email-policy"
import type { NotificationType } from "@/lib/domain"

describe(`emailTypeAllowed`, () => {
  it(`defaults to allowed when no prefs row exists (missing row = defaults)`, () => {
    expect(emailTypeAllowed(null, `issue_comment`)).toBe(true)
    expect(emailTypeAllowed(undefined, `pr_merged`)).toBe(true)
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
    ] as const) {
      expect(emailTypeAllowed(prefs, type)).toBe(true)
    }
  })

  it(`the master emailEnabled switch blocks all types`, () => {
    const prefs = { ...defaultEmailPrefs(), emailEnabled: false }
    expect(emailTypeAllowed(prefs, `issue_assigned`)).toBe(false)
    expect(emailTypeAllowed(prefs, `pr_merged`)).toBe(false)
  })

  it(`per-type opt-out blocks only that type`, () => {
    const prefs = {
      ...defaultEmailPrefs(),
      typePrefs: { issue_comment: false as const },
    }
    expect(emailTypeAllowed(prefs, `issue_comment`)).toBe(false)
    expect(emailTypeAllowed(prefs, `issue_mention`)).toBe(true)
    expect(emailTypeAllowed(prefs, `pr_opened`)).toBe(true)
  })

  it(`an explicit true in typePrefs stays on`, () => {
    const prefs = {
      ...defaultEmailPrefs(),
      typePrefs: { issue_assigned: true as const },
    }
    expect(emailTypeAllowed(prefs, `issue_assigned`)).toBe(true)
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
  }
) {
  return planEmailDigest({
    candidates,
    prefsByUser: opts?.prefsByUser ?? new Map(),
    lastDigestByUser: opts?.lastDigestByUser ?? new Map(),
    lastFailedByUser: opts?.lastFailedByUser ?? new Map(),
    now: NOW,
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
})

describe(`isDigestDue`, () => {
  it(`is due with no prior digest, whatever the cadence`, () => {
    expect(isDigestDue(null, null, NOW)).toBe(true)
    expect(
      isDigestDue({ ...defaultEmailPrefs(), digest: `daily` }, undefined, NOW)
    ).toBe(true)
  })

  it(`hourly (off) cadence: not due right after a digest, due ~an hour later`, () => {
    const prefs = { ...defaultEmailPrefs(), digest: `off` }
    expect(isDigestDue(prefs, minutesAgo(10), NOW)).toBe(false)
    expect(isDigestDue(prefs, minutesAgo(55), NOW)).toBe(true)
  })

  it(`daily cadence: at most one digest per ~day`, () => {
    const prefs = { ...defaultEmailPrefs(), digest: `daily` }
    expect(isDigestDue(prefs, minutesAgo(60 * 5), NOW)).toBe(false)
    expect(isDigestDue(prefs, minutesAgo(60 * 23), NOW)).toBe(true)
  })

  it(`defaults to daily: a missing row is not due 5h after a digest`, () => {
    expect(isDigestDue(defaultEmailPrefs(), minutesAgo(60 * 5), NOW)).toBe(false)
    expect(isDigestDue(null, minutesAgo(60 * 5), NOW)).toBe(false)
    expect(isDigestDue(null, minutesAgo(60 * 23), NOW)).toBe(true)
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
  it(`emails only notifications that stayed unread past the 1h window`, () => {
    const result = plan([
      candidate({ id: `n-old`, userId: `u1`, ageMinutes: 90 }),
      candidate({ id: `n-fresh`, userId: `u1`, ageMinutes: 30 }),
      candidate({ id: `n-read`, userId: `u1`, ageMinutes: 90, readAt: NOW }),
    ])
    expect(result.batches).toHaveLength(1)
    expect(result.batches[0].items.map((i) => i.notificationId)).toEqual([
      `n-old`,
    ])
    // Fresh + read rows are deferred/dropped, never claimed:
    expect(result.claimOnly).toHaveLength(0)
  })

  it(`never emails rows past the 24h backstop floor`, () => {
    const result = plan([
      candidate({ id: `n-ancient`, userId: `u1`, ageMinutes: 60 * 25 }),
    ])
    expect(result.batches).toHaveLength(0)
    expect(result.claimOnly).toHaveLength(0)
  })

  it(`groups into ONE batch per user, items oldest-first, batches by userId`, () => {
    const result = plan([
      candidate({ id: `b-newer`, userId: `u2`, ageMinutes: 70 }),
      candidate({ id: `a-1`, userId: `u1`, ageMinutes: 90 }),
      candidate({ id: `b-older`, userId: `u2`, ageMinutes: 120 }),
    ])
    expect(result.batches.map((b) => b.userId)).toEqual([`u1`, `u2`])
    expect(result.batches[1].items.map((i) => i.notificationId)).toEqual([
      `b-older`,
      `b-newer`,
    ])
  })

  it(`missing prefs row means defaults: emailed`, () => {
    const result = plan([candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })])
    expect(result.batches).toHaveLength(1)
  })

  it(`master email switch off → rows are claimed without an email`, () => {
    const result = plan(
      [
        candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 }),
        candidate({ id: `n2`, userId: `u1`, ageMinutes: 120 }),
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
          ageMinutes: 90,
          type: `issue_status_changed`,
        }),
        candidate({
          id: `n-mention`,
          userId: `u1`,
          ageMinutes: 90,
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
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })],
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
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })],
      { lastFailedByUser: new Map([[`u1`, minutesAgo(10)]]) }
    )
    expect(deferred.batches).toHaveLength(0)
    expect(deferred.claimOnly).toHaveLength(0)

    const stillDeferred = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })],
      { lastFailedByUser: new Map([[`u1`, minutesAgo(60 * 12)]]) }
    )
    expect(stillDeferred.batches).toHaveLength(0)
  })

  it(`retries a day after a failed attempt`, () => {
    const result = plan(
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })],
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
      [candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 })],
      {
        prefsByUser: prefs,
        lastDigestByUser: new Map([[`u1`, minutesAgo(60 * 3)]]),
      }
    )
    expect(notDue.batches).toHaveLength(0)

    const due = plan(
      [
        candidate({ id: `n1`, userId: `u1`, ageMinutes: 90 }),
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
