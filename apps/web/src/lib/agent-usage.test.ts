import { describe, expect, it } from "vitest"
import {
  accountCaption,
  agentHealth,
  agentProfileUsageRows,
  attentionRank,
  deviceLoginRows,
  deviceWorstHealth,
  healthBadgeLabel,
  loginLabel,
  miniWindows,
  worstHealth,
  sortDeviceLogins,
  type AgentProfileUsageRow,
  contextPercent,
  formatContextUsage,
  formatResetCountdown,
  blockedBadgeLabel,
  formatUsageCost,
  parseAgentLoginResult,
  parseAgentUsage,
  parseAgentUsageMap,
  sessionAgentUsage,
  severity,
  usageAge,
  usageGroups,
  usageIsFresh,
  usageState,
  CONTEXT_SECTION_TITLE,
  NO_LOGIN_REPORTED,
  USAGE_FRESH_MS,
} from "./agent-usage"
import type { CodingSession, Device, DeviceAgentUsage } from "@/db/schema"

type SessionUsageRow = Pick<
  CodingSession,
  `deviceId` | `userId` | `agent` | `status`
>
type SessionUsageDevice = Pick<Device, `deviceId` | `userId` | `agentUsage`>

// EXP-484: the ONE fixture every client's presentation tests run against
// (iOS AgentUsagePresentationTests, Android AgentUsagePresentationTest,
// desktop usage_bar.rs). Window keys and labels are the locked vocabulary:
// `session`/`5h`, `weekly`/`Week`, `model:<name>`/`<Name>`, `credits`/
// `Credits`.
const NOW = new Date(`2026-08-28T12:00:00.000Z`)

const USAGE: DeviceAgentUsage = {
  fetchedAt: `2026-08-28T11:55:00.000Z`,
  stale: false,
  windows: [
    {
      key: `session`,
      label: `5h`,
      percent: 42,
      resetsAt: `2026-08-28T14:10:00.000Z`,
    },
    {
      key: `weekly`,
      label: `Week`,
      percent: 81,
      resetsAt: `2026-09-01T02:00:00.000Z`,
    },
    {
      key: `model:fable`,
      label: `Fable`,
      percent: 96,
      resetsAt: `2026-09-01T02:00:00.000Z`,
    },
    { key: `credits`, label: `Credits`, percent: 12, resetsAt: null },
  ],
}

describe(`parseAgentUsage`, () => {
  it(`round-trips the fixture`, () => {
    expect(parseAgentUsage(USAGE)).toEqual(USAGE)
  })

  it(`drops malformed windows and clamps percent`, () => {
    const parsed = parseAgentUsage({
      fetchedAt: `2026-08-28T11:55:00.000Z`,
      windows: [
        { key: `session`, label: `5h`, percent: 137.6 },
        { key: ``, label: `5h`, percent: 10 },
        { key: `weekly`, percent: 10 },
        `nonsense`,
        { key: `credits`, label: `Credits`, percent: -4 },
      ],
    })
    expect(parsed?.windows).toEqual([
      { key: `session`, label: `5h`, percent: 100, resetsAt: null },
      { key: `credits`, label: `Credits`, percent: 0, resetsAt: null },
    ])
  })

  it(`keeps at most ten windows`, () => {
    const parsed = parseAgentUsage({
      windows: Array.from({ length: 14 }, (_, i) => ({
        key: `w${i}`,
        label: `W${i}`,
        percent: i,
      })),
    })
    expect(parsed?.windows).toHaveLength(10)
  })

  it(`yields null for a non-object`, () => {
    expect(parseAgentUsage(null)).toBeNull()
    expect(parseAgentUsage([1, 2])).toBeNull()
    expect(parseAgentUsage(`{}`)).toBeNull()
  })
})

describe(`parseAgentUsageMap`, () => {
  it(`parses per agent and drops unusable entries`, () => {
    const map = parseAgentUsageMap({ claude: USAGE, codex: 7 })
    expect(Object.keys(map)).toEqual([`claude`])
    expect(map.claude.windows).toHaveLength(4)
  })

  it(`yields an empty map for a non-object`, () => {
    expect(parseAgentUsageMap(undefined)).toEqual({})
  })
})

