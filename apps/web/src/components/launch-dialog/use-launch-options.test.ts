import { act, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { useLaunchOptions } from "@/components/launch-dialog/use-launch-options"
import type { SteerDevice } from "@/lib/steer-devices"

// EXP-772: plan mode is a per-surface DEFAULT, not just a hidden row. A
// surface that starts in build mode (the chat page) must also SEND
// `planMode: false` — the old `planModeHidden` prop hid the switch while the
// device's advertised default still rode out with the start.

const device: SteerDevice = {
  deviceId: `dev-1`,
  deviceLabel: `buildbox`,
  agents: [`claude`],
  online: true,
  launchDefaults: {
    defaultAgent: `claude`,
    agents: { claude: { planMode: true } },
  },
}

describe(`useLaunchOptions plan mode`, () => {
  it(`seeds the device's plan default by default`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [device] })
    )
    expect(result.current.planMode).toBe(true)
    expect(result.current.buildOptions().planMode).toBe(true)
  })

  it(`planModeOff starts in build mode and sends false`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [device], planModeOff: true })
    )
    expect(result.current.planMode).toBe(false)
    expect(result.current.buildOptions().planMode).toBe(false)

    // The user can still flip it — the default is a default, not a lock.
    act(() => result.current.setPlanMode(true))
    expect(result.current.buildOptions().planMode).toBe(true)
  })
})

// EXP-773: the PTY fallback is gone, so an agent outside the settled device's
// reported ACP set has nowhere to start — every launch surface blocks submit
// on this flag.
describe(`useLaunchOptions readiness`, () => {
  it(`is false while the device reports no ACP set`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [device] })
    )
    expect(result.current.agentNotReady).toBe(false)
  })

  it(`is true when the settled agent is outside the reported set`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({
        open: true,
        devices: [{ ...device, acpAgents: [`codex`] }],
      })
    )
    expect(result.current.agent).toBe(`claude`)
    expect(result.current.agentNotReady).toBe(true)
  })
})

// EXP-825 (EXP-747 B7): the ⋯ popover's Account picker — the agent profiles
// the settled device reports, the active one seeding the pick, and only a
// NAMED, non-system profile riding out as `account`.
describe(`useLaunchOptions account`, () => {
  const profiled: SteerDevice = {
    ...device,
    agents: [`claude`, `codex`],
    agentAccounts: {
      claude: {
        signedIn: true,
        profiles: [
          { id: `work`, label: `Work`, signedIn: true },
          { id: `system`, signedIn: true, active: true },
        ],
      },
      codex: {
        signedIn: true,
        profiles: [{ id: `only`, label: `Only`, signedIn: true, active: true }],
      },
    },
  }

  it(`lists the device's profiles active-first and seeds the active one`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [profiled] })
    )
    expect(result.current.accountProfiles.map((p) => p.id)).toEqual([
      `system`,
      `work`,
    ])
    expect(result.current.accountProfiles[0]!.label).toBe(`Default`)
    expect(result.current.account).toBe(`system`)
    // The ambient login is the server's default — nothing rides out.
    expect(result.current.buildOptions().account).toBeUndefined()
  })

  it(`emits a named profile and reseeds on an agent switch`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [profiled] })
    )
    act(() => result.current.setAccount(`work`))
    expect(result.current.buildOptions().account).toBe(`work`)

    act(() => result.current.switchAgent(`codex`))
    expect(result.current.accountProfiles.map((p) => p.id)).toEqual([`only`])
    expect(result.current.account).toBe(`only`)
    expect(result.current.buildOptions().account).toBe(`only`)
  })

  it(`sends nothing for a device that reports no profiles`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [device] })
    )
    expect(result.current.accountProfiles).toEqual([])
    expect(result.current.account).toBeUndefined()
    // A stale pick for a profile the machine does not have never rides out.
    act(() => result.current.setAccount(`ghost`))
    expect(result.current.buildOptions().account).toBeUndefined()
  })
})
