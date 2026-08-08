// EXP-420: the machine-row Update button must render only when a newer CLI
// version actually exists (or an update is already in flight) — not for
// every online server.
import { describe, expect, it } from "vitest"

import {
  deviceAgentLaunchDefaults,
  deviceDefaultAgent,
  deviceIsMine,
  deviceUpdateAvailable,
  showDeviceUpdateButton,
  type SteerDevice,
} from "./steer-devices"
import { agentSeed } from "./coding-launch-prefs"

function server(overrides: Partial<SteerDevice> = {}): SteerDevice {
  return {
    deviceId: `dev-1`,
    deviceLabel: `homelab`,
    kind: `server`,
    online: true,
    registered: true,
    version: `0.14.1`,
    ...overrides,
  }
}

describe(`deviceUpdateAvailable`, () => {
  it(`compares numerically, not lexicographically`, () => {
    expect(deviceUpdateAvailable(`0.9.9`, `0.10.0`)).toBe(true)
    expect(deviceUpdateAvailable(`0.14.1`, `0.14.2`)).toBe(true)
    expect(deviceUpdateAvailable(`0.14.2`, `0.14.2`)).toBe(false)
    expect(deviceUpdateAvailable(`0.15.0`, `0.14.2`)).toBe(false)
  })

  it(`fails closed on unknown or unparsable versions`, () => {
    expect(deviceUpdateAvailable(null, `0.14.2`)).toBe(false)
    expect(deviceUpdateAvailable(`0.14.1`, null)).toBe(false)
    expect(deviceUpdateAvailable(undefined, undefined)).toBe(false)
    expect(deviceUpdateAvailable(`not-a-version`, `0.14.2`)).toBe(false)
  })
})

describe(`showDeviceUpdateButton`, () => {
  it(`shows only for an outdated online registered server`, () => {
    expect(showDeviceUpdateButton(server(), `0.14.2`)).toBe(true)
  })

  it(`hides when the device is current`, () => {
    expect(showDeviceUpdateButton(server({ version: `0.14.2` }), `0.14.2`)).toBe(
      false
    )
  })

  it(`hides when the latest version is unknown (env unset)`, () => {
    expect(showDeviceUpdateButton(server(), null)).toBe(false)
    expect(showDeviceUpdateButton(server(), undefined)).toBe(false)
  })

  it(`hides for desktops, offline and unregistered rows`, () => {
    expect(showDeviceUpdateButton(server({ kind: `desktop` }), `0.14.2`)).toBe(
      false
    )
    expect(showDeviceUpdateButton(server({ online: false }), `0.14.2`)).toBe(
      false
    )
    expect(
      showDeviceUpdateButton(server({ registered: false }), `0.14.2`)
    ).toBe(false)
    expect(
      showDeviceUpdateButton(server({ registered: undefined }), `0.14.2`)
    ).toBe(false)
  })

  it(`keeps an in-flight update visible even once versions look current`, () => {
    // The daemon's re-register clears updateRequested and carries the new
    // version — until then the row must keep saying "Updating…"/"Queued".
    expect(
      showDeviceUpdateButton(
        server({ version: `0.14.2`, updateRequested: true }),
        `0.14.2`
      )
    ).toBe(true)
    expect(
      showDeviceUpdateButton(server({ updateRequested: true }), null)
    ).toBe(true)
  })
})

// EXP-437: the Start-coding dialog seeds from the selected device's
// advertised per-agent launch defaults.
describe(`device launch defaults`, () => {
  const advertising = server({
    agents: [`claude`, `codex`],
    launchDefaults: {
      defaultAgent: `claude`,
      agents: {
        claude: { model: `opus`, effort: ``, planMode: true },
        codex: { model: ``, effort: `high`, skipPermissions: true },
      },
    },
  })

  it(`resolves the default agent only when the device can run it`, () => {
    expect(deviceDefaultAgent(advertising)).toBe(`claude`)
    // Configured default not runnable there → null (caller falls back).
    expect(
      deviceDefaultAgent(
        server({
          agents: [`codex`],
          launchDefaults: { defaultAgent: `pi`, agents: {} },
        })
      )
    ).toBe(null)
    // No advertisement (old desktop) → null.
    expect(deviceDefaultAgent(server())).toBe(null)
    expect(deviceDefaultAgent(undefined)).toBe(null)
  })

  it(`returns the per-agent entry or null`, () => {
    expect(deviceAgentLaunchDefaults(advertising, `codex`)).toEqual({
      model: ``,
      effort: `high`,
      skipPermissions: true,
    })
    expect(deviceAgentLaunchDefaults(advertising, `pi`)).toBe(null)
    expect(deviceAgentLaunchDefaults(server(), `claude`)).toBe(null)
  })

  it(`agentSeed validates against the contract and capability-clamps`, () => {
    // The advertised values ride through; blank effort stays blank.
    expect(agentSeed(`claude`, { model: `opus`, effort: ``, planMode: true })).toEqual(
      { model: `opus`, effort: ``, ultracode: false, planMode: true, skipPermissions: false }
    )
    // Blank model is valid for codex; skip rides through.
    expect(
      agentSeed(`codex`, { model: ``, effort: `high`, skipPermissions: true })
    ).toEqual({
      model: ``,
      effort: `high`,
      ultracode: false,
      planMode: false,
      skipPermissions: true,
    })
    // A foreign/unknown value falls back to the static default; claude never
    // takes a blank model.
    expect(agentSeed(`claude`, { model: ``, effort: `warp9` })).toEqual({
      model: `fable`,
      effort: ``,
      ultracode: false,
      planMode: false,
      skipPermissions: false,
    })
    // Capability masking beats a lying advertisement: pi never skips, codex
    // never plans.
    expect(
      agentSeed(`pi`, { skipPermissions: true, planMode: true, ultracode: true })
    ).toEqual({
      model: ``,
      effort: ``,
      ultracode: false,
      planMode: false,
      skipPermissions: false,
    })
    // `null` = the static fallback for devices that advertise nothing.
    expect(agentSeed(`claude`, null)).toEqual({
      model: `fable`,
      effort: ``,
      ultracode: false,
      planMode: false,
      skipPermissions: false,
    })
  })
})

// EXP-432: teammates' shared rows carry `owner`; own rows never do.
describe(`deviceIsMine`, () => {
  it(`is true for own rows (owner absent) and false for shared rows`, () => {
    expect(deviceIsMine(server())).toBe(true)
    expect(deviceIsMine(server({ sharedTeamId: `team-1` }))).toBe(true)
    expect(
      deviceIsMine(
        server({
          sharedTeamId: `team-1`,
          owner: { id: `owner-1`, name: `Tessa` },
        })
      )
    ).toBe(false)
  })
})