describe(`usageIsFresh`, () => {
  it(`is fresh inside the window`, () => {
    expect(usageIsFresh(USAGE, NOW)).toBe(true)
  })

  it(`fails closed without a fetchedAt`, () => {
    expect(usageIsFresh({ windows: [] }, NOW)).toBe(false)
    expect(usageIsFresh({ fetchedAt: `nope`, windows: [] }, NOW)).toBe(false)
    expect(usageIsFresh(null, NOW)).toBe(false)
  })

  it(`is stale past the window`, () => {
    const old = new Date(NOW.getTime() - USAGE_FRESH_MS - 1).toISOString()
    expect(usageIsFresh({ ...USAGE, fetchedAt: old }, NOW)).toBe(false)
  })

  it(`ignores the device's own stale flag — the view ORs that in`, () => {
    expect(usageIsFresh({ ...USAGE, stale: true }, NOW)).toBe(true)
  })
})

describe(`usageGroups`, () => {
  it(`usage groups split current, weekly and other`, () => {
    expect(usageGroups(USAGE, NOW)).toEqual([
      {
        key: `session`,
        title: `Current session`,
        cards: [
          {
            key: `session`,
            title: `Current session`,
            percent: 42,
            severity: `normal`,
            caption: `resets in 2h 10m`,
          },
        ],
      },
      {
        key: `weekly`,
        title: ``,
        cards: [
          {
            key: `weekly`,
            title: `All models`,
            percent: 81,
            severity: `warning`,
            caption: `resets in 3d 14h`,
          },
          {
            key: `model:fable`,
            title: `Fable only`,
            percent: 96,
            severity: `danger`,
            caption: `resets in 3d 14h`,
          },
        ],
      },
      {
        key: `other`,
        title: `Other`,
        cards: [
          {
            key: `credits`,
            title: `Credits`,
            percent: 12,
            severity: `normal`,
            caption: ``,
          },
        ],
      },
    ])
  })

  it(`keeps every other window in report order`, () => {
    const groups = usageGroups(
      {
        ...USAGE,
        windows: [
          ...USAGE.windows,
          { key: `credits`, label: `Credits`, percent: 16, resetsAt: null },
        ],
      },
      NOW
    )
    expect(groups.map((group) => group.key)).toEqual([
      `session`,
      `weekly`,
      `other`,
    ])
    expect(groups[2].cards.map((card) => card.percent)).toEqual([12, 16])
  })

  it(`says an untouched session window has not started`, () => {
    const groups = usageGroups(
      {
        ...USAGE,
        windows: [{ key: `session`, label: `5h`, percent: 0, resetsAt: null }],
      },
      NOW
    )
    expect(groups).toEqual([
      {
        key: `session`,
        title: `Current session`,
        cards: [
          {
            key: `session`,
            title: `Current session`,
            percent: 0,
            severity: `normal`,
            caption: `Starts when a message is sent`,
          },
        ],
      },
    ])
  })

  it(`gives the weekly group no title — its cards name themselves`, () => {
    const groups = usageGroups(USAGE, NOW)
    expect(groups.find((group) => group.key === `weekly`)?.title).toBe(``)
  })

  it(`is empty without windows`, () => {
    expect(usageGroups({ windows: [] }, NOW)).toEqual([])
    expect(usageGroups(null, NOW)).toEqual([])
  })
})

// EXP-909: the SHORT form of the same fixture — three windows, wire labels,
// one line. Mirrored ×4 under these exact names (desktop `mini_windows`, iOS
// `testMiniWindowsPickSessionWeeklyThenTheFirstModelWindow`, Android).
describe(`miniWindows`, () => {
  it(`mini windows pick session, weekly, then the first model window`, () => {
    expect(miniWindows(USAGE).map((window) => window.label)).toEqual([
      `5h`,
      `Week`,
      `Fable`,
    ])
    // The WIRE labels, and the percents ride along untouched — the mini line
    // has no room for `Fable only` and no business re-deriving a number.
    expect(miniWindows(USAGE).map((window) => window.percent)).toEqual([
      42, 81, 96,
    ])
    // Only the FIRST model window: three bars is the whole format.
    const many = miniWindows({
      ...USAGE,
      windows: [
        ...USAGE.windows,
        { key: `model:sonnet`, label: `Sonnet`, percent: 5, resetsAt: null },
      ],
    })
    expect(many.map((window) => window.key)).toEqual([
      `session`,
      `weekly`,
      `model:fable`,
    ])
  })

  it(`mini windows fall back to report order`, () => {
    // A report that names none of the three still gets a line: its first
    // three windows, as reported.
    const windows = [
      { key: `credits`, label: `Credits`, percent: 12, resetsAt: null },
      { key: `monthly`, label: `Month`, percent: 40, resetsAt: null },
      { key: `daily`, label: `Day`, percent: 3, resetsAt: null },
      { key: `hourly`, label: `Hour`, percent: 1, resetsAt: null },
    ]
    expect(miniWindows({ windows }).map((window) => window.label)).toEqual([
      `Credits`,
      `Month`,
      `Day`,
    ])
    expect(miniWindows({ windows: [] })).toEqual([])
    expect(miniWindows(null)).toEqual([])
  })
})

