import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// Actions router (EXP-257/EXP-539): list returns DB rows only (clients and
// the MCP tool construct/append the virtual builtins themselves), the
// reserved builtin ids stay read/write-protected, inputs schemas persist
// through create/update, and the reserved name stays unique. DB access is a
// queued select-chain + insert recorder (steer.test.ts precedent).

const h = vi.hoisted(() => {
  const selectResults: unknown[][] = []
  const inserts: Record<string, unknown>[] = []
  const updates: Record<string, unknown>[] = []
  const deletes: unknown[] = []
  const makeChain = () => {
    const chain = {
      from: () => chain,
      where: () => chain,
      orderBy: () => chain,
      limit: () => Promise.resolve(selectResults.shift() ?? []),
      // list awaits after orderBy (no .limit) — make the chain thenable.
      then: (resolve: (rows: unknown[]) => unknown, reject?: () => unknown) =>
        Promise.resolve(selectResults.shift() ?? []).then(resolve, reject),
    }
    return chain
  }
  return {
    assertTeamMember: vi.fn(async () => ({ role: `member` }) as unknown),
    assertTeamOwner: vi.fn(async () => ({ role: `owner` }) as unknown),
    selectResults,
    inserts,
    updates,
    deletes,
    fakeDb: (() => {
      const fakeDb: Record<string, unknown> = {
        select: () => makeChain(),
        insert: () => ({
          values: (values: Record<string, unknown>) => {
            inserts.push(values)
            return {
              onConflictDoNothing: () => ({
                returning: async () => [{ id: `new-action`, ...values }],
              }),
              // The legacy automations mirror upserts by trigger id.
              onConflictDoUpdate: async () => undefined,
            }
          },
        }),
        update: () => ({
          set: (values: Record<string, unknown>) => {
            updates.push(values)
            return {
              where: () => ({
                returning: async () => [{ id: `updated-action`, ...values }],
              }),
            }
          },
        }),
        delete: (table: unknown) => {
          deletes.push(table)
          return { where: async () => undefined }
        },
      }
      // EXP-707: writes run in a txId-minting transaction now.
      fakeDb.transaction = async (fn: (tx: unknown) => Promise<unknown>) =>
        fn(fakeDb)
      return fakeDb
    })(),
  }
})

vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  assertTeamOwner: h.assertTeamOwner,
}))
// loadAction / assertRepoInTeam import the db lazily — same fake.
vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
// EXP-707: writes mint a txId sync barrier.
vi.mock(`@/lib/trpc`, async (importOriginal) => {
  const mod = await importOriginal<Record<string, unknown>>()
  return { ...mod, generateTxId: async () => 42 }
})

import { actionsRouter } from "@/lib/trpc/actions"
import { actions, automations } from "@/db/schema"

const { selectResults, inserts, updates, deletes, fakeDb } = h

const TEAM_ID = `11111111-1111-4111-8111-111111111111`
const ACTION_ID = `22222222-2222-4222-8222-222222222222`
const TRIGGER_ID = `33333333-3333-4333-8333-333333333333`
const BUILTIN_ID = `builtin:create-action`
const FIX_CONFLICTS_ID = `builtin:fix-conflicts`

const caller = actionsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(`http://localhost/`),
} as never)

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
  deletes.length = 0
  h.assertTeamMember.mockClear()
  h.assertTeamOwner.mockClear()
})

describe(`actions.list — rows only (EXP-539)`, () => {
  it(`returns DB rows flagged builtin: false and appends nothing`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Code review`, inputs: [] },
    ])
    const { actions } = await caller.list({ teamId: TEAM_ID })
    expect(actions).toHaveLength(1)
    expect(actions[0]).toMatchObject({ id: ACTION_ID, builtin: false })
  })

  it(`stays empty for a team with no actions (clients construct builtins locally)`, async () => {
    selectResults.push([])
    const { actions } = await caller.list({ teamId: TEAM_ID })
    expect(actions).toEqual([])
  })
})

describe(`actions — builtin is read/write-protected`, () => {
  it(`get refuses the builtin id`, async () => {
    const error = await rejectionOf(caller.get({ id: BUILTIN_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
  })

  it(`update refuses the builtin id before any DB work`, async () => {
    const error = await rejectionOf(
      caller.update({ id: BUILTIN_ID, name: `Hijack` })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`can't be edited`)
    expect(h.assertTeamOwner).not.toHaveBeenCalled()
  })

  it(`delete refuses the builtin id`, async () => {
    const error = await rejectionOf(caller.delete({ id: BUILTIN_ID }))
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`can't be deleted`)
  })

  it(`get/update/delete refuse the fix-conflicts builtin id too (EXP-259)`, async () => {
    for (const call of [
      caller.get({ id: FIX_CONFLICTS_ID }),
      caller.update({ id: FIX_CONFLICTS_ID, name: `Hijack` }),
      caller.delete({ id: FIX_CONFLICTS_ID }),
    ]) {
      const error = await rejectionOf(call)
      expect(error).toBeInstanceOf(TRPCError)
      expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    }
  })

  it(`get/update/delete refuse the chat builtin id too (EXP-615)`, async () => {
    for (const call of [
      caller.get({ id: `builtin:chat` }),
      caller.update({ id: `builtin:chat`, name: `Hijack` }),
      caller.delete({ id: `builtin:chat` }),
    ]) {
      const error = await rejectionOf(call)
      expect(error).toBeInstanceOf(TRPCError)
      expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    }
  })
})

