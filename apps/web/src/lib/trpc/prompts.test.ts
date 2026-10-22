import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// Prompts router (NEW-2: actions became PROMPTS): member reads, `get`
// returns the body, owner-only writes, names stay unique per team; no
// inputs, triggers, repository or MCP list. DB access is a queued
// select-chain + insert recorder (steer.test.ts precedent).

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
                returning: async () => [{ id: `new-prompt`, ...values }],
              }),
            }
          },
        }),
        update: () => ({
          set: (values: Record<string, unknown>) => {
            updates.push(values)
            return {
              where: () => ({
                returning: async () => [{ id: `updated-prompt`, ...values }],
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
// loadPrompt imports the db lazily — same fake.
vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
// EXP-707: writes mint a txId sync barrier.
vi.mock(`@/lib/trpc`, async (importOriginal) => {
  const mod = await importOriginal<Record<string, unknown>>()
  return { ...mod, generateTxId: async () => 42 }
})

import { promptsRouter } from "@/lib/trpc/prompts"
import { prompts } from "@/db/schema"

const { selectResults, inserts, updates, deletes, fakeDb } = h

const TEAM_ID = `11111111-1111-4111-8111-111111111111`
const PROMPT_ID = `22222222-2222-4222-8222-222222222222`

const caller = promptsRouter.createCaller({
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

const row = (over: Record<string, unknown> = {}) => ({
  id: PROMPT_ID,
  teamId: TEAM_ID,
  name: `Code review`,
  icon: null,
  body: `Review the diff`,
  sortOrder: 1,
  ...over,
})

beforeEach(() => {
  selectResults.length = 0
  inserts.length = 0
  updates.length = 0
  deletes.length = 0
  h.assertTeamMember.mockClear()
  h.assertTeamOwner.mockClear()
  h.assertTeamOwner.mockResolvedValue({ role: `owner` })
})

describe(`prompts.list / get`, () => {
  it(`lists the team's rows as they are, member-gated`, async () => {
    selectResults.push([{ id: PROMPT_ID, teamId: TEAM_ID, name: `Code review` }])
    const { prompts: rows } = await caller.list({ teamId: TEAM_ID })
    expect(rows).toEqual([{ id: PROMPT_ID, teamId: TEAM_ID, name: `Code review` }])
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`get returns the body, member-gated`, async () => {
    selectResults.push([row()])
    const { prompt } = await caller.get({ id: PROMPT_ID })
    expect(prompt.body).toBe(`Review the diff`)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`get 404s a vanished row and refuses a non-uuid id`, async () => {
    selectResults.push([])
    expect(((await rejectionOf(caller.get({ id: PROMPT_ID }))) as TRPCError).code).toBe(`NOT_FOUND`)
    expect(
      ((await rejectionOf(caller.get({ id: `builtin:create-prompt` }))) as TRPCError).code
    ).toBe(`BAD_REQUEST`)
  })
})

describe(`prompts.create`, () => {
  it(`persists name, icon and body and appends to the end`, async () => {
    selectResults.push([{ sortOrder: 4 }])
    const { prompt } = await caller.create({
      teamId: TEAM_ID,
      name: `Weekly review`,
      body: `Do the thing`,
      icon: `sparkles`,
    })
    expect(inserts[0]).toMatchObject({ name: `Weekly review`, icon: `sparkles`, sortOrder: 5 })
    expect(prompt).toMatchObject({ name: `Weekly review`, body: `Do the thing` })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`refuses the retired action fields and a blank body`, async () => {
    for (const input of [
      { teamId: TEAM_ID, name: `Inputs`, body: `x`, inputs: [{ key: `a`, label: `A`, type: `repo` }] },
      { teamId: TEAM_ID, name: `Hint`, body: `x`, promptPlaceholder: `Scope` },
      { teamId: TEAM_ID, name: `Blank`, body: `   ` },
    ]) {
      const error = await rejectionOf(caller.create(input as never))
      expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    }
    expect(inserts).toHaveLength(0)
  })

  it(`refuses a non-owner before any write`, async () => {
    h.assertTeamOwner.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN`, message: `Owner only` })
    )
    const error = await rejectionOf(caller.create({ teamId: TEAM_ID, name: `X`, body: `x` }))
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })

  it(`answers a duplicate name with CONFLICT`, async () => {
    selectResults.push([])
    const insert = fakeDb.insert as () => unknown
    fakeDb.insert = () => ({
      values: () => ({ onConflictDoNothing: () => ({ returning: async () => [] }) }),
    })
    try {
      const error = await rejectionOf(caller.create({ teamId: TEAM_ID, name: `Code review`, body: `x` }))
      expect((error as TRPCError).code).toBe(`CONFLICT`)
    } finally {
      fakeDb.insert = insert
    }
  })
})

describe(`prompts.update / delete`, () => {
  it(`writes only the given fields, owner-gated on the row's team`, async () => {
    selectResults.push([row()])
    await caller.update({ id: PROMPT_ID, body: `New body`, icon: null })
    expect(updates[0]).toEqual({ body: `New body`, icon: null })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`returns the row unchanged for an empty patch`, async () => {
    selectResults.push([row()])
    const { prompt } = await caller.update({ id: PROMPT_ID })
    expect(prompt).toMatchObject({ id: PROMPT_ID, body: `Review the diff` })
    expect(updates).toHaveLength(0)
  })

  it(`refuses a rename onto another prompt's name`, async () => {
    selectResults.push([row()], [{ id: `other` }])
    const error = await rejectionOf(caller.update({ id: PROMPT_ID, name: `Taken` }))
    expect((error as TRPCError).code).toBe(`CONFLICT`)
    expect(updates).toHaveLength(0)
  })

  it(`delete removes the row, owner-gated`, async () => {
    selectResults.push([row()])
    await caller.delete({ id: PROMPT_ID })
    expect(deletes).toEqual([prompts])
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`delete refuses a non-owner`, async () => {
    selectResults.push([row()])
    h.assertTeamOwner.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN`, message: `Owner only` })
    )
    const error = await rejectionOf(caller.delete({ id: PROMPT_ID }))
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(deletes).toHaveLength(0)
  })
})