describe(`usageAge`, () => {
  it(`usage age says as-of when not fresh or stale`, () => {
    // Fresh and not flagged: the numbers speak for themselves.
    expect(usageAge(USAGE, NOW)).toBeNull()
    // Past the freshness window…
    const old = new Date(NOW.getTime() - USAGE_FRESH_MS - 1).toISOString()
    expect(usageAge({ ...USAGE, fetchedAt: old }, NOW)).toMatch(/^as of /)
    // …or the device's own stale flag on an otherwise current report.
    expect(usageAge({ ...USAGE, stale: true }, NOW)).toMatch(/^as of /)
    // Nothing to date, nothing to say.
    expect(usageAge({ windows: [] }, NOW)).toBeNull()
    expect(usageAge(null, NOW)).toBeNull()
  })
})

describe(`severity`, () => {
  it(`crosses at 75 and 95`, () => {
    expect(severity(0)).toBe(`normal`)
    expect(severity(74)).toBe(`normal`)
    expect(severity(75)).toBe(`warning`)
    expect(severity(94)).toBe(`warning`)
    expect(severity(95)).toBe(`danger`)
    expect(severity(100)).toBe(`danger`)
  })
})

describe(`formatResetCountdown`, () => {
  it(`formats minutes, hours and days`, () => {
    expect(formatResetCountdown(`2026-08-28T12:45:00.000Z`, NOW)).toBe(
      `resets in 45m`
    )
    expect(formatResetCountdown(`2026-08-28T14:10:00.000Z`, NOW)).toBe(
      `resets in 2h 10m`
    )
    expect(formatResetCountdown(`2026-08-28T14:00:00.000Z`, NOW)).toBe(
      `resets in 2h`
    )
    expect(formatResetCountdown(`2026-08-31T02:00:00.000Z`, NOW)).toBe(
      `resets in 2d 14h`
    )
    expect(formatResetCountdown(`2026-08-31T12:00:00.000Z`, NOW)).toBe(
      `resets in 3d`
    )
  })

  it(`says soon inside the last minute and after the stamp`, () => {
    expect(formatResetCountdown(`2026-08-28T12:00:30.000Z`, NOW)).toBe(
      `resets soon`
    )
    expect(formatResetCountdown(`2026-08-28T11:00:00.000Z`, NOW)).toBe(
      `resets soon`
    )
  })

  it(`is null without a usable stamp`, () => {
    expect(formatResetCountdown(null, NOW)).toBeNull()
    expect(formatResetCountdown(`later`, NOW)).toBeNull()
  })
})

describe(`blockedBadgeLabel`, () => {
  const blocked = (over: Record<string, unknown> = {}) => ({
    kind: `rate_limit`,
    agent: `claude`,
    window: `session`,
    resetsAt: `2026-08-28T14:00:00.000Z`,
    since: `2026-08-28T11:30:00.000Z`,
    ...over,
  })

  it(`names the wall and counts down to the reset`, () => {
    expect(blockedBadgeLabel(blocked(), NOW)).toBe(`Rate limited · resets in 2h`)
  })

  it(`drops the countdown when the agent named no reset`, () => {
    expect(blockedBadgeLabel(blocked({ resetsAt: null }), NOW)).toBe(
      `Rate limited`
    )
    expect(blockedBadgeLabel(blocked({ resetsAt: `later` }), NOW)).toBe(
      `Rate limited`
    )
  })

  it(`still badges a wall this build has no name for`, () => {
    expect(blockedBadgeLabel(blocked({ kind: `quota` }), NOW)).toBe(
      `Blocked · resets in 2h`
    )
  })

  it(`is null when the run is not blocked`, () => {
    expect(blockedBadgeLabel(null, NOW)).toBeNull()
    expect(blockedBadgeLabel(undefined, NOW)).toBeNull()
  })
})

