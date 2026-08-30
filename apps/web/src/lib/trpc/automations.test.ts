import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-583 automations router: the target action must be a same-team custom
// row with no required input (while enabled), the device must be the caller's
// or shared with the team AND advertise `automations` (+ the pinned agent),
// event filters must be the team's, agent/model/effort follow the contract.
// DB access is a queued select-chain + insert/update recorder (actions.test.ts
// precedent); the transaction hands back the same fake.

const h = vi.hoisted(() => {
  const selectResults: unknown[][] = []
  const inserts: Record<string, unknown>[] = []
  const updates: Record<string, unknown>[] = []
  const makeChain = () => {
    const chain = {
      from: () => chain,
      where: () => chain,
      orderBy: () => chain,
      limit: () => Promise.resolve(selectResults.shift() ?? []),
      then: (resolve: (rows: unknown[]) => unknown, reject?: () => unknown) =>
        Promise.resolve(selectResults.shift() ?? []).then(resolve, reject),
    }
    return chain
  }
  const fakeDb: Record<string, unknown> = {
    select: () => makeChain(),
    insert: () => ({
      values: (values: Record<string, unknown>) => {
        inserts.push(values)
        return { returning: async () => [{ id: `new-automation`, ...values }] }
      },
    }),
    update: () => ({
      set: (values: Record<string, unknown>) => {
        updates.push(values)
        return {
          where: () => ({
            returning: async () => [{ id: `updated-automation`, ...values }],
          }),
        }
      },
    }),
    delete: () => ({ where: async () => undefined }),
  }
  fakeDb.transaction = async (fn: (tx: unknown) => Promise<unknown>) =>
    fn(fakeDb)
  return {
    assertTeamMember: vi.fn(async () => ({ role: `member` }) as unknown),
    assertTeamOwner: vi.fn(async () => ({ role: `owner` }) as unknown),
    selectResults,
    inserts,
    updates,
    fakeDb,
  }
})

vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  assertTeamOwner: h.assertTeamOwner,
}))
vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
vi.mock(`@/lib/trpc`, async (importOriginal) => {
  const mod = await importOriginal<Record<string, unknown>>()
  return { ...mod, generateTxId: async () => 42 }
})

import { automationsRouter } from "@/lib/trpc/automations"

const { selectResults, inserts, updates, fakeDb } = h

const TEAM_ID = `11111111-1111-4111-8111-111111111111`
const ACTION_ID = `22222222-2222-4222-8222-222222222222`
const AUTOMATION_ID = `44444444-4444-4444-8444-444444444444`
const BOARD_ID = `33333333-3333-4333-8333-333333333333`

const caller = automationsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(`http://localhost/`),
} as never)

const schedule = { kind: `schedule` as const, interval: `daily` as const, minuteOfDay: 420 }
const ownDevice = {
  userId: `actor`,
  sharedTeamId: null,
  caps: [`automations`],
  agents: [`claude`, `codex`],
}
const action = { id: ACTION_ID, teamId: TEAM_ID, inputs: [] }

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}

beforeEach(() => {
  selectResults.length = 0
  inserts.length = 0
  updates.length = 0
  h.assertTeamOwner.mockClear()
})