describe(`actions — tidy-up builtin (FEED-50)`, () => {
  it(`get/update/delete refuse the tidy-up builtin id`, async () => {
    for (const call of [
      caller.get({ id: `builtin:tidy-up` }),
      caller.update({ id: `builtin:tidy-up`, name: `Hijack` }),
      caller.delete({ id: `builtin:tidy-up` }),
    ]) {
      const error = await rejectionOf(call)
      expect(error).toBeInstanceOf(TRPCError)
      expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    }
  })

  it(`refuses the reserved Tidy up name`, async () => {
    const error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, name: `Tidy Up`, body: `x` })
    )
    expect((error as TRPCError).code).toBe(`CONFLICT`)
    expect(inserts).toHaveLength(0)
  })

  // The legacy mirror's action_id carries no FK, so the delete itself
  // cascades: mirror rows first, then the action row.
  it(`delete removes the action's mirror rows in the same transaction`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Nightly`, inputs: [] },
    ])
    await caller.delete({ id: ACTION_ID })
    expect(deletes).toEqual([automations, actions])
  })
})

describe(`actions.create — inputs + reserved name (EXP-257)`, () => {
  it(`persists a valid inputs schema`, async () => {
    // sortOrder probe select.
    selectResults.push([])
    const inputs = [
      { key: `topic`, label: `Topic`, type: `icon` as const, required: true },
    ]
    const { action } = await caller.create({
      teamId: TEAM_ID,
      name: `Weekly review`,
      body: `Do the thing`,
      inputs,
    })
    expect(inserts[0]).toMatchObject({ inputs })
    expect(action).toMatchObject({ name: `Weekly review` })
  })

  // EXP-825: the composer hint rides create/update; blank clears it.
  it(`persists the composer hint and clears it on blank`, async () => {
    selectResults.push([])
    await caller.create({
      teamId: TEAM_ID,
      name: `Release`,
      body: `x`,
      promptPlaceholder: `  Scope: which platforms  `,
    })
    expect(inserts[0]).toMatchObject({ promptPlaceholder: `Scope: which platforms` })

    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Release`, inputs: [] }])
    await caller.update({ id: ACTION_ID, promptPlaceholder: `` })
    expect(updates[0]!.promptPlaceholder).toBeNull()
  })

  it(`defaults inputs to an empty array`, async () => {
    selectResults.push([])
    await caller.create({ teamId: TEAM_ID, name: `Plain`, body: `x` })
    expect(inserts[0]!.inputs).toEqual([])
  })

  it(`rejects duplicate input keys as input validation`, async () => {
    const error = await rejectionOf(
      caller.create({
        teamId: TEAM_ID,
        name: `Dup`,
        body: `x`,
        inputs: [
          { key: `a`, label: `A`, type: `icon` },
          { key: `a`, label: `B`, type: `icon` },
        ],
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(inserts).toHaveLength(0)
  })

  it(`refuses the reserved builtin name, case-insensitively`, async () => {
    const error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, name: `create ACTION`, body: `x` })
    )
    expect((error as TRPCError).code).toBe(`CONFLICT`)
    expect(inserts).toHaveLength(0)
  })

  it(`refuses the reserved chat name (EXP-615)`, async () => {
    // Chat session rows carry actionName "Chat" and clients watch started
    // runs by that snapshot — a team action named "Chat" would hijack it.
    const error = await rejectionOf(
      caller.create({ teamId: TEAM_ID, name: `chat`, body: `x` })
    )
    expect((error as TRPCError).code).toBe(`CONFLICT`)
    expect(inserts).toHaveLength(0)
  })

  it(`refuses renaming an existing action to the reserved name`, async () => {
    // loadAction select.
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Old name`, inputs: [] },
    ])
    const error = await rejectionOf(
      caller.update({ id: ACTION_ID, name: `Create action` })
    )
    expect((error as TRPCError).code).toBe(`CONFLICT`)
  })
})

// SLOP-2: an action carries its triggers. A triggered run fills no inputs, so
// an enabled trigger and a required input can never meet, whichever side the
// write changes.
describe(`actions.update — required inputs vs triggers`, () => {
  const requiredInput = {
    key: `target`,
    label: `Target`,
    type: `icon` as const,
    required: true,
  }
  const optionalInput = { ...requiredInput, required: false }
  const stored = (enabled: boolean) => ({
    id: TRIGGER_ID,
    enabled,
    deviceId: `dev-1`,
    kind: `schedule` as const,
    interval: `daily` as const,
    minuteOfDay: 540,
  })

  it(`refuses adding a required input to an action with an enabled trigger`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Sweep`, inputs: [], triggers: [stored(true)] },
    ])
    const error = await rejectionOf(
      caller.update({ id: ACTION_ID, inputs: [requiredInput] })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`required inputs`)
    expect(updates).toHaveLength(0)
  })

  it(`allows a required input while every trigger is paused`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Sweep`, inputs: [], triggers: [stored(false)] },
    ])
    await caller.update({ id: ACTION_ID, inputs: [requiredInput] })
    expect(updates[0]!.inputs).toEqual([requiredInput])
  })

  it(`refuses enabling a trigger on an action with a required input`, async () => {
    selectResults.push([
      {
        id: ACTION_ID,
        teamId: TEAM_ID,
        name: `Sweep`,
        inputs: [requiredInput],
        triggers: [stored(false)],
      },
    ])
    const error = await rejectionOf(
      caller.update({ id: ACTION_ID, triggers: [stored(true)] })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`required inputs`)
    expect(updates).toHaveLength(0)
  })

  it(`allows making the inputs optional on a triggered action`, async () => {
    selectResults.push([
      {
        id: ACTION_ID,
        teamId: TEAM_ID,
        name: `Sweep`,
        inputs: [requiredInput],
        triggers: [stored(false)],
      },
    ])
    await caller.update({ id: ACTION_ID, inputs: [optionalInput] })
    expect(updates[0]!.inputs).toEqual([optionalInput])
  })
})

describe(`actions.update — triggers (SLOP-2)`, () => {
  const existing = {
    id: TRIGGER_ID,
    enabled: true,
    deviceId: `dev-1`,
    agent: `claude`,
    kind: `schedule` as const,
    interval: `daily` as const,
    minuteOfDay: 540,
  }
  const ownDevice = {
    userId: `actor`,
    sharedTeamIds: [],
    kind: `desktop`,
    caps: [`automations`],
    agents: [`claude`],
  }
  const action = (triggers: unknown[]) => ({
    id: ACTION_ID,
    teamId: TEAM_ID,
    name: `Sweep`,
    inputs: [],
    triggers,
  })

  it(`keeps a held trigger's id, mints one for a new trigger and stores the event source`, async () => {
    selectResults.push([action([existing])])
    // The new trigger's device is checked; the unchanged binding is not.
    selectResults.push([ownDevice])
    await caller.update({
      id: ACTION_ID,
      triggers: [
        { ...existing, enabled: false },
        { deviceId: `dev-1`, kind: `event`, event: `pr_merged`, model: null },
      ],
    })
    const written = updates[0]!.triggers as Record<string, unknown>[]
    expect(written[0]).toEqual({ ...existing, enabled: false })
    expect(written[1]).toEqual({
      id: expect.stringMatching(/^[0-9a-f-]{36}$/),
      enabled: true,
      deviceId: `dev-1`,
      kind: `event`,
      source: `exponential`,
      event: `pr_merged`,
    })
    expect(written[1]!.id).not.toBe(TRIGGER_ID)
  })

  it(`re-mints an id the action does not hold (ids are the mirror's keys)`, async () => {
    selectResults.push([action([])])
    selectResults.push([ownDevice])
    await caller.update({ id: ACTION_ID, triggers: [existing] })
    const written = updates[0]!.triggers as Record<string, unknown>[]
    expect(written[0]!.id).not.toBe(TRIGGER_ID)
  })

  it(`mirrors the triggers into the legacy automations rows, when-part only`, async () => {
    selectResults.push([action([existing])])
    await caller.update({ id: ACTION_ID, triggers: [existing] })
    // Rows the action no longer holds go; the kept one is upserted by id.
    expect(deletes).toEqual([automations])
    expect(inserts).toEqual([
      {
        id: TRIGGER_ID,
        teamId: TEAM_ID,
        actionId: ACTION_ID,
        deviceId: `dev-1`,
        enabled: true,
        trigger: { kind: `schedule`, interval: `daily`, minuteOfDay: 540 },
        agent: `claude`,
        account: null,
        model: null,
        effort: null,
        sortOrder: 1,
      },
    ])
  })

  it(`refuses a device that is neither the caller's nor shared with the team`, async () => {
    selectResults.push([action([])])
    selectResults.push([{ ...ownDevice, userId: `someone-else` }])
    const error = await rejectionOf(
      caller.update({ id: ACTION_ID, triggers: [{ ...existing, id: undefined }] })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`device must be yours`)
    expect(updates).toHaveLength(0)
  })

  it(`refuses an account pin without its agent, and an unknown model`, async () => {
    for (const pins of [{ agent: null, account: `work` }, { model: `gpt-nope` }]) {
      selectResults.length = 0
      selectResults.push([action([existing])])
      const error = await rejectionOf(
        caller.update({ id: ACTION_ID, triggers: [{ ...existing, ...pins }] })
      )
      expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    }
    expect(updates).toHaveLength(0)
  })

  it(`lets the migrated Tidy up row keep its reserved name`, async () => {
    selectResults.push([{ ...action([]), name: `Tidy up` }])
    await caller.update({ id: ACTION_ID, name: `Tidy up`, description: `Mine` })
    expect(updates[0]!.description).toBe(`Mine`)
  })
})
