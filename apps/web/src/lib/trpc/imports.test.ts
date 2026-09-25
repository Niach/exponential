import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-630 imports router: owner-only everywhere, the credential never in
// any selection, discovery handed to the worker, start gated on a clean
// dry run, cancel only from a live state. Fake-db harness mirrors
// team-invites.test.ts: `select()` shifts rows off a FIFO queue,
// `insert()`/`update()` record their values and serve `.returning()` from
// their own queues.

const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  const inserts: { values: Record<string, unknown> }[] = []
  const insertReturningQueue: unknown[][] = []
  const updates: { set: Record<string, unknown>; where: unknown }[] = []
  const updateReturningQueue: unknown[][] = []
  // EXP-1076: cancel discards a not-yet-started job outright.
  const deletes: { where: unknown }[] = []
  const deleteReturningQueue: unknown[][] = []

  const selectWheres: unknown[] = []

  function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
    const p = Promise.resolve(selectQueue.shift() ?? []) as Promise<unknown[]> &
      Record<string, (arg?: unknown) => unknown>
    for (const m of [`from`, `where`, `orderBy`, `limit`]) p[m] = () => p
    p.where = (arg?: unknown) => {
      selectWheres.push(arg)
      return p
    }
    return p as Promise<unknown[]> & Record<string, () => unknown>
  }

  const fakeDb = {
    select: () => selectChain(),
    insert: () => ({
      values: (values: Record<string, unknown>) => {
        inserts.push({ values })
        return { returning: async () => insertReturningQueue.shift() ?? [] }
      },
    }),
    update: () => ({
      set: (set: Record<string, unknown>) => ({
        where: (where: unknown) => {
          updates.push({ set, where })
          return { returning: async () => updateReturningQueue.shift() ?? [] }
        },
      }),
    }),
    delete: () => ({
      where: (where: unknown) => {
        deletes.push({ where })
        return { returning: async () => deleteReturningQueue.shift() ?? [] }
      },
    }),
  }

  return {
    selectQueue,
    inserts,
    insertReturningQueue,
    updates,
    updateReturningQueue,
    deletes,
    deleteReturningQueue,
    selectWheres,
    fakeDb,
    resolveTeamAccess: vi.fn(async (..._args: unknown[]) => ({}) as unknown),
    validateCredential: vi.fn(async (_key: string) => ({ ok: true as const, who: `me@linear` })),
    toBundle: vi.fn(),
    loadImportTeamState: vi.fn(),
    evaluatePlan: vi.fn(() => ({ blockers: [] as string[], warnings: [] as string[], counts: {} })),
    kick: vi.fn(),
  }
})

const {
  selectQueue,
  inserts,
  insertReturningQueue,
  updates,
  updateReturningQueue,
  deletes,
  deleteReturningQueue,
  selectWheres,
  fakeDb,
} = h

vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({ resolveTeamAccess: h.resolveTeamAccess }))
vi.mock(`@/lib/import/sources`, () => ({
  getImportSource: () => ({
    id: `linear`,
    label: `Linear`,
    validateCredential: h.validateCredential,
    toBundle: h.toBundle,
    discover: vi.fn(),
    fetchAsset: vi.fn(),
  }),
  isImportSource: () => true,
}))
vi.mock(`@/lib/import/team-state`, () => ({ loadImportTeamState: h.loadImportTeamState }))
vi.mock(`@/lib/import/plan`, () => ({ evaluatePlan: h.evaluatePlan }))
vi.mock(`@/lib/import/kick`, () => ({ kickImportWorker: h.kick }))

import { importsRouter, jobSelection } from "@/lib/trpc/imports"
import { IMPORT_TERMINAL_STATUSES } from "@/lib/import/bundle"
import { importJobs } from "@/db/schema"

