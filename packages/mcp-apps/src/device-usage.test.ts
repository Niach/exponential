import {
  deviceLogins,
  deviceStatusText,
  deviceWorstHealth,
  healthBadgeLabel,
  loginName,
  miniWindows,
  platformLabel,
  severity,
  usageAge,
  usageState,
  type DeviceListRow,
  addAccountTarget,
  addableAgents,
  canSignInOn,
  loginLanded,
  loginSignsIn,
  parseLoginStep,
} from "./device-usage"

const NOW = Date.parse(`2026-10-05T12:00:00Z`)
const device = (extra: Partial<DeviceListRow> = {}): DeviceListRow => ({
  deviceId: `d1`,
  label: `mac`,
  ...extra,
})

describe(`deviceLogins`, () => {
  it(`lists every profile in contract agent order, active first, dead credential next`, () => {
    const logins = deviceLogins(
      device({
        agentAccounts: {
          codex: { signedIn: true, email: `c@x.test` },
          claude: {
            signedIn: true,
            profiles: [
              { id: `b`, signedIn: true, email: `b@x.test` },
              { id: `a`, signedIn: false, health: `needs_relogin`, email: `a@x.test` },
              { id: `system`, signedIn: true, active: true, email: `s@x.test` },
            ],
          },
          pi: { signedIn: true },
        },
        agentUsage: {
          claude: { windows: [{ key: `session`, label: `5h`, percent: 140 }] },
          codex: { windows: [{ key: `weekly`, label: `Week`, percent: 12.4 }], stale: true },
        },
      })
    )
    expect(logins.map((login) => login.key)).toEqual([
      `claude:system`,
      `claude:a`,
      `claude:b`,
      `codex:system`,
    ])
    // The active profile falls back to the pre-profile usage slot, clamped.
    expect(logins[0]!.usage?.windows[0]!.percent).toBe(100)
    expect(logins[1]!.health).toBe(`needs_relogin`)
    expect(logins[3]!.usage?.windows[0]!.percent).toBe(12)
    expect(deviceWorstHealth(logins)).toBe(`needs_relogin`)
  })

  it(`tolerates junk payloads`, () => {
    expect(deviceLogins(device({ agentAccounts: null }))).toEqual([])
    expect(
      deviceLogins(device({ agentAccounts: { claude: { profiles: [null, 3] } } as never }))
    ).toHaveLength(1)
  })
})

describe(`login presentation`, () => {
  it(`names and badges a login`, () => {
    expect(loginName({ email: null, plan: `pro` })).toBe(`pro`)
    expect(loginName({ email: null, plan: null })).toBe(`No email`)
    expect(healthBadgeLabel(`signed_out`)).toBe(`Signed out`)
    expect(healthBadgeLabel(`unknown`)).toBeNull()
  })

  it(`says what a login's numbers are`, () => {
    const [login] = deviceLogins(device({ agentAccounts: { claude: { signedIn: true } } }))
    expect(usageState(login!)).toBe(`checking`)
    expect(usageState({ ...login!, signedIn: false })).toBe(`none`)
    expect(usageState({ ...login!, unmonitored: true })).toBe(`unmonitored`)
  })

  it(`picks the mini windows`, () => {
    const windows = [
      { key: `model:fable`, label: `Fable`, percent: 1, resetsAt: null },
      { key: `weekly`, label: `Week`, percent: 2, resetsAt: null },
      { key: `credits`, label: `Credits`, percent: 3, resetsAt: null },
      { key: `session`, label: `5h`, percent: 4, resetsAt: null },
    ]
    expect(miniWindows({ windows }).map((w) => w.key)).toEqual([`session`, `weekly`, `model:fable`])
    expect(miniWindows({ windows: [windows[2]!] }).map((w) => w.key)).toEqual([`credits`])
  })

  it(`tones and ages usage`, () => {
    expect([severity(10), severity(75), severity(95)]).toEqual([`normal`, `warning`, `danger`])
    expect(usageAge({ windows: [], fetchedAt: `2026-10-05T11:55:00Z` }, NOW)).toBeNull()
    expect(usageAge({ windows: [], fetchedAt: `2026-10-05T11:00:00Z` }, NOW)).toBe(`as of 1h ago`)
    expect(usageAge({ windows: [], fetchedAt: `2026-10-05T11:55:00Z`, stale: true }, NOW)).toBe(
      `as of 5m ago`
    )
  })

  it(`writes the device status line and platform`, () => {
    expect(deviceStatusText(device({ online: true }), NOW)).toBe(`Online`)
    expect(deviceStatusText(device({ lastSeenAt: `2026-10-05T10:00:00Z` }), NOW)).toBe(
      `Last seen 2h ago`
    )
    expect(deviceStatusText(device(), NOW)).toBe(`Offline`)
    expect(platformLabel(`macos`)).toBe(`macOS`)
    expect(platformLabel(`freebsd`)).toBe(`freebsd`)
  })
})

