import { describe, expect, it } from "vitest"
import {
  readTriggerBase,
  triggerWriteBase,
} from "@/components/action-triggers-section"

const schedule = (id: string, enabled: boolean) => ({
  id,
  enabled,
  deviceId: `device-1`,
  kind: `schedule`,
  schedule: { frequency: `daily`, time: `09:00` },
})

describe(`triggerWriteBase`, () => {
  const synced = {
    actionId: `a1`,
    triggers: [schedule(`t1`, true)],
    updatedAt: new Date(`2026-10-01T10:00:00.000Z`),
  }

  it(`builds on the synced row when nothing was written`, () => {
    expect(triggerWriteBase(synced, null)).toBe(synced.triggers)
  })

  it(`keeps the last write until the synced row echoes it`, () => {
    const last = {
      actionId: `a1`,
      triggers: [schedule(`t1`, false)],
      updatedAt: `2026-10-01T10:00:05.000Z`,
    }
    expect(triggerWriteBase(synced, last)).toBe(last.triggers)
    const echoed = { ...synced, updatedAt: new Date(last.updatedAt) }
    expect(triggerWriteBase(echoed, last)).toBe(echoed.triggers)
  })

  it(`ignores another action's write`, () => {
    const last = {
      actionId: `a2`,
      triggers: [],
      updatedAt: `2026-10-01T11:00:00.000Z`,
    }
    expect(triggerWriteBase(synced, last)).toBe(synced.triggers)
  })
})

describe(`readTriggerBase`, () => {
  it(`flags a stored entry this build cannot read`, () => {
    const stored = [{ id: `t9`, deviceId: `device-1`, kind: `webhook` }]
    expect(readTriggerBase(stored)).toEqual({ triggers: [], unreadable: true })
    expect(readTriggerBase([])).toEqual({ triggers: [], unreadable: false })
    expect(readTriggerBase(null)).toEqual({ triggers: [], unreadable: false })
  })
})
