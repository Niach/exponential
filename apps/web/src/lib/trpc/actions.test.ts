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
        delete: () => ({ where: async () => undefined }),
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
// EXP-707: writes mint a txId sync barrier (automations.test.ts precedent).
vi.mock(`@/lib/trpc`, async (importOriginal) => {
  const mod = await importOriginal<Record<string, unknown>>()
  return { ...mod, generateTxId: async () => 42 }
})

import { actionsRouter } from "@/lib/trpc/actions"

const { selectResults, inserts, updates, fakeDb } = h

const TEAM_ID = `11111111-1111-4111-8111-111111111111`
const ACTION_ID = `22222222-2222-4222-8222-222222222222`
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

// EXP-583: automations are their own rows; the actions router keeps ONE
// guard — an input can't turn required while an ENABLED automation targets
// the action (automated runs fill no inputs).
describe(`actions.update — required inputs vs automations (EXP-583)`, () => {
  const requiredInput = {
    key: `target`,
    label: `Target`,
    type: `icon` as const,
    required: true,
  }
  const optionalInput = { ...requiredInput, required: false }

  it(`refuses adding a required input to an action with an enabled automation`, async () => {
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Sweep`, inputs: [] }])
    // assertNoEnabledAutomation probe finds one.
    selectResults.push([{ id: `auto-1` }])
    const error = await rejectionOf(
      caller.update({ id: ACTION_ID, inputs: [requiredInput] })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`required inputs`)
    expect(updates).toHaveLength(0)
  })

  it(`allows a required input when no enabled automation targets the action`, async () => {
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Sweep`, inputs: [] }])
    selectResults.push([])
    await caller.update({ id: ACTION_ID, inputs: [requiredInput] })
    expect(updates[0]!.inputs).toEqual([requiredInput])
  })

  it(`allows making the inputs optional on an automated action`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Sweep`, inputs: [requiredInput] },
    ])
    await caller.update({ id: ACTION_ID, inputs: [optionalInput] })
    expect(updates[0]!.inputs).toEqual([optionalInput])
  })
})

// ── EXP-825 compat: retired text/textarea input kinds ────────────────────────
// Old action editors (iOS ≤ 0.14.28, Android ≤ 0.14.30, desktop ≤ 0.14.35 and
// its creator run telling the agent `type: text`) still submit free-text
// defs. The boundary drops them, seeding the composer hint from the FIRST
// dropped def (placeholder, else label, LEFT 200) when the row has none —
// migration 0108's rule. The stored shape never carries the retired kinds.

describe(`actions.create/update — EXP-825 compat: retired text inputs`, () => {
  it(`drops text/textarea defs on create and seeds the hint from the first one's placeholder`, async () => {
    selectResults.push([])
    const { action } = await caller.create({
      teamId: TEAM_ID,
      name: `Release`,
      body: `x`,
      inputs: [
        { key: `scope`, label: `Scope`, type: `textarea`, required: true, placeholder: ` Which platforms ` },
        { key: `repo`, label: `Repository`, type: `repo` },
        { key: `notes`, label: `Notes`, type: `text`, placeholder: `Anything else` },
      ],
    })
    expect(inserts[0]!.inputs).toEqual([
      { key: `repo`, label: `Repository`, type: `repo`, required: false },
    ])
    expect(inserts[0]!.promptPlaceholder).toBe(`Which platforms`)
    expect(action).toMatchObject({ promptPlaceholder: `Which platforms` })
  })

  it(`seeds from the label when the dropped def has no placeholder, LEFT 200`, async () => {
    selectResults.push([])
    await caller.create({
      teamId: TEAM_ID,
      name: `Plain text`,
      body: `x`,
      inputs: [{ key: `what`, label: `What to do`, type: `text` }],
    })
    expect(inserts[0]!.inputs).toEqual([])
    expect(inserts[0]!.promptPlaceholder).toBe(`What to do`)

    selectResults.push([])
    await caller.create({
      teamId: TEAM_ID,
      name: `Long`,
      body: `x`,
      inputs: [
        { key: `what`, label: `L`, type: `text`, placeholder: `p`.repeat(200) },
      ],
    })
    // The def placeholder is capped at 200 itself; the seed never exceeds it.
    expect((inserts[1]!.promptPlaceholder as string).length).toBe(200)
  })

  it(`keeps a hint the caller sent instead of seeding`, async () => {
    selectResults.push([])
    await caller.create({
      teamId: TEAM_ID,
      name: `Hinted`,
      body: `x`,
      inputs: [{ key: `what`, label: `What`, type: `text`, placeholder: `Old` }],
      promptPlaceholder: `New hint`,
    })
    expect(inserts[0]!.promptPlaceholder).toBe(`New hint`)
    expect(inserts[0]!.inputs).toEqual([])
  })

  it(`update drops the retired defs and seeds only a row with no hint`, async () => {
    // loadAction: a row WITHOUT a hint → seeded.
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Old`, inputs: [], promptPlaceholder: null },
    ])
    await caller.update({
      id: ACTION_ID,
      inputs: [
        { key: `topic`, label: `Topic`, type: `text`, placeholder: `Which topic` },
        { key: `board`, label: `Board`, type: `board` },
      ],
    })
    expect(updates[0]!.inputs).toEqual([
      { key: `board`, label: `Board`, type: `board`, required: false },
    ])
    expect(updates[0]!.promptPlaceholder).toBe(`Which topic`)

    // A row that already has a hint keeps it.
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Old`, inputs: [], promptPlaceholder: `Keep me` },
    ])
    await caller.update({
      id: ACTION_ID,
      inputs: [{ key: `topic`, label: `Topic`, type: `text`, placeholder: `Which topic` }],
    })
    expect(updates[1]!.inputs).toEqual([])
    expect(`promptPlaceholder` in updates[1]!).toBe(false)
  })

  it(`a dropped required text def is not a required input for the automation guard`, async () => {
    // loadAction only — no automation probe select is queued; if the guard
    // ran it would pop an empty result and pass anyway, so assert the
    // stored shape carries no required input.
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Old`, inputs: [] }])
    await caller.update({
      id: ACTION_ID,
      inputs: [{ key: `what`, label: `What`, type: `textarea`, required: true }],
    })
    expect(updates[0]!.inputs).toEqual([])
    expect(selectResults).toHaveLength(0)
  })

  it(`still rejects an unknown input kind`, async () => {
    const error = await rejectionOf(
      caller.create({
        teamId: TEAM_ID,
        name: `Bogus`,
        body: `x`,
        inputs: [{ key: `x`, label: `X`, type: `number` as never }],
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(inserts).toHaveLength(0)
  })
})