describe(`accountCaption`, () => {
  it(`is the email alone — no prefix, no plan tail`, () => {
    expect(
      accountCaption({
        signedIn: true,
        email: `danny@example.com`,
        plan: `Max`,
      })
    ).toBe(`danny@example.com`)
    expect(accountCaption({ signedIn: true, email: `danny@example.com` })).toBe(
      `danny@example.com`
    )
  })

  it(`says signed out`, () => {
    expect(accountCaption({ signedIn: false })).toBe(`signed out`)
  })

  it(`renders a plan-only account (a provider, no email)`, () => {
    expect(accountCaption({ signedIn: true, plan: `anthropic (oauth)` })).toBe(
      `anthropic (oauth)`
    )
  })

  it(`degrades to signed in, then to unknown`, () => {
    expect(accountCaption({ signedIn: true })).toBe(`signed in`)
    expect(accountCaption(null)).toBe(`unknown`)
  })
})

describe(`sessionAgentUsage`, () => {
  const session = {
    deviceId: `dev-1`,
    userId: `user-1`,
    agent: `claude`,
    status: `running`,
  } as SessionUsageRow
  const device = {
    deviceId: `dev-1`,
    userId: `user-1`,
    agentUsage: { claude: USAGE },
  } as SessionUsageDevice

  it(`resolves the host machine's numbers for the session's agent`, () => {
    expect(sessionAgentUsage(session, [device], NOW)).toMatchObject({
      agent: `claude`,
    })
    expect(
      sessionAgentUsage({ ...session, status: `in_review` }, [device], NOW)
    ).not.toBeNull()
  })

  it(`prefers the session owner's own row on a shared deviceId`, () => {
    const foreign = {
      deviceId: `dev-1`,
      userId: `other`,
      agentUsage: { claude: { ...USAGE, windows: [] } },
    } as SessionUsageDevice
    expect(
      sessionAgentUsage(session, [foreign, device], NOW)?.usage.windows
    ).toHaveLength(4)
  })

  it(`hides stale numbers beside a live agent`, () => {
    const old = new Date(NOW.getTime() - USAGE_FRESH_MS - 1).toISOString()
    const stale = {
      ...device,
      agentUsage: { claude: { ...USAGE, fetchedAt: old } },
    } as SessionUsageDevice
    expect(sessionAgentUsage(session, [stale], NOW)).toBeNull()
  })

  it(`hides on ended runs, agent-less rows, unknown devices and empty usage`, () => {
    expect(
      sessionAgentUsage({ ...session, status: `ended` }, [device], NOW)
    ).toBeNull()
    expect(
      sessionAgentUsage({ ...session, agent: null }, [device], NOW)
    ).toBeNull()
    expect(
      sessionAgentUsage({ ...session, deviceId: null }, [device], NOW)
    ).toBeNull()
    expect(sessionAgentUsage(session, [], NOW)).toBeNull()
    expect(
      sessionAgentUsage({ ...session, agent: `codex` }, [device], NOW)
    ).toBeNull()
    expect(
      sessionAgentUsage(
        session,
        [{ ...device, agentUsage: { claude: { windows: [] } } }],
        NOW
      )
    ).toBeNull()
  })
})

describe(`parseAgentLoginResult`, () => {
  // The exact strings the desktop's `LoginProgress::to_result_text` writes
  // (apps/desktop/crates/coding/src/agent_login.rs) — codex hands back a
  // device code, claude only a URL, and anything else on a command result is
  // not a login answer at all.
  it(`reads a codex device-code answer`, () => {
    expect(
      parseAgentLoginResult(
        `{"agent":"codex","phase":"url","url":"https://auth.openai.com/device","code":"WDJB-MJHT"}`
      )
    ).toEqual({
      agent: `codex`,
      phase: `url`,
      url: `https://auth.openai.com/device`,
      code: `WDJB-MJHT`,
      message: null,
    })
  })

  it(`reads a claude answer with no code`, () => {
    expect(
      parseAgentLoginResult(
        `{"agent":"claude","phase":"url","url":"https://claude.ai/oauth/authorize?x=1"}`
      )
    ).toEqual({
      agent: `claude`,
      phase: `url`,
      url: `https://claude.ai/oauth/authorize?x=1`,
      code: null,
      message: null,
    })
  })

  it(`reads a failure and rejects anything that isn't a login answer`, () => {
    expect(
      parseAgentLoginResult(
        `{"agent":"claude","phase":"failed","message":"No sign-in URL appeared."}`
      )
    ).toEqual({
      agent: `claude`,
      phase: `failed`,
      url: null,
      code: null,
      message: `No sign-in URL appeared.`,
    })
    expect(parseAgentLoginResult(`Pruned 2 worktrees`)).toBeNull()
    expect(parseAgentLoginResult(``)).toBeNull()
    expect(parseAgentLoginResult(null)).toBeNull()
    // A `url` phase with no URL is unusable — treat it as not an answer.
    expect(parseAgentLoginResult(`{"agent":"codex","phase":"url"}`)).toBeNull()
  })
})