// Drizzle conditions are SQL trees; the tests read the columns and the
// literal values out of them (the billing.test.ts pattern).
function flattenSqlChunks(node: unknown): unknown[] {
  const chunks = (node as { queryChunks?: unknown[] }).queryChunks
  if (!Array.isArray(chunks)) return [node]
  return chunks.flatMap(flattenSqlChunks)
}

// The bound literals of a condition: a status fence renders its values as a
// nested array of params, so walk into arrays and keep what a Param holds.
function sqlParamValues(node: unknown): unknown[] {
  const out: unknown[] = []
  const visit = (leaf: unknown) => {
    if (Array.isArray(leaf)) {
      leaf.forEach(visit)
      return
    }
    const value = (leaf as { value?: unknown })?.value
    if (value === undefined) return
    if (Array.isArray(value)) value.forEach(visit)
    else out.push(value)
  }
  flattenSqlChunks(node).forEach(visit)
  return out
}

const TEAM = `11111111-1111-4111-8111-111111111111`
const JOB = `22222222-2222-4222-8222-222222222222`
const ME = `user-me`

function caller() {
  return importsRouter.createCaller({
    db: fakeDb,
    request: new Request(`http://localhost/`),
    session: { user: { id: ME } },
  } as never)
}

const PLAN = {
  routing: `team`,
  importHistory: true,
  importArchived: true,
  boards: { "team:a": { mode: `create`, name: `A`, prefix: `A`, numbering: `preserve` } },
  statuses: {},
  labels: {},
  users: {},
}

function jobRow(overrides: Record<string, unknown> = {}) {
  return {
    id: JOB,
    teamId: TEAM,
    createdByUserId: ME,
    source: `linear`,
    status: `draft`,
    preview: null,
    plan: PLAN,
    progress: null,
    counts: null,
    error: null,
    startedAt: null,
    finishedAt: null,
    createdAt: new Date(`2026-09-23T00:00:00Z`),
    updatedAt: new Date(`2026-09-23T00:00:00Z`),
    ...overrides,
  }
}

beforeEach(() => {
  selectQueue.length = 0
  inserts.length = 0
  insertReturningQueue.length = 0
  updates.length = 0
  updateReturningQueue.length = 0
  deletes.length = 0
  deleteReturningQueue.length = 0
  selectWheres.length = 0
  h.resolveTeamAccess.mockReset().mockResolvedValue({})
  h.validateCredential.mockReset().mockResolvedValue({ ok: true, who: `me@linear` })
  h.evaluatePlan.mockReset().mockReturnValue({ blockers: [], warnings: [], counts: {} })
  h.toBundle.mockReset().mockReturnValue({ source: `linear` })
  h.loadImportTeamState.mockReset().mockResolvedValue({})
  h.kick.mockReset()
})

describe(`jobSelection`, () => {
  it(`never selects the credential, the payload or the claim token`, () => {
    const selected = new Set<unknown>(Object.values(jobSelection))
    expect(selected.has(importJobs.credential)).toBe(false)
    expect(selected.has(importJobs.payload)).toBe(false)
    expect(selected.has(importJobs.claimToken)).toBe(false)
    expect(selected.has(importJobs.id)).toBe(true)
  })
})

