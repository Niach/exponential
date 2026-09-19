import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-981 — the workflows router's refusals. The graph itself (folding,
// layout, edges) is covered by lib/workflows.test.ts + workflow-layout.test.ts;
// this proves the write path's gates: membership, ONE repository, backlog
// issues only, a draft-only configuration and the launch vocabulary.
//
// Fake-db harness as in relations.test.ts: a FIFO select queue.

const h = vi.hoisted(() => ({
  assertTeamMember: vi.fn(async (..._args: unknown[]) => ({ role: `member` })),
  assertDeviceUsable: vi.fn(async (..._args: unknown[]) => {}),
  replanWorkflow: vi.fn(async (..._args: unknown[]) => ({
    nodes: 2,
    edges: 0,
    depth: 1,
    width: 2,
    cycles: [],
  })),
}))

const dbHolder = vi.hoisted(() => ({ db: {} as Record<string, unknown> }))
vi.mock(`@/db/connection`, () => ({
  get db() {
    return dbHolder.db
  },
}))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({ assertTeamMember: h.assertTeamMember }))
vi.mock(`@/lib/trpc/automations`, () => ({ assertDeviceUsable: h.assertDeviceUsable }))
vi.mock(`@/lib/workflows`, () => ({
  nodeEdges: () => [],
  replanWorkflow: h.replanWorkflow,
  workflowIntegrationBranch: (id: string) => `exp/wf-${id.slice(0, 8)}`,
}))

const selectQueue: unknown[][] = []
type Chain = Promise<unknown[]> & Record<string, (arg?: unknown) => unknown>
function chain(result: unknown[]): Chain {
  const p = Promise.resolve(result) as Chain
  for (const m of [`from`, `where`, `innerJoin`, `limit`, `orderBy`, `values`, `set`, `returning`]) {
    p[m] = () => p
  }
  return p
}
const written: Array<{ op: string; values?: unknown }> = []
const fakeDb = {
  select: vi.fn(() => chain(selectQueue.shift() ?? [])),
  insert: vi.fn(() => {
    const p = chain([{ id: `wf-1`, metrics: {} }])
    p.values = (values: unknown) => {
      written.push({ op: `insert`, values })
      return p
    }
    return p
  }),
  update: vi.fn(() => {
    const p = chain([{ id: `wf-1` }])
    p.set = (values: unknown) => {
      written.push({ op: `update`, values })
      return p
    }
    return p
  }),
  delete: vi.fn(() => chain([])),
  execute: vi.fn(async () => ({ rows: [{ txid: `7` }] })),
  transaction: vi.fn(async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeDb)),
}
dbHolder.db = fakeDb

vi.mock(`@/lib/steer`, () => ({ getSteerRelayConfig: () => null, relayPostInput: vi.fn() }))
vi.mock(`@/lib/steer-child-messages`, () => ({ oneLine: (text: string) => text }))

import {
  appendDecisionLine,
  nodeNeedsApproval,
  workflowsRouter,
} from "@/lib/trpc/workflows"

const caller = workflowsRouter.createCaller({
  db: fakeDb,
  session: { user: { id: `user-1` } },
} as never)

const TEAM = `11111111-1111-4111-8111-111111111111`
const WF = `22222222-2222-4222-8222-222222222222`
const A = `33333333-3333-4333-8333-333333333333`
const B = `44444444-4444-4444-8444-444444444444`
const issue = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  identifier: id === A ? `APP-6` : `APP-10`,
  number: id === A ? 6 : 10,
  status: `backlog`,
  teamId: TEAM,
  repositoryId: `repo-1`,
  ...over,
})
const workflow = (over: Record<string, unknown> = {}) => ({
  id: WF,
  teamId: TEAM,
  repositoryId: `repo-1`,
  status: `draft`,
  deviceId: null,
  launch: {},
  ...over,
})
const rejection = async (p: Promise<unknown>) => p.then(() => null, (e: unknown) => e as TRPCError)

beforeEach(() => {
  selectQueue.length = 0
  written.length = 0
  vi.clearAllMocks()
})

describe(`workflows.create`, () => {
  it(`names the draft after its lowest issue NUMBER and lays it out`, async () => {
    selectQueue.push([issue(B), issue(A)])
    const result = await caller.create({ teamId: TEAM, issueIds: [B, A] })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, TEAM)
    expect(written[0]!.values).toMatchObject({ name: `APP-6 +1`, repositoryId: `repo-1` })
    expect(h.replanWorkflow).toHaveBeenCalledTimes(1)
    expect(result.workflow.metrics).toMatchObject({ nodes: 2 })
  })

  it.each([
    [`a started issue`, [issue(A, { status: `in_progress` })], `already started`],
    [`a board without a repository`, [issue(A, { repositoryId: null })], `without a repository`],
    [`another team's issue`, [issue(A, { teamId: `other` })], `another team`],
    [`an unknown issue`, [], `not found`],
  ])(`refuses %s`, async (_name, rows, message) => {
    selectQueue.push(rows)
    const error = await rejection(caller.create({ teamId: TEAM, issueIds: [A] }))
    expect(error?.code).toBe(`BAD_REQUEST`)
    expect(error?.message).toContain(message)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`refuses issues of two repositories`, async () => {
    selectQueue.push([issue(A), issue(B, { repositoryId: `repo-2` })])
    const error = await rejection(caller.create({ teamId: TEAM, issueIds: [A, B] }))
    expect(error?.message).toContain(`ONE repository`)
  })
})

