import { describe, expect, it } from "vitest"
import type { Device } from "@/db/schema"
import {
  addAccountBlockReason,
  addAccountDevices,
  addAccountLoginTarget,
  addableAgents,
  agentInstalledOn,
  nextProfileLabel,
} from "@/lib/agent-account-add"

// EXP-827: the Add-account rules — which machines and agents the flow
// offers, and whether a login lands in the ambient slot or a new profile.

const now = new Date(`2026-09-11T10:00:00Z`)

function device(overrides: Partial<Device>): Device {
  return {
    id: `row-${overrides.deviceId ?? `x`}`,
    deviceId: `dev-1`,
    userId: `me`,
    label: `Studio`,
    kind: `desktop`,
    agents: [`claude`],
    unauthedAgents: [],
    caps: [`agent-login`],
    lastSeenAt: now,
    agentAccounts: {},
    ...overrides,
  } as unknown as Device
}

describe(`agentInstalledOn / addableAgents`, () => {
  it(`counts runnable and signed-out agents, deduped`, () => {
    const row = device({ agents: [`claude`, `codex`], unauthedAgents: [`codex`] })
    expect(agentInstalledOn(row, `claude`)).toBe(true)
    expect(agentInstalledOn(row, `codex`)).toBe(true)
    expect(agentInstalledOn(row, `nope`)).toBe(false)
    // EXP-849: every installed agent is addable — each one has a device-code
    // sign-in now.
    expect(addableAgents(row)).toEqual([`claude`, `codex`])
  })
})

describe(`addAccountDevices`, () => {
  it(`keeps own, online, capable machines with the agent installed`, () => {
    const rows = [
      device({ deviceId: `mine-online` }),
      device({ deviceId: `mine-offline`, lastSeenAt: new Date(`2026-09-11T09:00:00Z`) }),
      device({ deviceId: `no-cap`, caps: [] }),
      device({ deviceId: `theirs`, userId: `other` }),
      device({ deviceId: `codex-only`, agents: [`codex`] }),
      device({ deviceId: `codex-signed-out`, agents: [], unauthedAgents: [`codex`] }),
    ]
    expect(
      addAccountDevices(rows, { currentUserId: `me`, now, agent: `claude` }).map(
        (row) => row.deviceId
      )
    ).toEqual([`mine-online`])
    expect(
      addAccountDevices(rows, { currentUserId: `me`, now, agent: `codex` }).map(
        (row) => row.deviceId
      )
    ).toEqual([`codex-only`, `codex-signed-out`])
    // Without an agent: any machine that can take some login.
    expect(
      addAccountDevices(rows, { currentUserId: `me`, now }).map((row) => row.deviceId)
    ).toEqual([`mine-online`, `codex-only`, `codex-signed-out`])
  })

  it(`skips the machines already holding the account`, () => {
    const rows = [device({ deviceId: `a` }), device({ deviceId: `b` })]
    expect(
      addAccountDevices(rows, {
        currentUserId: `me`,
        now,
        agent: `claude`,
        exclude: [`a`],
      }).map((row) => row.deviceId)
    ).toEqual([`b`])
  })
})

describe(`addAccountLoginTarget`, () => {
  it(`uses the ambient login while it is signed out`, () => {
    expect(
      addAccountLoginTarget(device({ agentAccounts: {} }), `claude`, `X`)
    ).toEqual({ profileId: `system` })
    expect(
      addAccountLoginTarget(
        device({ agentAccounts: { claude: { signedIn: false } } }),
        `claude`,
        `X`
      )
    ).toEqual({ profileId: `system` })
    expect(
      addAccountLoginTarget(
        device({
          agentAccounts: {
            claude: {
              signedIn: true,
              profiles: [
                { id: `system`, signedIn: false },
                { id: `p1`, signedIn: true, active: true },
              ],
            },
          },
        }),
        `claude`,
        `X`
      )
    ).toEqual({ profileId: `system` })
  })

  it(`creates a new profile once the ambient login is taken, clamped`, () => {
    const row = device({
      agentAccounts: { claude: { signedIn: true, email: `a@b.c` } },
    })
    expect(addAccountLoginTarget(row, `claude`, `  a@b.c `)).toEqual({
      newProfileLabel: `a@b.c`,
    })
    const long = `x`.repeat(80)
    expect(addAccountLoginTarget(row, `claude`, long).newProfileLabel).toHaveLength(64)
  })
})

describe(`nextProfileLabel`, () => {
  it(`numbers one past the reported profiles`, () => {
    expect(nextProfileLabel(device({}), `claude`, `Claude`)).toBe(`Claude account 2`)
    expect(
      nextProfileLabel(
        device({
          agentAccounts: {
            claude: {
              signedIn: true,
              profiles: [
                { id: `system`, signedIn: true },
                { id: `p1`, signedIn: true },
                { id: `p2`, signedIn: false },
              ],
            },
          },
        }),
        `claude`,
        `Claude`
      )
    ).toBe(`Claude account 4`)
  })
})

// EXP-845: the controls never hide — they say why they are disabled.
describe(`addAccountBlockReason`, () => {
  it(`is null while a machine can take the sign-in`, () => {
    expect(
      addAccountBlockReason([device({ deviceId: `mine` })], {
        currentUserId: `me`,
        now,
      })
    ).toBeNull()
  })

  it(`names the first filter that emptied the list`, () => {
    expect(addAccountBlockReason([], { currentUserId: `me`, now })).toBe(
      `Connect one of your devices first.`
    )
    // Somebody else's machine is not mine to sign in on.
    expect(
      addAccountBlockReason([device({ deviceId: `theirs`, userId: `other` })], {
        currentUserId: `me`,
        now,
      })
    ).toBe(`Connect one of your devices first.`)
    // The named agent is installed nowhere.
    expect(
      addAccountBlockReason([device({ deviceId: `mine`, agents: [`claude`] })], {
        currentUserId: `me`,
        now,
        agent: `codex`,
        agentLabel: `Codex`,
      })
    ).toBe(`No device of yours has Codex installed.`)
    // The one machine that has it is offline.
    expect(
      addAccountBlockReason(
        [
          device({
            deviceId: `studio`,
            label: `Studio`,
            lastSeenAt: new Date(`2026-09-11T09:00:00Z`),
          }),
        ],
        { currentUserId: `me`, now, agent: `claude` }
      )
    ).toBe(`Studio is offline.`)
    // Two of them: no single machine to name.
    expect(
      addAccountBlockReason(
        [
          device({ deviceId: `a`, lastSeenAt: new Date(`2026-09-11T09:00:00Z`) }),
          device({ deviceId: `b`, lastSeenAt: new Date(`2026-09-11T09:00:00Z`) }),
        ],
        { currentUserId: `me`, now }
      )
    ).toBe(`None of those devices is online right now.`)
    // Online and installed, but the build cannot be driven remotely.
    expect(
      addAccountBlockReason([device({ deviceId: `old`, caps: [] })], {
        currentUserId: `me`,
        now,
      })
    ).toBe(`Remote sign-in needs a newer Exponential version on that device.`)
    // Every machine already holds the account.
    expect(
      addAccountBlockReason([device({ deviceId: `mine` })], {
        currentUserId: `me`,
        now,
        exclude: [`mine`],
      })
    ).toBe(`Every device of yours already uses this account.`)
  })
})
