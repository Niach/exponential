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