describe(`workflows.update`, () => {
  it(`renames a running workflow but refuses to reconfigure it`, async () => {
    selectQueue.push([workflow({ status: `running` })])
    await caller.update({ id: WF, name: `Renamed` })
    expect(written[0]!.values).toEqual({ name: `Renamed` })

    selectQueue.push([workflow({ status: `running` })])
    const error = await rejection(caller.update({ id: WF, gate: `none` }))
    expect(error?.code).toBe(`PRECONDITION_FAILED`)
  })

  it(`validates the launch vocabulary per agent`, async () => {
    selectQueue.push([workflow()])
    const codex = await rejection(
      caller.update({ id: WF, launch: { agent: `codex`, subagentModel: `opus` } })
    )
    expect(codex?.message).toBe(`Only claude takes a subagent model`)

    selectQueue.push([workflow()])
    const model = await rejection(caller.update({ id: WF, launch: { model: `gpt-nope` } }))
    expect(model?.message).toBe(`Unknown claude model`)
  })

  it(`checks a runner against the workflows capability`, async () => {
    selectQueue.push([workflow()])
    await caller.update({ id: WF, deviceId: `dev-1`, launch: { agent: `claude` } })
    expect(h.assertDeviceUsable).toHaveBeenCalledWith(
      `dev-1`,
      TEAM,
      `user-1`,
      `claude`,
      expect.objectContaining({ cap: `workflows` })
    )
  })
})

describe(`workflows.delete`, () => {
  it(`refuses while the workflow runs`, async () => {
    selectQueue.push([workflow({ status: `paused` })])
    const error = await rejection(caller.delete({ id: WF }))
    expect(error?.code).toBe(`PRECONDITION_FAILED`)
    expect(fakeDb.delete).not.toHaveBeenCalled()
  })
})

// EXP-982 — running a workflow.
describe(`workflows.start`, () => {
  const ready = { deviceId: `dev-1`, startOn: `landed`, decisions: `` }

  it(`refuses a workflow with no runner, a speculative start, or a cycle`, async () => {
    selectQueue.push([workflow({ ...ready, deviceId: null })])
    expect((await rejection(caller.start({ id: WF })))?.message).toContain(`Pick the device`)

    selectQueue.push([workflow({ ...ready, startOn: `contract` })])
    expect((await rejection(caller.start({ id: WF })))?.message).toContain(`When landed`)

    selectQueue.push([workflow(ready)])
    h.replanWorkflow.mockResolvedValueOnce({
      nodes: 2,
      edges: 2,
      depth: 1,
      width: 2,
      cycles: [[`APP-1`, `APP-2`]],
    } as never)
    const cyclic = await rejection(caller.start({ id: WF }))
    expect(cyclic?.message).toContain(`APP-1, APP-2`)
  })

  it(`resets every node and flips the workflow to running`, async () => {
    selectQueue.push([workflow(ready)])
    await caller.start({ id: WF })
    expect(h.assertDeviceUsable).toHaveBeenCalled()
    expect(written.map((w) => w.values)).toEqual([
      expect.objectContaining({ state: `blocked`, attempt: 0, sessionId: null }),
      expect.objectContaining({ status: `running` }),
    ])
  })
})

describe(`the engine's write path`, () => {
  const node = (over: Record<string, unknown> = {}) => ({
    id: `node-1`,
    workflowId: WF,
    issueId: A,
    kind: `leaf`,
    state: `in_review`,
    approvedAt: null,
    ...over,
  })
  const NODE = `55555555-5555-4555-8555-555555555555`

  it(`refuses a caller that does not own the runner device`, async () => {
    selectQueue.push([node()], [workflow({ status: `running`, deviceId: `dev-1` })], [])
    const error = await rejection(caller.reportNode({ nodeId: NODE, state: `running` }))
    expect(error?.code).toBe(`FORBIDDEN`)
  })

  it(`never lets a report make or unmake a landed node`, async () => {
    selectQueue.push(
      [node({ state: `landed` })],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }]
    )
    expect(await caller.reportNode({ nodeId: NODE, state: `failed` })).toEqual({
      updated: false,
    })
  })

  it(`holds an unapproved node at the gate, server-side`, async () => {
    selectQueue.push(
      [node()],
      [workflow({ status: `running`, deviceId: `dev-1`, gate: `human` })],
      [{ id: `device-row` }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: false,
      reason: `Waiting for a person to approve`,
    })
  })
})

describe(`nodeNeedsApproval`, () => {
  it(`always gates the contract, and everything under a gate`, () => {
    expect(nodeNeedsApproval(`none`, `leaf`)).toBe(false)
    expect(nodeNeedsApproval(`none`, `contract`)).toBe(true)
    expect(nodeNeedsApproval(`human`, `leaf`)).toBe(true)
    expect(nodeNeedsApproval(`agent`, `integration`)).toBe(true)
  })
})

describe(`appendDecisionLine`, () => {
  const now = new Date(`2026-09-19T10:00:00Z`)

  it(`appends one dated line`, () => {
    expect(appendDecisionLine(``, `  use\ncursors `, now)).toBe(`2026-09-19: use cursors`)
    expect(appendDecisionLine(`2026-09-18: a`, `b`, now)).toBe(
      `2026-09-18: a\n2026-09-19: b`
    )
  })

  it(`drops the OLDEST lines when the log is full`, () => {
    const full = Array.from({ length: 40 }, (_, i) => `l${i}: ${`x`.repeat(1990)}`).join(`\n`)
    const next = appendDecisionLine(full, `newest`, now)
    expect(next.length).toBeLessThanOrEqual(65536)
    expect(next.endsWith(`2026-09-19: newest`)).toBe(true)
    expect(next.startsWith(`l0:`)).toBe(false)
  })
})
