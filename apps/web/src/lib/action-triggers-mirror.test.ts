import { beforeEach, describe, expect, it } from "vitest"
import type { ActionTrigger } from "@exp/db-schema/domain"
import { automations } from "@/db/schema"
import {
  pauseDeviceTriggers,
  syncAutomationMirror,
} from "@/lib/action-triggers-mirror"

// SLOP-2: the legacy `automations` table mirrors `actions.triggers` until it
// is dropped, and a withdrawn device share pauses that device's triggers. The
// transaction is a recorder: a queued select, and every write in order.

type Write =
  | { op: `delete`; table: unknown }
  | { op: `upsert`; values: Record<string, unknown>; set: Record<string, unknown> }
  | { op: `update`; set: Record<string, unknown> }

const writes: Write[] = []
let selected: unknown[] = []

const tx = {
  select: () => ({ from: () => ({ where: async () => selected }) }),
  delete: (table: unknown) => ({
    where: async () => {
      writes.push({ op: `delete`, table })
    },
  }),
  insert: () => ({
    values: (values: Record<string, unknown>) => ({
      onConflictDoUpdate: async ({ set }: { set: Record<string, unknown> }) => {
        writes.push({ op: `upsert`, values, set })
      },
    }),
  }),
  update: () => ({
    set: (set: Record<string, unknown>) => ({
      where: async () => {
        writes.push({ op: `update`, set })
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
  writes.length = 0
  selected = []
})

describe(`syncAutomationMirror`, () => {
  it(`upserts one row per trigger, by its id, with the when-part alone`, async () => {
    await syncAutomationMirror(tx, {
      id: ACTION,
      teamId: TEAM,
      triggers: [schedule, event],
    })
    // Rows the action dropped go first; kept ones are never deleted, so the
    // run history's FK (ON DELETE SET NULL) survives an edit.
    expect(writes[0]).toEqual({ op: `delete`, table: automations })
    const upserts = writes.slice(1) as Extract<Write, { op: `upsert` }>[]
    expect(upserts.map((write) => write.values)).toEqual([
      {
        id: `t-schedule`,
        teamId: TEAM,
        actionId: ACTION,
        deviceId: `dev-1`,
        enabled: true,
        trigger: { kind: `schedule`, interval: `weekly`, minuteOfDay: 540, weekday: 1 },
        agent: `claude`,
        account: `work`,
        model: null,
        effort: null,
        sortOrder: 1,
      },
      {
        id: `t-event`,
        teamId: TEAM,
        actionId: ACTION,
        deviceId: `dev-2`,
        enabled: true,
        // No `source`: clients from before the merge never knew the key.
        trigger: { kind: `event`, event: `pr_merged`, filters: { boardIds: [`b-1`] } },
        agent: null,
        account: null,
        model: null,
        effort: null,
        sortOrder: 2,
      },
    ])
    expect(upserts[0]!.set).toMatchObject({ enabled: true, deviceId: `dev-1` })
  })

  it(`only deletes when the action has no triggers left`, async () => {
    await syncAutomationMirror(tx, { id: ACTION, teamId: TEAM, triggers: [] })
    expect(writes).toEqual([{ op: `delete`, table: automations }])
  })
})

describe(`pauseDeviceTriggers`, () => {
  it(`pauses the device's enabled triggers and leaves the others as they are`, async () => {
    selected = [
      { id: ACTION, teamId: TEAM, triggers: [schedule, event] },
      // Nothing bound to dev-1 here: untouched, no write.
      { id: `other`, teamId: TEAM, triggers: [event] },
      // Already paused: untouched too.
      { id: `paused`, teamId: TEAM, triggers: [{ ...schedule, enabled: false }] },
    ]
    await pauseDeviceTriggers(tx, [TEAM], `dev-1`)
    const updates = writes.filter((write) => write.op === `update`)
    expect(updates).toHaveLength(1)
    expect(updates[0]!.set.triggers).toEqual([
      { ...schedule, enabled: false },
      event,
    ])
    // …and the mirror follows in the same transaction.
    const upserts = writes.filter((write) => write.op === `upsert`)
    expect(upserts.map((write) => [write.values.id, write.values.enabled])).toEqual([
      [`t-schedule`, false],
      [`t-event`, true],
    ])
  })
})
