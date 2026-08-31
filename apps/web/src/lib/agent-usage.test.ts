import { describe, expect, it } from "vitest"
import {
  accountCaption,
  accountLine,
  accountRow,
  formatResetCountdown,
  parseAgentLoginResult,
  parseAgentUsage,
  parseAgentUsageMap,
  sessionAgentUsage,
  severity,
  usageGroups,
  usageIsFresh,
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

  it(`renders a plan-only account (pi reports a provider, no email)`, () => {
    expect(accountCaption({ signedIn: true, plan: `anthropic (oauth)` })).toBe(
      `anthropic (oauth)`
    )
  })

  it(`degrades to signed in, then to unknown`, () => {
    expect(accountCaption({ signedIn: true })).toBe(`signed in`)
    expect(accountCaption(null)).toBe(`unknown`)
  })
})

describe(`accountLine`, () => {
  it(`is the email alone — no prefix, no plan tail`, () => {
    expect(
      accountLine({ signedIn: true, email: `danny@example.com`, plan: `Max` })
    ).toBe(`danny@example.com`)
    expect(accountLine({ signedIn: true, email: `danny@example.com` })).toBe(
      `danny@example.com`
    )
  })

  it(`falls back to the bare plan without an email (pi)`, () => {
    expect(accountLine({ signedIn: true, plan: `anthropic (oauth)` })).toBe(
      `anthropic (oauth)`
    )
    expect(accountLine({ signedIn: true })).toBe(`signed in`)
  })

  it(`spells out the negatives`, () => {
    expect(accountLine({ signedIn: false })).toBe(`Not signed in`)
    expect(accountLine(null)).toBe(`Sign-in status unknown`)
  })
})

describe(`accountRow`, () => {
  it(`prefixes the agent`, () => {
    expect(
      accountRow(`claude`, {
        signedIn: true,
        email: `danny@example.com`,
        plan: `Max`,
      })
    ).toBe(`claude · danny@example.com`)
    expect(accountRow(`codex`, { signedIn: false })).toBe(`codex · signed out`)
    expect(accountRow(`pi`, null)).toBe(`pi · unknown`)
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