describe(`imports.connect`, () => {
  it(`requires the team OWNER`, async () => {
    h.resolveTeamAccess.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    await expect(
      caller().connect({ teamId: TEAM, source: `linear`, apiKey: `lin_api_x` })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(ME, TEAM, `mutate_resources`, { roles: [`owner`] })
    expect(inserts).toEqual([])
  })

  it(`refuses a credential the adapter rejects`, async () => {
    h.validateCredential.mockResolvedValueOnce({ ok: false, reason: `nope` } as never)
    await expect(
      caller().connect({ teamId: TEAM, source: `linear`, apiKey: `bad` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST`, message: `nope` })
    expect(inserts).toEqual([])
    expect(h.kick).not.toHaveBeenCalled()
  })

  it(`creates a previewing job holding the credential and kicks the worker`, async () => {
    insertReturningQueue.push([{ id: JOB }])
    const result = await caller().connect({ teamId: TEAM, source: `linear`, apiKey: ` lin_api_ok ` })
    expect(result).toEqual({ jobId: JOB, connectedAs: `me@linear` })
    expect(inserts[0]!.values).toMatchObject({
      teamId: TEAM,
      createdByUserId: ME,
      source: `linear`,
      status: `previewing`,
      credential: `lin_api_ok`,
    })
    expect(h.kick).toHaveBeenCalledTimes(1)
  })
})

describe(`imports.get`, () => {
  it(`loads the job for its team's owner and parses the jsonb columns`, async () => {
    selectQueue.push([jobRow()])
    const job = await caller().get({ jobId: JOB })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(ME, TEAM, `mutate_resources`, { roles: [`owner`] })
    expect(job.plan).toEqual(PLAN)
    expect(job.status).toBe(`draft`)
    expect(job).not.toHaveProperty(`credential`)
  })

  it(`404s an unknown job before any team check`, async () => {
    await expect(caller().get({ jobId: JOB })).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(h.resolveTeamAccess).not.toHaveBeenCalled()
  })
})

describe(`imports.start`, () => {
  it(`refuses when the dry run has blockers`, async () => {
    selectQueue.push([jobRow()], [{ payload: {} }])
    h.evaluatePlan.mockReturnValueOnce({ blockers: [`Prefix taken`], warnings: [], counts: {} })
    await expect(caller().start({ jobId: JOB })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `The dry run has blockers: Prefix taken`,
    })
    expect(updates).toEqual([])
  })

  it(`moves a clean draft to ready and kicks the worker`, async () => {
    selectQueue.push([jobRow()], [{ payload: {} }])
    updateReturningQueue.push([{ id: JOB }])
    await caller().start({ jobId: JOB })
    expect(updates[0]!.set).toMatchObject({ status: `ready`, error: null })
    expect(h.kick).toHaveBeenCalledTimes(1)
  })

  it(`refuses from a running job`, async () => {
    selectQueue.push([jobRow({ status: `running` })])
    await expect(caller().start({ jobId: JOB })).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
  })
})

describe(`imports.savePlan`, () => {
  it(`stores a valid plan on a draft`, async () => {
    selectQueue.push([jobRow()])
    updateReturningQueue.push([{ id: JOB }])
    await caller().savePlan({ jobId: JOB, plan: PLAN as never })
    expect(updates[0]!.set).toEqual({ plan: PLAN })
  })

  it(`rejects a malformed plan at the boundary`, async () => {
    selectQueue.push([jobRow()])
    await expect(
      caller().savePlan({ jobId: JOB, plan: { ...PLAN, boards: { x: { mode: `elsewhere` } } } as never })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})

describe(`imports.dryRun`, () => {
  it(`says the data expired once the worker purged the payload`, async () => {
    selectQueue.push([jobRow({ status: `failed` })], [{ payload: null }])
    await expect(caller().dryRun({ jobId: JOB })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: /expired/,
    })
    expect(h.toBundle).not.toHaveBeenCalled()
  })
})

describe(`imports.cancel`, () => {
  it(`cancels a live job, wipes the credential AND nulls the claim token in the same UPDATE`, async () => {
    selectQueue.push([jobRow({ status: `running` })])
    updateReturningQueue.push([{ id: JOB }])
    await caller().cancel({ jobId: JOB })
    // The worker fences every write on `claim_token = mine`; a NULL token
    // (plus the status flip) is what stops a finishing discovery or a
    // failure from writing over `cancelled`.
    expect(updates[0]!.set).toMatchObject({ status: `cancelled`, credential: null, claimToken: null })
    expect(updates[0]!.set.finishedAt).toBeInstanceOf(Date)
    // EXP-1076: the discard DELETE is fenced on draft|previewing, so a
    // running job can never match it — rows it already wrote must stay.
    expect(deleteReturningQueue).toHaveLength(0)
  })

  it(`refuses once the job already finished`, async () => {
    selectQueue.push([jobRow({ status: `completed` })])
    await expect(caller().cancel({ jobId: JOB })).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
  })

  // EXP-1076: backing out before the import started leaves no trace.
  it.each([`draft`, `previewing`])(
    `discards a %s job outright instead of parking a cancelled card`,
    async (status) => {
      selectQueue.push([jobRow({ status })])
      deleteReturningQueue.push([{ id: JOB }])

      await expect(caller().cancel({ jobId: JOB })).resolves.toEqual({
        ok: true,
        discarded: true,
      })

      expect(deletes).toHaveLength(1)
      const where = flattenSqlChunks(deletes[0]!.where)
      expect(where).toContain(importJobs.id)
      expect(where).toContain(importJobs.status)
      const fence = sqlParamValues(deletes[0]!.where)
      expect(fence).toContain(`draft`)
      expect(fence).toContain(`previewing`)
      expect(fence).not.toContain(`running`)
      // Nothing was written outside the job row in those states, so there is
      // no cancelled card to keep — and no UPDATE at all.
      expect(updates).toHaveLength(0)
    }
  )

  it(`falls back to the cancel UPDATE when the fenced delete matched nothing`, async () => {
    // The job reached `ready` between the read and the write: the DELETE is
    // fenced on draft|previewing, so it returns 0 rows and the park wins.
    selectQueue.push([jobRow({ status: `draft` })])
    deleteReturningQueue.push([])
    updateReturningQueue.push([{ id: JOB }])

    await expect(caller().cancel({ jobId: JOB })).resolves.toEqual({
      ok: true,
      discarded: false,
    })
    expect(deletes).toHaveLength(1)
    expect(updates[0]!.set).toMatchObject({ status: `cancelled`, credential: null })
  })
})

// EXP-1076: clearing the Recent imports list hides the CARDS only —
// import_entity_map (the re-import idempotency key) must outlive them.
describe(`imports.clearHistory`, () => {
  it(`requires the team OWNER`, async () => {
    h.resolveTeamAccess.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    await expect(caller().clearHistory({ teamId: TEAM })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(ME, TEAM, `mutate_resources`, {
      roles: [`owner`],
    })
    expect(updates).toEqual([])
  })

  it(`dismisses only this team's FINISHED, not-yet-cleared jobs and counts them`, async () => {
    updateReturningQueue.push([{ id: JOB }, { id: `other` }])

    await expect(caller().clearHistory({ teamId: TEAM })).resolves.toEqual({ cleared: 2 })

    expect(updates).toHaveLength(1)
    expect(updates[0]!.set.dismissedAt).toBeInstanceOf(Date)
    const where = flattenSqlChunks(updates[0]!.where)
    expect(where).toContain(importJobs.teamId)
    expect(where).toContain(importJobs.status)
    expect(where).toContain(importJobs.dismissedAt)
    // A live job would reappear the moment it writes — terminal states only.
    const fence = sqlParamValues(updates[0]!.where)
    for (const terminal of [...IMPORT_TERMINAL_STATUSES]) {
      expect(fence).toContain(terminal)
    }
    for (const live of [`draft`, `previewing`, `ready`, `running`]) {
      expect(fence).not.toContain(live)
    }
    // Never deletes: the entity map keyed to these jobs stays.
    expect(deletes).toEqual([])
  })

  it(`reports zero when there is nothing to clear`, async () => {
    await expect(caller().clearHistory({ teamId: TEAM })).resolves.toEqual({ cleared: 0 })
  })
})

describe(`imports.list`, () => {
  it(`hides cleared cards`, async () => {
    selectQueue.push([jobRow({ status: `completed` })])
    await caller().list({ teamId: TEAM })
    const where = flattenSqlChunks(selectWheres.at(-1))
    expect(where).toContain(importJobs.teamId)
    expect(where).toContain(importJobs.dismissedAt)
  })
})