// EXP-746: the RUN's own context meter, beside the machine's rate-limit
// cards. Both test names are mirrored ×4.
describe(`session context usage`, () => {
  it(`context usage reads used over size with a percent`, () => {
    expect(
      formatContextUsage({ contextUsed: 124_000, contextSize: 200_000 })
    ).toBe(`124k / 200k (62%)`)
    expect(contextPercent({ contextUsed: 124_000, contextSize: 200_000 })).toBe(
      62
    )
    // Under a thousand tokens stays a plain count; the percent floors, so a
    // bar never reads full before it is.
    expect(formatContextUsage({ contextUsed: 999, contextSize: 1_000 })).toBe(
      `999 / 1k (99%)`
    )
    // The fixtures that catch ×4 arithmetic drift, one per trap:
    // an exact fraction (`(used / size) * 100` floors this to 57 in floating
    // point, `used * 100 / size` gives 58 like the other three clients)…
    expect(
      formatContextUsage({ contextUsed: 116_000, contextSize: 200_000 })
    ).toBe(`116k / 200k (58%)`)
    expect(contextPercent({ contextUsed: 116_000, contextSize: 200_000 })).toBe(
      58
    )
    // …a count that is not a round thousand (k ROUNDS, never truncates, so
    // this is 125k and not 124k)…
    expect(
      formatContextUsage({ contextUsed: 124_600, contextSize: 200_000 })
    ).toBe(`125k / 200k (62%)`)
    expect(
      formatContextUsage({ contextUsed: 1_500, contextSize: 200_000 })
    ).toBe(`2k / 200k (0%)`)
    // …and a run past its own window, which reads full rather than 150%.
    expect(formatContextUsage({ contextUsed: 300, contextSize: 200 })).toBe(
      `300 / 200 (100%)`
    )
    expect(contextPercent({ contextUsed: 300, contextSize: 200 })).toBe(100)
    // No measurement at all renders nothing.
    expect(formatContextUsage(null)).toBe(``)
    expect(contextPercent(null)).toBeNull()
    expect(contextPercent({ contextUsed: 10, contextSize: 0 })).toBeNull()
  })

  it(`a cost under half a cent renders nothing`, () => {
    expect(
      formatUsageCost({ contextUsed: 1, contextSize: 2, costUsd: 1.235 })
    ).toBe(`$1.24`)
    expect(
      formatUsageCost({ contextUsed: 1, contextSize: 2, costUsd: 0.004 })
    ).toBeNull()
    expect(formatUsageCost({ contextUsed: 1, contextSize: 2 })).toBeNull()
    expect(formatUsageCost(null)).toBeNull()
  })

  it(`pins the section title byte for byte`, () => {
    expect(CONTEXT_SECTION_TITLE).toBe(`Context`)
  })
})

