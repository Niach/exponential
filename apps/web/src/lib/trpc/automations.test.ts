import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// SLOP-2 compat shim: the `automations.*` adapter old clients still write
// through (iOS <= 0.14.49, Android <= 0.14.50, desktop/CLI <= 0.14.58). Each
// procedure takes the OLD input shape and must land as a rewrite of the owning
// action's `triggers` plus the re-synced mirror row, returned in the old wire
// shape. Delete with lib/trpc/automations.ts.
//
// The DB is a small store: `actions` and `automations` rows are kept by id
// and found by the uuids a drizzle condition carries as params; `devices`
// reads come off a queue.

const h = vi.hoisted(() => {
  type Row = Record<string, unknown>
  const state = {
    actions: new Map<string, Row>(),
    mirror: new Map<string, Row>(),
    deviceRows: [] as Row[][],
    actionWrites: [] as { id: string; set: Row }[],
  }
  const tableName = (table: unknown) =>
    (table as Record<symbol, string>)[Symbol.for(`drizzle:Name`)]
  // Every bound value of a drizzle condition (`eq`/`and`/`notInArray`).
  const params = (node: unknown, out: unknown[] = []): unknown[] => {
    if (!node || typeof node !== `object`) return out
    if (Array.isArray(node)) {
      for (const item of node) params(item, out)
      return out
    }
    const chunk = node as { queryChunks?: unknown[]; value?: unknown; encoder?: unknown }
    if (Array.isArray(chunk.queryChunks)) params(chunk.queryChunks, out)
    else if (`encoder` in chunk) out.push(chunk.value)
    return out
  }
  const select = () => {
    let table = ``
    let bound: unknown[] = []
    const rows = (): Row[] => {
      if (table === `devices`) return state.deviceRows.shift() ?? []
      if (table === `automations`) {
        return [...state.mirror.values()]
          .filter((row) => bound.includes(row.id) || bound.includes(row.teamId))
          .sort((a, b) => (a.sortOrder as number) - (b.sortOrder as number))
      }
      if (table === `actions`) {
        return [...state.actions.values()].filter(
          (row) =>
            bound.includes(row.id) ||
            (bound.includes(row.teamId) && bound.includes(row.name))
        )
      }
      return []
    }
    const chain = {
      from: (t: unknown) => {
        table = tableName(t)
        return chain
      },
      where: (condition: unknown) => {
        bound = params(condition)
        return chain
      },
      orderBy: () => chain,
      limit: (n: number) => Promise.resolve(rows().slice(0, n)),
      then: (resolve: (rows: Row[]) => unknown, reject?: () => unknown) =>
        Promise.resolve(rows()).then(resolve, reject),
    }
    return chain
  }
  const fakeDb: Record<string, unknown> = {
    select,
    update: (t: unknown) => ({
      set: (set: Row) => ({
        where: async (condition: unknown) => {
          if (tableName(t) !== `actions`) return
          for (const id of params(condition)) {
            const row = state.actions.get(id as string)
            if (!row) continue
            state.actions.set(id as string, { ...row, ...set })
            state.actionWrites.push({ id: id as string, set })
          }
        },
      }),
    }),
    insert: () => ({
      values: (values: Row) => ({
        onConflictDoUpdate: async ({ set }: { set: Row }) => {
          const id = values.id as string
          const held = state.mirror.get(id)
          state.mirror.set(
            id,
            held ? { ...held, ...set } : { createdAt: new Date(0), updatedAt: new Date(0), ...values }
          )
        },
      }),
    }),
    delete: (t: unknown) => ({
      where: async (condition: unknown) => {
        if (tableName(t) !== `automations`) return
        const bound = params(condition)
        for (const [id, row] of state.mirror) {
          // `eq(id)` alone, or the mirror sync's "this action's rows, minus
          // the kept ids".
          const byId = bound.length === 1 && bound[0] === id
          const dropped = bound.includes(row.actionId) && !bound.includes(id)
          if (byId || dropped) state.mirror.delete(id)
        }
      },
    }),
  }
  fakeDb.transaction = async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeDb)
  return {
    state,
    fakeDb,
    assertTeamMember: vi.fn(async () => ({ role: `member` }) as unknown),
    assertTeamOwner: vi.fn(async () => ({ role: `owner` }) as unknown),
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

import { MAX_ACTION_TRIGGERS } from "@exp/db-schema/domain"
import { automationsRouter } from "@/lib/trpc/automations"

const { state } = h

const TEAM = `11111111-1111-4111-8111-111111111111`
const ACTION = `22222222-2222-4222-8222-222222222222`
const OTHER_ACTION = `44444444-4444-4444-8444-444444444444`
const TIDY_ACTION = `55555555-5555-4555-8555-555555555555`
const T1 = `33333333-3333-4333-8333-333333333331`
const T2 = `33333333-3333-4333-8333-333333333332`
const UUID = /^[0-9a-f]{8}-[0-9a-f-]{27}$/

const caller = automationsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: h.fakeDb,
  request: new Request(`http://localhost/`),
} as never)

