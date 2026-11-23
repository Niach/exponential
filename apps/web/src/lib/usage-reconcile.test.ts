import { describe, expect, it } from "vitest"
import type { DeviceAgentAccounts, DeviceAgentUsage, DeviceAgentUsageMap } from "@/db/schema"
import {
  reconcileDeviceUsage,
  usageIsStale,
  USAGE_STALE_MS,
} from "@/lib/usage-reconcile"

// #FEED-304, the incident's own rows as the devices list returned them at
// 2026-11-23 ~19:26 UTC.
const NOW = new Date(`2026-11-23T19:26:00.000Z`)

const usage = (
  fetchedAt: string,
  week: number,
  fable: number,
  over: Partial<DeviceAgentUsage> = {}
): DeviceAgentUsage => ({
  fetchedAt,
  stale: false,
  windows: [
    { key: `weekly`, label: `Week`, percent: week, resetsAt: `2026-11-24T19:00:00.000Z` },
    { key: `model:fable`, label: `Fable`, percent: fable, resetsAt: `2026-11-24T18:59:59.860Z` },
  ],
  ...over,
})

const DENNIS = {
  ...usage(`2026-11-23T19:24:47.000Z`, 40, 5),
  windows: [
    { key: `weekly`, label: `Week`, percent: 40, resetsAt: `2026-11-29T21:59:59.614Z` },
  ],
}

const macbook = (): { id: string; agentAccounts: DeviceAgentAccounts; agentUsage: DeviceAgentUsageMap } => ({
  id: `macbook`,
  agentAccounts: {
    claude: {
      signedIn: true,
      email: `dennis@example.com`,
      profiles: [
        { id: `098c5a75`, signedIn: true, active: true, plan: `max`, email: `dennis@example.com`, usage: DENNIS },
        {
          id: `b4bc56c4`,
          signedIn: true,
          plan: `max`,
          email: `danny@yourev.example`,
          usage: usage(`2026-11-22T22:00:28.000Z`, 100, 100),
        },
      ],
    },
  },
  agentUsage: { claude: DENNIS },
})

const mint = (): { id: string; agentAccounts: DeviceAgentAccounts; agentUsage: DeviceAgentUsageMap } => ({
  id: `mint`,
  agentAccounts: {
    claude: {
      signedIn: true,
      email: `danny@yourev.example`,
      profiles: [
        {
          id: `3e1b7e83`,
          signedIn: true,
          active: true,
          plan: `max`,
          email: `Danny@Yourev.example`,
          usage: usage(`2026-11-23T19:20:08.000Z`, 62, 19),
        },
      ],
    },
  },
  agentUsage: { claude: usage(`2026-11-23T19:20:08.000Z`, 62, 19) },
})

const profile = (state: { agentAccounts: DeviceAgentAccounts | null }, id: string) =>
  state.agentAccounts!.claude!.profiles!.find((entry) => entry.id === id)!

describe(`usageIsStale (#FEED-304)`, () => {
  it(`a 21-hour-old fetch is stale whatever the device said`, () => {
    expect(usageIsStale(usage(`2026-11-22T22:00:28.000Z`, 100, 100), NOW)).toBe(true)
  })

  it(`a recent fetch is current, up to the threshold`, () => {
    expect(usageIsStale(usage(`2026-11-23T19:20:08.000Z`, 62, 19), NOW)).toBe(false)
    const edge = new Date(NOW.getTime() - USAGE_STALE_MS)
    expect(usageIsStale(usage(new Date(edge.getTime() + 1).toISOString(), 1, 1), NOW)).toBe(false)
    expect(usageIsStale(usage(edge.toISOString(), 1, 1), NOW)).toBe(true)
  })

  it(`a window past its reset is stale, however fresh the fetch`, () => {
    const afterReset = new Date(`2026-11-24T19:00:30.000Z`)
    expect(usageIsStale(usage(`2026-11-24T18:59:00.000Z`, 100, 100), afterReset)).toBe(true)
  })

  it(`the device's own flag and an unreadable stamp are stale too`, () => {
    expect(usageIsStale(usage(`2026-11-23T19:20:08.000Z`, 62, 19, { stale: true }), NOW)).toBe(true)
    expect(usageIsStale({ windows: [] }, NOW)).toBe(true)
    expect(usageIsStale(null, NOW)).toBe(false)
  })
})

