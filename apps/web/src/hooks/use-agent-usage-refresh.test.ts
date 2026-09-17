import { describe, expect, it } from "vitest"
import {
  AUTO_REFRESH_RETRY_MS,
  MAX_BLIND_ATTEMPTS,
  REFRESH_BUCKET_LIMIT,
  REFRESH_BUCKET_WINDOW_MS,
  createUsageRefreshState,
  planUsageRefresh,
  usageRefreshEligible,
  type UsageRefreshState,
} from "@/hooks/use-agent-usage-refresh"
import type { AgentProfileUsageRow } from "@/lib/agent-usage"

// EXP-875: the Devices page's auto-refresh is a COMMAND RATE, and a command
// is a message to somebody's machine. These tests say, per tick, exactly how
// many go out and to which logins — the arithmetic the loop got wrong (eight
// machines × three logins each got their own 60 s floor, and logins that can
// never report usage were asked forever).

function row(overrides: Partial<AgentProfileUsageRow> = {}): AgentProfileUsageRow {
  const deviceId = overrides.deviceId ?? `dev-1`
  const agent = overrides.agent ?? `claude`
  const profileId = overrides.profileId ?? `system`
  return {
    key: `${deviceId}:${agent}:${profileId}`,
    deviceId,
    deviceLabel: deviceId,
    mine: true,
    online: true,
    agent,
    profileId,
    profileLabel: `Default`,
    active: true,
    signedIn: true,
    health: `ok`,
    email: `danny@example.com`,
    plan: null,
    usage: null,
    unmonitored: false,
    checkedAt: null,
    ...overrides,
  }
}

/** Eight machines, three logins each — the shape of the page that made the
 *  reviewers count commands in the first place. */
function fleet(devices = 8, logins = 3): AgentProfileUsageRow[] {
  const rows: AgentProfileUsageRow[] = []
  for (let d = 0; d < devices; d++) {
    for (let p = 0; p < logins; p++) {
      rows.push(row({ deviceId: `dev-${d}`, profileId: `p-${p}` }))
    }
  }
  return rows
}

function tick(
  rows: readonly AgentProfileUsageRow[],
  state: UsageRefreshState,
  at: number,
  inFlight: ReadonlySet<string> = new Set()
): string[] {
  return planUsageRefresh({
    rows,
    eligible: () => true,
    inFlight,
    now: new Date(at),
    at,
    state,
  })
}

describe(`usageRefreshEligible`, () => {
  it(`only passes a login that could ever answer`, () => {
    expect(usageRefreshEligible(row())).toBe(true)
    // A teammate's machine takes no commands from this page.
    expect(usageRefreshEligible(row({ mine: false }))).toBe(false)
    // A signed-out login can only report nothing, forever.
    expect(usageRefreshEligible(row({ signedIn: false }))).toBe(false)
    // The machine deliberately collects nothing for this one.
    expect(usageRefreshEligible(row({ unmonitored: true }))).toBe(false)
  })
})

describe(`planUsageRefresh`, () => {
  it(`spends at most the page bucket per window, however many logins`, () => {
    const rows = fleet()
    expect(rows).toHaveLength(24)
    const state = createUsageRefreshState()
    const at = 1_000_000

    const first = tick(rows, state, at)
    expect(first).toHaveLength(REFRESH_BUCKET_LIMIT)
    // The 30 s beat comes round again inside the window: nothing more.
    expect(tick(rows, state, at + 30_000)).toEqual([])
    expect(tick(rows, state, at + REFRESH_BUCKET_WINDOW_MS - 1)).toEqual([])

    // Next window, next four — and they are DIFFERENT logins (round-robin:
    // the ones waiting longest go first).
    const second = tick(rows, state, at + REFRESH_BUCKET_WINDOW_MS)
    expect(second).toHaveLength(REFRESH_BUCKET_LIMIT)
    expect(second.some((key) => first.includes(key))).toBe(false)
  })

  it(`comes round the whole list instead of starving its tail`, () => {
    const rows = fleet()
    const state = createUsageRefreshState()
    const seen = new Set<string>()
    // 24 logins, 4 a minute, 3 asks each before the blind cap: six windows
    // cover every row exactly once.
    for (let window = 0; window < 6; window++) {
      for (const key of tick(rows, state, 1_000_000 + window * REFRESH_BUCKET_WINDOW_MS)) {
        seen.add(key)
      }
    }
    expect(seen.size).toBe(rows.length)
  })

  it(`never commands a login that cannot report usage`, () => {
    const rows = [
      row({ deviceId: `mine`, mine: true }),
      row({ deviceId: `theirs`, mine: false }),
      row({ deviceId: `out`, signedIn: false }),
      row({ deviceId: `dark`, unmonitored: true }),
    ]
    const state = createUsageRefreshState()
    const commanded: string[] = []
    for (let window = 0; window < 10; window++) {
      commanded.push(
        ...tick(rows, state, 1_000_000 + window * REFRESH_BUCKET_WINDOW_MS)
      )
    }
    // Only the one answerable login, and only up to the blind cap.
    expect(new Set(commanded)).toEqual(new Set([`mine:claude:system`]))
    expect(commanded).toHaveLength(MAX_BLIND_ATTEMPTS)
  })

  it(`gives up on a login that keeps reporting nothing, until its row moves`, () => {
    const rows = [row()]
    const state = createUsageRefreshState()
    let at = 1_000_000
    for (let i = 0; i < MAX_BLIND_ATTEMPTS; i++) {
      expect(tick(rows, state, at)).toHaveLength(1)
      at += AUTO_REFRESH_RETRY_MS
    }
    // The machine never answered: stop asking.
    for (let i = 0; i < 5; i++) {
      expect(tick(rows, state, at)).toEqual([])
      at += AUTO_REFRESH_RETRY_MS
    }
    // …until the row actually changes (a report landed, long enough ago to be
    // past the device's own floor).
    const answered = [
      row({ usage: { windows: [], fetchedAt: new Date(at - 60 * 60_000).toISOString() } }),
    ]
    expect(tick(answered, state, at)).toEqual([answered[0]!.key])
  })

  it(`respects the per-login retry floor`, () => {
    const rows = [row()]
    const state = createUsageRefreshState()
    const at = 1_000_000
    expect(tick(rows, state, at)).toHaveLength(1)
    expect(tick(rows, state, at + AUTO_REFRESH_RETRY_MS - 1)).toEqual([])
    expect(tick(rows, state, at + AUTO_REFRESH_RETRY_MS)).toHaveLength(1)
  })

  it(`respects the DEVICE's own rate-limit floor`, () => {
    const at = 1_000_000
    const rows = [
      row({ usage: { windows: [], fetchedAt: new Date(at - 1_000).toISOString() } }),
    ]
    const state = createUsageRefreshState()
    expect(tick(rows, state, at)).toEqual([])
  })

  it(`skips a login with a command already in flight`, () => {
    const rows = [row()]
    const state = createUsageRefreshState()
    expect(tick(rows, state, 1_000_000, new Set([rows[0]!.key]))).toEqual([])
  })

  it(`lets the caller veto a row (offline, no cap)`, () => {
    const rows = [row({ deviceId: `a` }), row({ deviceId: `b` })]
    const state = createUsageRefreshState()
    const keys = planUsageRefresh({
      rows,
      eligible: (candidate) => candidate.deviceId === `b`,
      inFlight: new Set(),
      now: new Date(1_000_000),
      at: 1_000_000,
      state,
    })
    expect(keys).toEqual([`b:claude:system`])
  })
})