const rejectionOf = (promise: Promise<unknown>) =>
  promise.then(
    () => undefined,
    (e: unknown) => e as TRPCError
  )

const ownDevice = {
  userId: `actor`,
  sharedTeamIds: [],
  kind: `desktop`,
  caps: [`automations`],
  agents: [`claude`, `codex`],
}

const schedule = {
  id: T1,
  enabled: true,
  deviceId: `dev-1`,
  agent: `claude`,
  account: `work`,
  kind: `schedule` as const,
  interval: `daily` as const,
  minuteOfDay: 540,
}
const event = {
  id: T2,
  enabled: true,
  deviceId: `dev-1`,
  kind: `event` as const,
  source: `exponential` as const,
  event: `pr_merged` as const,
}

/** Seeds an action with its triggers AND the mirror rows the server keeps. */
function seedAction(
  id: string,
  triggers: Record<string, unknown>[],
  extra: Record<string, unknown> = {}
) {
  state.actions.set(id, { id, teamId: TEAM, name: `Sweep`, inputs: [], triggers, ...extra })
  triggers.forEach((trigger, index) => {
    const { id: triggerId, enabled, deviceId, agent, account, model, effort, source: _source, ...when } =
      trigger
    state.mirror.set(triggerId as string, {
      id: triggerId,
      teamId: TEAM,
      actionId: id,
      deviceId,
      enabled,
      trigger: when,
      agent: agent ?? null,
      account: account ?? null,
      model: model ?? null,
      effort: effort ?? null,
      sortOrder: index + 1,
      createdAt: new Date(0),
      updatedAt: new Date(0),
    })
  })
}

const triggersOf = (id: string) =>
  state.actions.get(id)!.triggers as Record<string, unknown>[]

beforeEach(() => {
  state.actions.clear()
  state.mirror.clear()
  state.deviceRows = []
  state.actionWrites = []
  h.assertTeamMember.mockClear()
  h.assertTeamOwner.mockClear()
})

describe(`automations.list (legacy adapter)`, () => {
  it(`returns the team's mirror rows in order, member-gated`, async () => {
    seedAction(ACTION, [schedule, event])
    const { automations } = await caller.list({ teamId: TEAM })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM)
    expect(automations.map((row) => row.id)).toEqual([T1, T2])
    expect(automations[0]).toMatchObject({
      actionId: ACTION,
      deviceId: `dev-1`,
      trigger: { kind: `schedule`, interval: `daily`, minuteOfDay: 540 },
      account: `work`,
    })
  })
})