describe(`automations.create`, () => {
  it(`persists a schedule bound to the caller's own device`, async () => {
    selectResults.push([action]) // target action
    selectResults.push([ownDevice]) // device rows
    selectResults.push([]) // sortOrder probe
    const { automation, txid } = await caller.create({
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      deviceId: `device-1`,
      trigger: schedule,
      agent: `claude`,
      model: `opus`,
    })
    expect(txid).toBe(42)
    expect(automation).toMatchObject({
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      deviceId: `device-1`,
      enabled: true,
      trigger: schedule,
      agent: `claude`,
      model: `opus`,
      effort: null,
      sortOrder: 1,
    })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`rejects a foreign action, a builtin and a cross-team action`, async () => {
    selectResults.push([])
    let error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, actionId: ACTION_ID, deviceId: `d`, trigger: schedule })
    )
    expect((error as TRPCError).message).toBe(`Action not found`)

    selectResults.push([{ ...action, teamId: `other` }])
    error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, actionId: ACTION_ID, deviceId: `d`, trigger: schedule })
    )
    expect((error as TRPCError).message).toBe(`Action must belong to the team`)
    expect(inserts).toHaveLength(0)
  })

  it(`refuses an enabled automation on an action with required inputs`, async () => {
    selectResults.push([{ ...action, inputs: [{ key: `t`, label: `T`, type: `text`, required: true }] }])
    const error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, actionId: ACTION_ID, deviceId: `d`, trigger: schedule })
    )
    expect((error as TRPCError).message).toContain(`required inputs`)
  })

  it(`allows a DISABLED automation on an action with required inputs`, async () => {
    selectResults.push([{ ...action, inputs: [{ key: `t`, label: `T`, type: `text`, required: true }] }])
    selectResults.push([ownDevice])
    selectResults.push([])
    await caller.create({
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      deviceId: `d`,
      trigger: schedule,
      enabled: false,
    })
    expect(inserts[0]).toMatchObject({ enabled: false })
  })

  it(`rejects a device that is neither the caller's nor shared with the team`, async () => {
    selectResults.push([action])
    selectResults.push([{ ...ownDevice, userId: `someone-else` }])
    const error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, actionId: ACTION_ID, deviceId: `d`, trigger: schedule })
    )
    expect((error as TRPCError).message).toContain(`yours or shared`)
  })

  it(`accepts a device shared with the team by a current member`, async () => {
    selectResults.push([action])
    selectResults.push([{ ...ownDevice, userId: `host`, sharedTeamId: TEAM_ID }])
    selectResults.push([{ userId: `host` }]) // teamMembers probe
    selectResults.push([])
    await caller.create({ teamId: TEAM_ID, actionId: ACTION_ID, deviceId: `d`, trigger: schedule })
    expect(inserts).toHaveLength(1)
  })

  it(`rejects a device without the automations cap or the pinned agent`, async () => {
    selectResults.push([action])
    selectResults.push([{ ...ownDevice, caps: [`actions`] }])
    let error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, actionId: ACTION_ID, deviceId: `d`, trigger: schedule })
    )
    expect((error as TRPCError).message).toContain(`No agent is signed in`)

    selectResults.push([action])
    selectResults.push([ownDevice])
    error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, actionId: ACTION_ID, deviceId: `d`, trigger: schedule, agent: `pi` })
    )
    expect((error as TRPCError).message).toBe(`pi is not available on that device`)
  })

  it(`validates model/effort against the agent's contract lists`, async () => {
    selectResults.push([action])
    const error = await rejectionOf(
      caller.create({
        teamId: TEAM_ID,
        actionId: ACTION_ID,
        deviceId: `d`,
        trigger: schedule,
        agent: `codex`,
        model: `opus`,
      })
    )
    expect((error as TRPCError).message).toBe(`Unknown codex model`)
  })

  it(`rejects event filters naming another team's boards`, async () => {
    selectResults.push([action])
    selectResults.push([ownDevice])
    selectResults.push([]) // boards probe finds nothing
    const error = await rejectionOf(
      caller.create({
        teamId: TEAM_ID,
        actionId: ACTION_ID,
        deviceId: `d`,
        trigger: { kind: `event`, event: `created`, filters: { boardIds: [BOARD_ID] } },
      })
    )
    expect((error as TRPCError).message).toBe(`Automation boards must belong to the team`)
  })

  it(`rejects malformed triggers at the zod boundary`, async () => {
    const bad = [
      { kind: `schedule`, interval: `weekly`, minuteOfDay: 1 },
      { kind: `schedule`, interval: `daily`, minuteOfDay: 1440 },
      { kind: `event`, event: `comment_added` },
      { kind: `event`, event: `created`, filters: { labelIds: [BOARD_ID] } },
      { kind: `schedule`, interval: `daily`, minuteOfDay: 1, deviceId: `d` },
    ]
    for (const trigger of bad) {
      const error = await rejectionOf(
        caller.create({
          teamId: TEAM_ID,
          actionId: ACTION_ID,
          deviceId: `d`,
          trigger: trigger as never,
        })
      )
      expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    }
    expect(inserts).toHaveLength(0)
  })
})

describe(`automations.update`, () => {
  const existing = {
    id: AUTOMATION_ID,
    teamId: TEAM_ID,
    actionId: ACTION_ID,
    deviceId: `device-1`,
    enabled: true,
    trigger: schedule,
    agent: null,
    model: null,
    effort: null,
  }

  it(`toggles enabled without re-checking an unchanged device binding`, async () => {
    selectResults.push([existing])
    selectResults.push([action])
    const { automation } = await caller.update({ id: AUTOMATION_ID, enabled: false })
    expect(automation).toMatchObject({ enabled: false, deviceId: `device-1` })
    expect(updates).toHaveLength(1)
  })

  it(`re-checks the device when it changes`, async () => {
    selectResults.push([existing])
    selectResults.push([action])
    selectResults.push([]) // no device rows
    const error = await rejectionOf(caller.update({ id: AUTOMATION_ID, deviceId: `other` }))
    expect((error as TRPCError).message).toContain(`yours or shared`)
    expect(updates).toHaveLength(0)
  })

  it(`refuses enabling when the target now has a required input`, async () => {
    selectResults.push([{ ...existing, enabled: false }])
    selectResults.push([{ ...action, inputs: [{ key: `t`, label: `T`, type: `text`, required: true }] }])
    const error = await rejectionOf(caller.update({ id: AUTOMATION_ID, enabled: true }))
    expect((error as TRPCError).message).toContain(`required inputs`)
  })

  it(`404s an unknown automation`, async () => {
    selectResults.push([])
    const error = await rejectionOf(caller.update({ id: AUTOMATION_ID, enabled: true }))
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
  })
})

describe(`automations.delete`, () => {
  it(`is owner-gated on the row's team`, async () => {
    selectResults.push([{ id: AUTOMATION_ID, teamId: TEAM_ID }])
    const result = await caller.delete({ id: AUTOMATION_ID })
    expect(result).toEqual({ ok: true, txid: 42 })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })
})
