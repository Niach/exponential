import { beforeEach, describe, expect, it } from "vitest"
import type { ActionTrigger } from "@exp/db-schema/domain"
import { pauseDeviceTriggers } from "@/lib/action-trigger-rules"

// A withdrawn device share pauses that device's triggers. The transaction is
// a recorder: a queued select, and every update in order.

const updates: Array<Record<string, unknown>> = []
let selected: unknown[] = []

const tx = {
  select: () => ({ from: () => ({ where: async () => selected }) }),
  update: () => ({
    set: (set: Record<string, unknown>) => ({
      where: async () => {
        updates.push(set)
      },
    }),
  }),
} as never

const TEAM = `11111111-1111-4111-8111-111111111111`
const ACTION = `22222222-2222-4222-8222-222222222222`

const schedule: ActionTrigger = {
  id: `t-schedule`,
  enabled: true,
  deviceId: `dev-1`,
  agent: `claude`,
  account: `work`,
  kind: `schedule`,
  interval: `weekly`,
  minuteOfDay: 540,
  weekday: 1,
}
const event: ActionTrigger = {
  id: `t-event`,
  enabled: true,
  deviceId: `dev-2`,
  kind: `event`,
  source: `exponential`,
  event: `pr_merged`,
  filters: { boardIds: [`b-1`] },
}

beforeEach(() => {
  updates.length = 0
  selected = []
})

describe(`pauseDeviceTriggers`, () => {
  it(`pauses the device's enabled triggers and leaves the others as they are`, async () => {
    selected = [
      { id: ACTION, triggers: [schedule, event] },
      // Nothing bound to dev-1 here: untouched, no write.
      { id: `other`, triggers: [event] },
      // Already paused: untouched too.
      { id: `paused`, triggers: [{ ...schedule, enabled: false }] },
    ]
    await pauseDeviceTriggers(tx, [TEAM], `dev-1`)
    expect(updates).toHaveLength(1)
    expect(updates[0]!.triggers).toEqual([{ ...schedule, enabled: false }, event])
  })
})