describe(`automations.create (legacy adapter)`, () => {
  it(`appends one trigger to the action and returns its mirror row`, async () => {
    seedAction(ACTION, [schedule])
    state.deviceRows.push([ownDevice])
    const { automation, txId } = await caller.create({
      teamId: TEAM,
      actionId: ACTION,
      deviceId: `dev-1`,
      // An old build's when-part: no `source`, and an unknown key is dropped
      // (strip mode), never refused.
      trigger: { kind: `event`, event: `pr_merged`, somethingOld: 1 } as never,
      agent: `codex`,
    })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM)
    expect(txId).toBe(42)
    const triggers = triggersOf(ACTION)
    // The sibling is carried over untouched.
    expect(triggers[0]).toEqual(schedule)
    expect(triggers[1]).toEqual({
      id: expect.stringMatching(UUID),
      enabled: true,
      deviceId: `dev-1`,
      agent: `codex`,
      kind: `event`,
      source: `exponential`,
      event: `pr_merged`,
    })
    expect(automation).toMatchObject({
      id: triggers[1]!.id,
      teamId: TEAM,
      actionId: ACTION,
      deviceId: `dev-1`,
      enabled: true,
      trigger: { kind: `event`, event: `pr_merged` },
      agent: `codex`,
      account: null,
      model: null,
      effort: null,
      sortOrder: 2,
    })
  })

  it(`stores the old ambient pin (system or null) as unpinned, like the migration`, async () => {
    seedAction(ACTION, [])
    state.deviceRows.push([ownDevice])
    const { automation } = await caller.create({
      teamId: TEAM,
      actionId: ACTION,
      deviceId: `dev-1`,
      trigger: { kind: `schedule`, interval: `weekly`, minuteOfDay: 60, weekday: 2 },
      enabled: false,
      agent: `claude`,
      account: `system`,
    })
    expect(triggersOf(ACTION)[0]).not.toHaveProperty(`account`)
    expect(automation).toMatchObject({ enabled: false, account: null })
  })

  it(`maps builtin:tidy-up to the team's real Tidy up row`, async () => {
    seedAction(TIDY_ACTION, [], { name: `Tidy up` })
    state.deviceRows.push([ownDevice])
    const { automation } = await caller.create({
      teamId: TEAM,
      actionId: `builtin:tidy-up`,
      deviceId: `dev-1`,
      trigger: { kind: `schedule`, interval: `daily`, minuteOfDay: 0 },
    })
    expect(automation.actionId).toBe(TIDY_ACTION)
    expect(triggersOf(TIDY_ACTION)).toHaveLength(1)
  })

  it(`refuses builtin:tidy-up with a worded error when the team has no such row`, async () => {
    const error = await rejectionOf(
      caller.create({
        teamId: TEAM,
        actionId: `builtin:tidy-up`,
        deviceId: `dev-1`,
        trigger: { kind: `schedule`, interval: `daily`, minuteOfDay: 0 },
      })
    )
    expect(error).toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `Update the app to schedule Tidy up`,
    })
  })

  it(`applies the trigger rules: a foreign action, a required input, a foreign device, the cap`, async () => {
    state.actions.set(ACTION, { id: ACTION, teamId: `other-team`, inputs: [], triggers: [] })
    const input = {
      teamId: TEAM,
      actionId: ACTION,
      deviceId: `dev-1`,
      trigger: { kind: `schedule` as const, interval: `daily` as const, minuteOfDay: 0 },
    }
    expect(await rejectionOf(caller.create(input))).toMatchObject({
      code: `BAD_REQUEST`,
      message: `Action must belong to the team`,
    })

    seedAction(ACTION, [], { inputs: [{ key: `repo`, required: true }] })
    state.deviceRows.push([ownDevice])
    expect(await rejectionOf(caller.create(input))).toMatchObject({ code: `BAD_REQUEST` })

    seedAction(ACTION, [])
    state.deviceRows.push([{ ...ownDevice, userId: `someone-else` }])
    expect((await rejectionOf(caller.create(input)))!.message).toContain(
      `device must be yours`
    )

    seedAction(
      ACTION,
      Array.from({ length: MAX_ACTION_TRIGGERS }, (_, i) => ({
        ...schedule,
        id: `33333333-3333-4333-8333-3333333333${String(i).padStart(2, `0`)}`,
      }))
    )
    expect((await rejectionOf(caller.create(input)))!.message).toContain(`at most`)
    expect(state.actionWrites).toHaveLength(0)
  })
})

