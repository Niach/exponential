import { describe, expect, it } from "vitest"
import type { Device } from "@/db/schema"
import {
  addAccountLoginTarget,
  addableAgents,
  agentInstalledOn,
  nextProfileLabel,
} from "@/lib/agent-account-add"

// EXP-827: the Add-account rules — which agents a machine offers, and whether
// a login lands in the ambient slot or a new profile. EXP-909 dropped the
// cross-device half (`addAccountDevices` / `addAccountBlockReason`): the flow
// is opened from a row under ONE machine now, so there is no machine to pick
// and no reason to explain.

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
  function withProfiles(labels: string[]): Device {
    return device({
      agentAccounts: {
        claude: {
          signedIn: true,
          profiles: [
            { id: `system`, label: `Default`, signedIn: true },
            ...labels.map((label, i) => ({ id: `p${i + 1}`, label, signedIn: true })),
          ],
        },
      },
    })
  }

  it(`starts at 2 with only the ambient login`, () => {
    expect(nextProfileLabel(device({}), `claude`, `Claude`)).toBe(`Claude account 2`)
    expect(nextProfileLabel(withProfiles([]), `claude`, `Claude`)).toBe(`Claude account 2`)
  })

  it(`skips the numbers still in use`, () => {
    expect(
      nextProfileLabel(withProfiles([`Claude account 2`, `Claude account 3`]), `claude`, `Claude`)
    ).toBe(`Claude account 4`)
  })

  it(`reuses a gap left by a removed profile instead of re-issuing a taken label`, () => {
    // "account 2" was removed: counting would say "account 3", which is
    // already a usable profile, and the sign-in sheet would land at open.
    expect(nextProfileLabel(withProfiles([`Claude account 3`]), `claude`, `Claude`)).toBe(
      `Claude account 2`
    )
  })

  it(`matches labels exactly — case and agent name included`, () => {
    expect(
      nextProfileLabel(withProfiles([`claude account 2`, `Codex account 2`]), `claude`, `Claude`)
    ).toBe(`Claude account 2`)
  })
})