// EXP-849: account health — the device's probe verdict, its fallback and the
// two badge strings. Hand-mirrored with the desktop's `usage_bar.rs` and the
// natives' account rows.
describe(`account health (EXP-849)`, () => {
  it(`derives health from signedIn when the device sent none`, () => {
    expect(agentHealth({ signedIn: true })).toBe(`ok`)
    expect(agentHealth({ signedIn: false })).toBe(`signed_out`)
    expect(agentHealth(null)).toBe(`unknown`)
  })

  it(`prefers the device's own verdict over the derivation`, () => {
    expect(agentHealth({ signedIn: true, health: `needs_relogin` })).toBe(
      `needs_relogin`
    )
    // A signed-in CLI that was never probed is UNKNOWN, not ok.
    expect(agentHealth({ signedIn: true, health: `unknown` })).toBe(`unknown`)
  })

  it(`badges only the two negatives, and keeps them distinct`, () => {
    expect(healthBadgeLabel(`needs_relogin`)).toBe(`Needs re-login`)
    expect(healthBadgeLabel(`signed_out`)).toBe(`Signed out`)
    expect(healthBadgeLabel(`ok`)).toBeNull()
    expect(healthBadgeLabel(`unknown`)).toBeNull()
  })

  it(`folds a set to its worst value`, () => {
    expect(worstHealth([`ok`, `unknown`, `needs_relogin`, `signed_out`])).toBe(
      `needs_relogin`
    )
    expect(worstHealth([`ok`, `unknown`])).toBe(`unknown`)
    expect(worstHealth([])).toBeNull()
  })

  it(`badges a device row with the worst health of its accounts`, () => {
    expect(
      deviceWorstHealth({
        agentAccounts: {
          claude: {
            signedIn: true,
            profiles: [
              { id: `system`, signedIn: true, health: `ok`, active: true },
              { id: `work`, signedIn: true, health: `needs_relogin` },
            ],
          },
          codex: { signedIn: true, health: `ok` },
        },
      })
    ).toBe(`needs_relogin`)
    // A pre-profile machine falls back to the top-level account…
    expect(
      deviceWorstHealth({ agentAccounts: { claude: { signedIn: false } } })
    ).toBe(`signed_out`)
    // …and a machine that reported nothing claims nothing.
    expect(deviceWorstHealth({ agentAccounts: {} })).toBeNull()
  })

  it(`ranks an expired credential with the signed-out rows`, () => {
    expect(attentionRank({ signedIn: true, usage: null, health: `ok` })).toBe(2)
    expect(
      attentionRank({ signedIn: true, usage: null, health: `needs_relogin` })
    ).toBe(0)
    expect(attentionRank({ signedIn: false, usage: null })).toBe(0)
  })
})

// EXP-849 retired `pi` from contract `codingAgent`. The server clamps what a
// machine may write (lib/trpc/devices.ts), but a row written before that
// clamp — or one served by a self-hosted instance on an older image — must
// still never surface: no usage row, no account chip, no health verdict for
// an agent this build has no name, icon or launcher for.
describe(`retired agent ids (EXP-849)`, () => {
  const piDevice = {
    deviceId: `unraid`,
    label: `unraid`,
    userId: `me`,
    agentAccounts: {
      claude: { signedIn: true, email: `dev@acme.test` },
      pi: {
        signedIn: true,
        email: `dev@acme.test`,
        health: `needs_relogin` as const,
        profiles: [{ id: `system`, signedIn: true, active: true }],
      },
    },
    agentUsage: {
      claude: {
        fetchedAt: `2026-08-28T11:55:00.000Z`,
        stale: false,
        windows: [{ key: `session`, label: `5h`, percent: 20, resetsAt: null }],
      },
      pi: {
        fetchedAt: `2026-08-28T11:55:00.000Z`,
        stale: false,
        windows: [{ key: `session`, label: `5h`, percent: 99, resetsAt: null }],
      },
    },
    agentUsageAt: null,
    lastSeenAt: new Date(`2026-08-28T11:59:00.000Z`),
  }

  it(`yields no usage row for a retired agent`, () => {
    const rows = agentProfileUsageRows([piDevice], `me`, () => true)
    expect(rows.map((row) => row.agent)).toEqual([`claude`])
  })

  it(`never badges a device off a retired agent's health`, () => {
    // Without the filter the retired `pi` login would drag the whole machine
    // to "Needs re-login" with no row to act on.
    expect(deviceWorstHealth({ agentAccounts: piDevice.agentAccounts })).toBe(
      `ok`
    )
  })

  it(`reports nothing at all for a machine that ONLY knows the retired agent`, () => {
    const onlyPi = {
      ...piDevice,
      agentAccounts: { pi: { signedIn: true } },
      agentUsage: { pi: piDevice.agentUsage.pi },
    }
    expect(agentProfileUsageRows([onlyPi], `me`, () => true)).toEqual([])
    expect(deviceWorstHealth({ agentAccounts: onlyPi.agentAccounts })).toBeNull()
  })
})