describe(`automations.update (legacy adapter)`, () => {
  it(`pauses with {id, enabled} alone: no device check, siblings untouched`, async () => {
    seedAction(ACTION, [schedule, event])
    const { automation, txId } = await caller.update({ id: T1, enabled: false })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM)
    expect(txId).toBe(42)
    expect(triggersOf(ACTION)).toEqual([{ ...schedule, enabled: false }, event])
    expect(automation).toMatchObject({
      id: T1,
      enabled: false,
      account: `work`,
      trigger: { kind: `schedule`, interval: `daily`, minuteOfDay: 540 },
      sortOrder: 1,
    })
    // The sibling's mirror row survived the sync.
    expect(state.mirror.get(T2)).toMatchObject({ enabled: true })
  })

  it(`pauses a trigger whose pinned model left the contract, on an over-cap action`, async () => {
    const stale = { ...schedule, model: `retired-model` }
    const many = Array.from({ length: MAX_ACTION_TRIGGERS + 3 }, (_, i) => ({
      ...event,
      id: `33333333-3333-4333-8333-3333333334${String(i).padStart(2, `0`)}`,
    }))
    seedAction(ACTION, [stale, ...many])
    const { automation } = await caller.update({ id: T1, enabled: false })
    expect(automation).toMatchObject({ enabled: false, model: `retired-model` })
    expect(triggersOf(ACTION)).toHaveLength(MAX_ACTION_TRIGGERS + 4)
  })

  it(`applies an old edit form: when-part, device and explicit-null pins`, async () => {
    seedAction(ACTION, [schedule])
    state.deviceRows.push([ownDevice])
    const { automation } = await caller.update({
      id: T1,
      actionId: ACTION,
      deviceId: `dev-2`,
      trigger: { kind: `schedule`, interval: `monthly`, minuteOfDay: 30, dayOfMonth: 5 },
      agent: `claude`,
      account: null,
      model: null,
      effort: null,
      sortOrder: 7,
    })
    expect(triggersOf(ACTION)).toEqual([
      {
        id: T1,
        enabled: true,
        deviceId: `dev-2`,
        agent: `claude`,
        kind: `schedule`,
        interval: `monthly`,
        minuteOfDay: 30,
        dayOfMonth: 5,
      },
    ])
    // `sortOrder` is accepted and ignored: position decides.
    expect(automation).toMatchObject({ deviceId: `dev-2`, account: null, sortOrder: 1 })
  })

  it(`drops the account pin on an agent switch that names none (EXP-995)`, async () => {
    seedAction(ACTION, [schedule])
    state.deviceRows.push([ownDevice])
    const { automation } = await caller.update({ id: T1, agent: `codex` })
    expect(automation).toMatchObject({ agent: `codex`, account: null })
    expect(triggersOf(ACTION)[0]).not.toHaveProperty(`account`)
  })

  it(`moves the trigger to another action, keeping its id and mirror row`, async () => {
    seedAction(ACTION, [schedule, event])
    seedAction(OTHER_ACTION, [], { name: `Other` })
    const { automation } = await caller.update({ id: T1, actionId: OTHER_ACTION })
    expect(triggersOf(ACTION)).toEqual([event])
    expect(triggersOf(OTHER_ACTION)).toEqual([schedule])
    expect(automation).toMatchObject({ id: T1, actionId: OTHER_ACTION, sortOrder: 1 })
    expect(state.mirror.get(T2)).toMatchObject({ actionId: ACTION, sortOrder: 1 })
    expect(state.mirror.size).toBe(2)
  })

  it(`refuses an unknown model it was asked to set, and an unknown id`, async () => {
    seedAction(ACTION, [schedule])
    expect(await rejectionOf(caller.update({ id: T1, model: `gpt-nope` }))).toMatchObject({
      code: `BAD_REQUEST`,
    })
    expect(await rejectionOf(caller.update({ id: T2, enabled: false }))).toMatchObject({
      code: `NOT_FOUND`,
      message: `Automation not found`,
    })
    expect(state.actionWrites).toHaveLength(0)
  })
})

describe(`automations.delete (legacy adapter)`, () => {
  it(`removes the trigger from its action and its mirror row`, async () => {
    seedAction(ACTION, [schedule, event])
    const result = await caller.delete({ id: T1 })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM)
    expect(result).toEqual({ ok: true, txId: 42 })
    expect(triggersOf(ACTION)).toEqual([event])
    expect([...state.mirror.keys()]).toEqual([T2])
    expect(state.mirror.get(T2)).toMatchObject({ sortOrder: 1 })
  })

  it(`drops a mirror row whose action is gone, and 404s an unknown id`, async () => {
    seedAction(ACTION, [schedule])
    state.actions.clear()
    expect(await caller.delete({ id: T1 })).toEqual({ ok: true, txId: 42 })
    expect(state.mirror.size).toBe(0)
    expect(await rejectionOf(caller.delete({ id: T1 }))).toMatchObject({
      code: `NOT_FOUND`,
    })
  })
})