describe(`reconcileDeviceUsage (#FEED-304)`, () => {
  it(`the same account on two devices reads the newest fetch on both`, () => {
    const [mac, other] = reconcileDeviceUsage([macbook(), mint()], NOW)
    expect(mac!.changed).toBe(true)
    expect(profile(mac!, `b4bc56c4`).usage).toEqual(usage(`2026-11-23T19:20:08.000Z`, 62, 19))
    // The other login and the top-level map (the active login's) stand.
    expect(profile(mac!, `098c5a75`).usage).toEqual(DENNIS)
    expect(mac!.agentUsage).toEqual({ claude: DENNIS })
    expect(other!.changed).toBe(false)
    expect(other!.agentAccounts).toBe(other!.row.agentAccounts)
  })

  it(`is a fixed point: a second pass changes nothing`, () => {
    const first = reconcileDeviceUsage([macbook(), mint()], NOW)
    const second = reconcileDeviceUsage(
      first.map((entry) => ({ id: entry.row.id, agentAccounts: entry.agentAccounts, agentUsage: entry.agentUsage })),
      NOW
    )
    expect(second.map((entry) => entry.changed)).toEqual([false, false])
  })

  it(`the active login's adopted numbers move the top-level map too`, () => {
    const stuck = mint()
    stuck.agentAccounts.claude!.profiles![0]!.usage = usage(`2026-11-22T21:29:36.000Z`, 100, 100)
    stuck.agentUsage = { claude: usage(`2026-11-22T21:29:36.000Z`, 100, 100) }
    const fresh = macbook()
    profile(fresh, `b4bc56c4`).usage = usage(`2026-11-23T19:20:08.000Z`, 62, 19)
    const [first] = reconcileDeviceUsage([stuck, fresh], NOW)
    expect(profile(first!, `3e1b7e83`).usage?.windows?.[0]?.percent).toBe(62)
    expect(first!.agentUsage!.claude).toEqual(usage(`2026-11-23T19:20:08.000Z`, 62, 19))
  })

  it(`with no newer fetch anywhere, the old numbers are marked stale`, () => {
    const [mac] = reconcileDeviceUsage([macbook()], NOW)
    expect(mac!.changed).toBe(true)
    expect(profile(mac!, `b4bc56c4`).usage).toEqual(
      usage(`2026-11-22T22:00:28.000Z`, 100, 100, { stale: true })
    )
    expect(profile(mac!, `098c5a75`).usage?.stale).toBe(false)
  })

  it(`never crosses accounts, plans, signed-out or unmonitored logins`, () => {
    const cases: Array<(row: ReturnType<typeof macbook>) => void> = [
      (row) => (profile(row, `b4bc56c4`).email = `someone@else.example`),
      (row) => (profile(row, `b4bc56c4`).plan = `pro`),
      (row) => (profile(row, `b4bc56c4`).signedIn = false),
      (row) => (profile(row, `b4bc56c4`).unmonitored = true),
    ]
    for (const mutate of cases) {
      const row = macbook()
      mutate(row)
      const [mac] = reconcileDeviceUsage([row, mint()], NOW)
      expect(profile(mac!, `b4bc56c4`).usage?.windows?.[0]?.percent).toBe(100)
      expect(profile(mac!, `b4bc56c4`).usage?.stale).toBe(true)
    }
  })

  it(`a clock running ahead never outranks a real read`, () => {
    const ahead = mint()
    ahead.agentAccounts.claude!.profiles![0]!.usage = usage(`2026-11-23T23:00:00.000Z`, 1, 1)
    const row = macbook()
    profile(row, `b4bc56c4`).usage = usage(`2026-11-23T19:20:08.000Z`, 62, 19)
    const [mac] = reconcileDeviceUsage([row, ahead], NOW)
    expect(mac!.changed).toBe(false)
  })

  it(`a login with no numbers yet takes the account's newest`, () => {
    const row = macbook()
    delete profile(row, `b4bc56c4`).usage
    const [mac] = reconcileDeviceUsage([row, mint()], NOW)
    expect(profile(mac!, `b4bc56c4`).usage?.windows?.[0]?.percent).toBe(62)
  })

  it(`a device that reports no profiles still ages`, () => {
    const [old] = reconcileDeviceUsage(
      [{ agentAccounts: null, agentUsage: { claude: usage(`2026-11-12T05:33:13.000Z`, 38, 0) } }],
      NOW
    )
    expect(old!.changed).toBe(true)
    expect(old!.agentUsage!.claude!.stale).toBe(true)
  })
})
