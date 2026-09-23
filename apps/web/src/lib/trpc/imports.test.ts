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
  const updates: { set: Record<string, unknown> }[] = []
  const updateReturningQueue: unknown[][] = []

  function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
    const p = Promise.resolve(selectQueue.shift() ?? []) as Promise<unknown[]> &
      Record<string, () => unknown>
    for (const m of [`from`, `where`, `orderBy`, `limit`]) p[m] = () => p
    return p
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
      set: (set: Record<string, unknown>) => {
        updates.push({ set })
        return { where: () => ({ returning: async () => updateReturningQueue.shift() ?? [] }) }
      },
    }),
  }

  return {
    selectQueue,
    inserts,
    insertReturningQueue,
    updates,
    updateReturningQueue,
    fakeDb,
    resolveTeamAccess: vi.fn(async (..._args: unknown[]) => ({}) as unknown),
    validateCredential: vi.fn(async (_key: string) => ({ ok: true as const, who: `me@linear` })),
    toBundle: vi.fn(),
    loadImportTeamState: vi.fn(),
    evaluatePlan: vi.fn(() => ({ blockers: [] as string[], warnings: [] as string[], counts: {} })),
    kick: vi.fn(),
  }
})

const { selectQueue, inserts, insertReturningQueue, updates, updateReturningQueue, fakeDb } = h

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
import { importJobs } from "@/db/schema"

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

describe(`imports.cancel`, () => {
  it(`cancels a live job and wipes the credential in the same UPDATE`, async () => {
    selectQueue.push([jobRow({ status: `running` })])
    updateReturningQueue.push([{ id: JOB }])
    await caller().cancel({ jobId: JOB })
    expect(updates[0]!.set).toMatchObject({ status: `cancelled`, credential: null })
  })

  it(`refuses once the job already finished`, async () => {
    selectQueue.push([jobRow({ status: `completed` })])
    await expect(caller().cancel({ jobId: JOB })).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
  })
})
