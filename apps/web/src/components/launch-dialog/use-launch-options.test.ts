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

  // EXP-1196: a doctor report decides over the ACP set.
  const doctor = (git: string, claude: string): SteerDevice[`doctor`] => ({
    checkedAt: `2026-10-05T19:00:00.000Z`,
    items: [
      { key: `git`, group: `required`, state: git },
      { key: `claude`, group: `agents`, state: claude },
    ],
  })

  it(`uses the doctor when present: the picked agent's row not ok blocks`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({
        open: true,
        // The ACP set says yes; the doctor says the agent needs an update.
        devices: [{ ...device, acpAgents: [`claude`], doctor: doctor(`ok`, `action`) }],
      })
    )
    expect(result.current.agentNotReady).toBe(true)
  })

  it(`uses the doctor when present: Git not ok blocks`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({
        open: true,
        devices: [{ ...device, doctor: doctor(`error`, `ok`) }],
      })
    )
    expect(result.current.agentNotReady).toBe(true)
  })

  it(`uses the doctor when present: Git + agent ok starts, whatever the ACP set`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({
        open: true,
        devices: [{ ...device, acpAgents: [`codex`], doctor: doctor(`ok`, `ok`) }],
      })
    )
    expect(result.current.agentNotReady).toBe(false)
  })
})

// EXP-872: ONE account picker — the flattened logins of the settled device
// (both agents, email-labelled, the last used one first); a pick implies
// the agent. EXP-1158: the picked option's id rides out as `account`
// VERBATIM, `system` included (it names the ambient login).
describe(`useLaunchOptions account`, () => {
  const profiled: SteerDevice = {
    ...device,
    agents: [`claude`, `codex`],
    launchDefaults: { defaultAgent: `claude` },
    agentAccounts: {
      claude: {
        signedIn: true,
        profiles: [
          { id: `work`, label: `Work`, signedIn: true, email: `work@x.test` },
          { id: `system`, signedIn: true, active: true, email: `me@x.test` },
        ],
      },
      codex: {
        signedIn: true,
        profiles: [
          { id: `only`, label: `Only`, signedIn: true, active: true, email: `codex@x.test` },
        ],
      },
    },
  }

  it(`lists every login last-used-first, by email, and seeds the last used one`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [profiled] })
    )
    expect(result.current.accountOptions.map((o) => `${o.agent}:${o.id}`)).toEqual([
      `claude:system`,
      `claude:work`,
      `codex:only`,
    ])
    expect(result.current.accountOptions.map((o) => o.email)).toEqual([
      `me@x.test`,
      `work@x.test`,
      `codex@x.test`,
    ])
    expect(result.current.accountKey).toBe(`claude:system`)
    expect(result.current.agent).toBe(`claude`)
    // The ambient login rides out by NAME: an absent account would mean
    // "the last used one", which may differ by the time the device starts.
    expect(result.current.buildOptions().account).toBe(`system`)
  })

  it(`a pick implies the agent and emits a named profile`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [profiled] })
    )
    act(() => result.current.setAccountKey(`claude:work`))
    expect(result.current.buildOptions().account).toBe(`work`)
    expect(result.current.buildOptions().agent).toBe(`claude`)

    act(() => result.current.setAccountKey(`codex:only`))
    expect(result.current.agent).toBe(`codex`)
    expect(result.current.buildOptions().agent).toBe(`codex`)
    expect(result.current.buildOptions().account).toBe(`only`)
  })

  it(`falls back to one ambient option per runnable agent when the device reports no login`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [device] })
    )
    expect(result.current.accountOptions).toEqual([
      {
        id: `system`,
        agent: `claude`,
        email: `Claude Code`,
        isLastUsed: true,
        health: `unknown`,
      },
    ])
    expect(result.current.accountKey).toBe(`claude:system`)
    expect(result.current.buildOptions().account).toBe(`system`)
  })

  it(`re-seeds to the last used login when the pick vanishes with a device switch`, () => {
    const other: SteerDevice = {
      ...profiled,
      deviceId: `dev-2`,
      launchDefaults: { defaultAgent: `codex` },
    }
    const { result } = renderHook(() =>
      useLaunchOptions({ open: true, devices: [profiled, other] })
    )
    act(() => result.current.setAccountKey(`claude:work`))
    act(() => result.current.setDeviceId(`dev-2`))
    expect(result.current.accountKey).toBe(`codex:only`)
    expect(result.current.agent).toBe(`codex`)
  })
})

// FEED-73: a real action owns its run's MCP list (the server sets it on
// start), so the composer's pick never rides out for one.
describe(`useLaunchOptions MCP pick`, () => {
  const mcpServers = [
    {
      id: `linear`,
      enabledByDefault: true,
      connection: { status: `connected` },
    },
  ] as never

  it(`sends the pick, and omits it when the subject owns the list`, () => {
    const { result } = renderHook(() =>
      useLaunchOptions({
        open: true,
        devices: [device],
        teamId: `team-feed-73`,
        mcpServers,
      })
    )
    expect(result.current.buildOptions().mcpServerIds).toEqual([`linear`])
    expect(
      result.current.buildOptions({ omitMcp: true }).mcpServerIds
    ).toBeUndefined()
  })
})