// EXP-862: every login's numbers are collected, so an empty bar means one of
// three different things — and the row has to say which.
describe(`usageState (EXP-862)`, () => {
  const row = (over: Partial<AgentProfileUsageRow> = {}) => ({
    signedIn: true,
    unmonitored: false,
    usage: {
      fetchedAt: new Date().toISOString(),
      stale: false,
      windows: [{ key: `session`, label: `Session`, percent: 12, resetsAt: null }],
    },
    ...over,
  })

  it(`reads numbers as ready, stale ones included`, () => {
    expect(usageState(row())).toBe(`ready`)
    expect(
      usageState(
        row({
          usage: {
            fetchedAt: `2020-01-01T00:00:00.000Z`,
            stale: true,
            windows: [
              { key: `session`, label: `Session`, percent: 3, resetsAt: null },
            ],
          },
        })
      )
    ).toBe(`ready`)
  })

  it(`reads a signed-in login with nothing read yet as checking`, () => {
    expect(usageState(row({ usage: null }))).toBe(`checking`)
    expect(
      usageState(
        row({
          usage: { fetchedAt: `2026-09-12T10:00:00.000Z`, stale: false, windows: [] },
        })
      )
    ).toBe(`checking`)
  })

  it(`keeps the machine's own "I collect nothing for this one" verdict`, () => {
    expect(usageState(row({ unmonitored: true, usage: null }))).toBe(
      `unmonitored`
    )
  })

  it(`says nothing at all for a signed-out login`, () => {
    expect(usageState(row({ signedIn: false }))).toBe(`none`)
    expect(usageState(row({ signedIn: false, unmonitored: true }))).toBe(`none`)
  })
})

// EXP-909: the logins listed UNDER one device row — the Devices page's fold.
// Same names, same test names ×4 (desktop `sort_device_logins` / `login_label`,
// iOS/Android `AgentAccountsRows`).
describe(`device logins (EXP-909)`, () => {
  const device = {
    deviceId: `macbook`,
    deviceLabel: `MacBook`,
    agentAccounts: {
      codex: {
        signedIn: true,
        email: `dev@acme.test`,
        profiles: [
          { id: `system`, signedIn: true, active: true, email: `dev@acme.test` },
        ],
      },
      claude: {
        signedIn: true,
        profiles: [
          {
            id: `work`,
            label: `Claude account 2`,
            signedIn: true,
            active: false,
            email: `work@acme.test`,
            plan: `max`,
          },
          {
            id: `system`,
            label: `Default`,
            signedIn: true,
            active: true,
            email: `dev@acme.test`,
            plan: `max`,
          },
          {
            id: `dead`,
            label: `Claude account 3`,
            signedIn: true,
            active: false,
            health: `needs_relogin` as const,
          },
        ],
      },
    },
    agentUsage: {},
    agentUsageAt: null,
  }

  it(`device logins lead with the active login, in contract agent order`, () => {
    const rows = sortDeviceLogins(
      deviceLoginRows(device, { mine: true, online: true })
    )
    expect(rows.map((row) => `${row.agent}:${row.profileId}`)).toEqual([
      // claude before codex (contract order), the machine's ACTIVE login
      // first, then the expired credential, then the healthy spare.
      `claude:system`,
      `claude:dead`,
      `claude:work`,
      `codex:system`,
    ])
    // The rows are one machine's: every one carries its label and verdicts.
    expect(rows.every((row) => row.deviceLabel === `MacBook`)).toBe(true)
    expect(rows.every((row) => row.mine && row.online)).toBe(true)
  })

  it(`the login label is the identity, never the status`, () => {
    expect(
      loginLabel({ email: `dev@acme.test`, plan: `max`, profileLabel: `Default` })
    ).toBe(`dev@acme.test`)
    expect(
      loginLabel({ email: null, plan: `openai-codex (oauth)`, profileLabel: `Default` })
    ).toBe(`openai-codex (oauth)`)
    expect(
      loginLabel({ email: null, plan: null, profileLabel: `Claude account 2` })
    ).toBe(`Claude account 2`)
  })

  it(`says a machine reported no login at all`, () => {
    expect(
      deviceLoginRows(
        { deviceId: `mint`, deviceLabel: `mint`, agentAccounts: {} },
        { mine: true, online: false }
      )
    ).toEqual([])
    expect(NO_LOGIN_REPORTED).toBe(`No login reported`)
  })
})