describe(`sign-in rules (EXP-1199, agent-account-add.ts / device-agent-account.tsx)`, () => {
  const own = { deviceId: `d`, online: true, caps: [`agent-login`], agents: [`claude`], unauthedAgents: [`codex`] }

  it(`signs in only on your own online machine with the cap`, () => {
    expect(canSignInOn(own)).toBe(true)
    expect(canSignInOn({ ...own, online: false })).toBe(false)
    expect(canSignInOn({ ...own, caps: [] })).toBe(false)
    expect(canSignInOn({ ...own, owner: { id: `u`, name: `Ann` } })).toBe(false)
    expect(addableAgents(own)).toEqual([`claude`, `codex`])
  })

  it(`lands a new login on the free ambient one, else the next free label`, () => {
    expect(addAccountTarget({ ...own, agentAccounts: { codex: { signedIn: false } } }, `codex`)).toEqual({
      profileId: `system`,
    })
    expect(
      addAccountTarget(
        {
          ...own,
          agentAccounts: {
            claude: {
              signedIn: true,
              profiles: [
                { id: `system`, signedIn: true },
                { id: `a`, label: `Claude Code account 2`, signedIn: true },
              ],
            },
          },
        },
        `claude`
      )
    ).toEqual({ name: `Claude Code account 3` })
  })

  it(`offers Sign in on a signed-out login or a dead credential`, () => {
    expect(loginSignsIn({ signedIn: false, health: `signed_out` })).toBe(true)
    expect(loginSignsIn({ signedIn: true, health: `needs_relogin` })).toBe(true)
    expect(loginSignsIn({ signedIn: true, health: `ok` })).toBe(false)
  })

  it(`lands only when the TARGETED login is signed in and healthy`, () => {
    const device = {
      ...own,
      agentAccounts: {
        claude: {
          signedIn: true,
          profiles: [
            { id: `system`, signedIn: true },
            { id: `p`, label: `Work`, signedIn: true, health: `needs_relogin` },
          ],
        },
      },
    }
    expect(loginLanded(device, `claude`, { name: `Work` })).toBe(false)
    expect(loginLanded(device, `claude`, { profileId: `p` })).toBe(false)
    expect(loginLanded(device, `claude`, { profileId: `system` })).toBe(true)
    expect(loginLanded(device, `claude`, { name: `Other` })).toBe(false)
    expect(loginLanded({ ...own, agentAccounts: { codex: { signedIn: true } } }, `codex`, { profileId: `system` })).toBe(true)
  })

  it(`parses the tool's steps tolerantly`, () => {
    expect(parseLoginStep({ status: `url`, commandId: `c`, url: `https://x`, code: `AB-CD` })).toEqual({
      status: `url`,
      commandId: `c`,
      url: `https://x`,
      code: `AB-CD`,
    })
    expect(parseLoginStep({ status: `pending`, commandId: `c` })).toEqual({ status: `pending`, commandId: `c` })
    expect(parseLoginStep({ status: `failed` })).toEqual({ status: `failed`, message: `The device reported a failure.` })
    expect(parseLoginStep({ status: `url` })).toBeNull()
    expect(parseLoginStep(`nope`)).toBeNull()
  })
})
