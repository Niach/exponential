import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-257 actions router: the virtual builtin "Create action" rides every
// list (non-editable/non-deletable), inputs schemas persist through
// create/update, and the reserved name stays unique. DB access is a queued
// select-chain + insert recorder (steer.test.ts precedent).

const h = vi.hoisted(() => {
  const selectResults: unknown[][] = []
  const inserts: Record<string, unknown>[] = []
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
    fakeDb: {
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
    },
  }
})

vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  assertTeamOwner: h.assertTeamOwner,
}))
// loadAction / assertRepoInTeam import the db lazily — same fake.
vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))

import { actionsRouter } from "@/lib/trpc/actions"

const { selectResults, inserts, fakeDb } = h

const TEAM_ID = `11111111-1111-4111-8111-111111111111`
const ACTION_ID = `22222222-2222-4222-8222-222222222222`
const BUILTIN_ID = `builtin:create-action`

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
  h.assertTeamMember.mockClear()
  h.assertTeamOwner.mockClear()
})

describe(`actions.list — builtin injection (EXP-257)`, () => {
  it(`appends the virtual Create action and flags real rows builtin: false`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Code review`, inputs: [] },
    ])
    const { actions } = await caller.list({ teamId: TEAM_ID })
    expect(actions).toHaveLength(2)
    expect(actions[0]).toMatchObject({ id: ACTION_ID, builtin: false })
    expect(actions[1]).toMatchObject({
      id: BUILTIN_ID,
      teamId: TEAM_ID,
      name: `Create action`,
      builtin: true,
      inputs: [
        { key: `description`, type: `text`, required: true },
        { key: `repo`, type: `repo`, required: false },
      ],
    })
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
})

describe(`actions.create — inputs + reserved name (EXP-257)`, () => {
  it(`persists a valid inputs schema`, async () => {
    // sortOrder probe select.
    selectResults.push([])
    const inputs = [
      { key: `topic`, label: `Topic`, type: `text` as const, required: true },
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
          { key: `a`, label: `A`, type: `text` },
          { key: `a`, label: `B`, type: `text` },
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
