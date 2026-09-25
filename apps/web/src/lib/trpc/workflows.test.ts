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
  loadWorkflowEdges: vi.fn(async (..._args: unknown[]) => ({
    nodes: [] as Array<{ id: string; state: string }>,
    edges: [] as Array<[string, string]>,
  })),
  replanWorkflow: vi.fn(async (..._args: unknown[]) => ({
    nodes: 2,
    edges: 0,
    depth: 1,
    width: 2,
    cycles: [],
  })),
  ensureNodePrOnIntegrationBranch: vi.fn(
    async (..._args: unknown[]): Promise<{ ok: boolean; reason?: string; retargeted?: boolean }> => ({
      ok: true,
      retargeted: false,
    })
  ),
  retargetReleasedDependents: vi.fn(async (..._args: unknown[]) => [] as string[]),
  mergePr: vi.fn(async (..._args: unknown[]) => ({ merged: true })),
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
  loadWorkflowEdges: h.loadWorkflowEdges,
  replanWorkflow: h.replanWorkflow,
  workflowIntegrationBranch: (id: string) => `exp/wf-${id.slice(0, 8)}`,
}))
vi.mock(`@/lib/workflow-final-pr`, () => ({
  ensureNodePrOnIntegrationBranch: h.ensureNodePrOnIntegrationBranch,
  retargetReleasedDependents: h.retargetReleasedDependents,
}))
vi.mock(`@/lib/trpc/issues`, () => ({
  issuesRouter: { createCaller: () => ({ mergePr: h.mergePr }) },
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
// What the next UPDATE ... RETURNING resolves with (FIFO; default = one row).
const updateQueue: unknown[][] = []
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
    const p = chain(updateQueue.shift() ?? [{ id: `wf-1` }])
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
  mergeLaunch,
  mergeBelongsToAttempt,
  mergedNodeOutcome,
  reviewOutcome,
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
  updateQueue.length = 0
  written.length = 0
  vi.clearAllMocks()
  h.assertTeamMember.mockResolvedValue({ role: `member` })
  h.ensureNodePrOnIntegrationBranch.mockResolvedValue({ ok: true, retargeted: false })
  h.retargetReleasedDependents.mockResolvedValue([])
  h.loadWorkflowEdges.mockResolvedValue({ nodes: [], edges: [] })
})

describe(`workflows.create`, () => {
  it(`names the draft after its lowest issue NUMBER and lays it out`, async () => {
    selectQueue.push([issue(B), issue(A)])
    const result = await caller.create({ teamId: TEAM, issueIds: [B, A] })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, TEAM)
    expect(written[0]!.values).toMatchObject({ name: `APP-6 +1`, repositoryId: `repo-1` })
    // EXP-1002: the draft opens on the shipped split, every field explicit.
    expect(written[0]!.values).toMatchObject({
      launch: {
        agent: `claude`,
        model: `opus`,
        contractModel: `fable`,
        integrationModel: `fable`,
        riskModel: `fable`,
        subagentModel: `opus`,
      },
    })
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
    const error = await rejection(caller.update({ id: WF, startOn: `landed` }))
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

  // compat: iOS 0.14.39, Android 0.14.40 and desktop 0.14.47 still send the
  // removed `gate` (EXP-1010); strip mode leaves nothing to set.
  it(`answers a gate-only patch with the row as it is, writing nothing`, async () => {
    for (const status of [`draft`, `running`]) {
      const row = workflow({ status })
      // loadWorkflow, then the guard's own read.
      selectQueue.push([row], [row])
      const result = await caller.update({ id: WF, gate: `human` } as never)
      expect(result.workflow).toEqual(row)
      expect(result.txId).toBeDefined()
    }
    expect(fakeDb.update).not.toHaveBeenCalled()
    expect(written).toEqual([])
  })

  // compat: older clients send a launch WITHOUT the EXP-1002 phase pins.
  describe(`the phase pins' cross-client contract`, () => {
    const stored = {
      agent: `claude`,
      model: `opus`,
      contractModel: `fable`,
      integrationModel: `fable`,
      riskModel: `fable`,
    }

    it(`keeps a stored pin whose key is ABSENT`, async () => {
      selectQueue.push([workflow({ launch: stored })])
      await caller.update({ id: WF, launch: { agent: `claude`, model: `sonnet` } })
      expect(written[0]!.values).toEqual({
        launch: {
          agent: `claude`,
          model: `sonnet`,
          contractModel: `fable`,
          integrationModel: `fable`,
          riskModel: `fable`,
        },
      })
    })

    it(`clears on null and sets on a string, storing no null pins`, async () => {
      selectQueue.push([workflow({ launch: stored })])
      await caller.update({
        id: WF,
        launch: {
          agent: `claude`,
          model: `opus`,
          contractModel: null,
          integrationModel: `sonnet`,
          riskModel: null,
        },
      })
      expect(written[0]!.values).toEqual({
        launch: { agent: `claude`, model: `opus`, integrationModel: `sonnet` },
      })
    })

    it(`carries no pin across an agent switch: the new agent's shipped ones stand in`, () => {
      expect(mergeLaunch(stored, { agent: `codex`, model: `gpt-5.6-sol` })).toEqual({
        agent: `codex`,
        model: `gpt-5.6-sol`,
        contractModel: `gpt-5.6-luna`,
        integrationModel: `gpt-5.6-luna`,
        riskModel: `gpt-5.6-luna`,
      })
    })
  })

  describe(`binding a device that cannot run the stored agent`, () => {
    const claudeLaunch = { agent: `claude`, model: `opus`, contractModel: `fable` }
    const codexOnly = async (...args: unknown[]) => {
      if (args[3] === `claude`) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `claude is not available on that device` })
      }
    }

    it(`re-seeds a DRAFT's launch from the first agent the device runs`, async () => {
      h.assertDeviceUsable.mockImplementation(codexOnly)
      selectQueue.push([workflow({ launch: claudeLaunch })])
      await caller.update({ id: WF, deviceId: `dev-codex` })
      expect(written[0]!.values).toEqual({
        deviceId: `dev-codex`,
        launch: expect.objectContaining({ agent: `codex`, model: `gpt-5.6-sol` }),
      })
      h.assertDeviceUsable.mockReset()
    })

    it(`leaves the launch alone when the device runs the stored agent`, async () => {
      selectQueue.push([workflow({ launch: claudeLaunch })])
      await caller.update({ id: WF, deviceId: `dev-1` })
      expect(written[0]!.values).toEqual({ deviceId: `dev-1` })
    })

    it(`still refuses a device that runs no agent at all`, async () => {
      h.assertDeviceUsable.mockImplementation(async (...args: unknown[]) => {
        if (args[3]) throw new TRPCError({ code: `BAD_REQUEST`, message: `${String(args[3])} is not available on that device` })
      })
      selectQueue.push([workflow({ launch: claudeLaunch })])
      const error = await rejection(caller.update({ id: WF, deviceId: `dev-none` }))
      expect(error?.message).toBe(`claude is not available on that device`)
      expect(fakeDb.transaction).not.toHaveBeenCalled()
      h.assertDeviceUsable.mockReset()
    })

    it(`never re-seeds when the caller sent a launch of their own`, async () => {
      h.assertDeviceUsable.mockImplementation(codexOnly)
      selectQueue.push([workflow({ launch: claudeLaunch })])
      const error = await rejection(
        caller.update({ id: WF, deviceId: `dev-codex`, launch: { agent: `claude` } })
      )
      expect(error?.message).toBe(`claude is not available on that device`)
      h.assertDeviceUsable.mockReset()
    })
  })
})

describe(`workflows.updateNode`, () => {
  // compat: iOS ≤0.14.38, Android ≤0.14.39, desktop/CLI ≤0.14.46.
  it(`accepts a BUDGET-ONLY patch and writes nothing`, async () => {
    selectQueue.push([workflow()], [{ id: `node-1`, issueId: A, members: [] }])
    const result = await caller.updateNode({
      workflowId: WF,
      issueId: A,
      budget: { maxTurns: 40 },
    } as never)
    expect(result).toEqual({ txId: expect.anything(), nodeId: `node-1` })
    expect(fakeDb.update).not.toHaveBeenCalled()
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, TEAM)
  })

  it(`still refuses a budget-only patch for an issue outside the workflow`, async () => {
    selectQueue.push([workflow()], [{ id: `node-1`, issueId: A, members: [] }])
    const error = await rejection(
      caller.updateNode({ workflowId: WF, issueId: B, budget: 1 } as never)
    )
    expect(error?.message).toBe(`That issue is not part of the workflow`)
  })

  it(`writes the fields it still knows, budget dropped`, async () => {
    selectQueue.push([workflow()], [{ id: `node-1`, issueId: A, members: [] }])
    await caller.updateNode({ workflowId: WF, issueId: A, risk: `high`, budget: 3 } as never)
    expect(written[0]!.values).toEqual({ risk: `high` })
  })
})

describe(`workflows.setIssues`, () => {
  it(`refuses to remove a MEMBER issue (folded into its parent's node)`, async () => {
    selectQueue.push(
      [workflow()],
      [{ issueId: A, members: [B] }]
    )
    const error = await rejection(caller.setIssues({ id: WF, removeIssueIds: [B] }))
    expect(error?.code).toBe(`BAD_REQUEST`)
    expect(error?.message).toContain(`remove the parent relation`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`counts covered ISSUES, members included, against the cap`, async () => {
    const members = Array.from({ length: 49 }, (_, i) => `member-${i}`)
    selectQueue.push([workflow()], [{ issueId: A, members }], [issue(B)])
    const error = await rejection(caller.setIssues({ id: WF, addIssueIds: [B] }))
    expect(error?.message).toContain(`at most 50 issues`)
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

  it(`refuses a workflow with no runner or a cycle`, async () => {
    selectQueue.push([workflow({ ...ready, deviceId: null })])
    expect((await rejection(caller.start({ id: WF })))?.message).toContain(`Pick the device`)

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

  it(`says whether the guarded write matched a row`, async () => {
    const live = [
      [node()],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }],
    ]
    selectQueue.push(...live)
    updateQueue.push([{ id: `node-1` }])
    expect(await caller.reportNode({ nodeId: NODE, state: `running` })).toEqual({ updated: true })

    // `landNode` landed it between the read and the write: no row matched.
    selectQueue.push(...live)
    updateQueue.push([])
    expect(await caller.reportNode({ nodeId: NODE, state: `running` })).toEqual({ updated: false })
  })

  // compat: desktop/CLI ≤0.14.46 engines.
  it(`writes a legacy \`paused\` report as a held \`failed\``, async () => {
    selectQueue.push(
      [node({ state: `running` })],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }]
    )
    await caller.reportNode({ nodeId: NODE, state: `paused` as never, note: `Budget` })
    expect(written[0]!.values).toMatchObject({ state: `failed`, note: `Budget` })
    // Past the engine's one free restart.
    expect((written[0]!.values as { attempt?: unknown }).attempt).toBeDefined()
  })

  it(`a person's approval drops the reviewer's head so the engine never reads it as stale`, async () => {
    selectQueue.push([node({ approvedAt: null })], [workflow()], [{ since: new Date() }])
    await caller.approveNode({ nodeId: NODE, approved: true } as never)
    const write = written.find((w) => w.op === `update`)
    expect(write?.values).toMatchObject({ approvedAt: expect.any(Date) })
    expect(typeof (write?.values as { review: unknown }).review).toBe(`object`)
    expect((write?.values as { review: unknown }).review).not.toBeNull()
  })

  it(`withdrawing an approval leaves the review alone`, async () => {
    selectQueue.push([node()], [workflow()])
    await caller.approveNode({ nodeId: NODE, approved: false } as never)
    const write = written.find((w) => w.op === `update`)
    expect(write?.values).toEqual({ approvedAt: null })
  })

  it(`holds an unapproved node at the gate, server-side`, async () => {
    selectQueue.push(
      [node()],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: false,
      reason: `Waiting for a person to approve`,
      retargeted: [],
    })
  })

  it(`requires the engine's owner to be a MEMBER of the workflow's team`, async () => {
    h.assertTeamMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN`, message: `Not a member` })
    )
    selectQueue.push([node()], [workflow({ status: `running`, deviceId: `dev-1` })])
    const error = await rejection(caller.reportNode({ nodeId: NODE, state: `running` }))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, TEAM)
    expect(fakeDb.update).not.toHaveBeenCalled()
  })

  // Zod 4's `record` over an enum key is exhaustive; the daemon sends one or
  // two counters per call.
  it(`reportMetrics accepts a partial set of counters`, async () => {
    selectQueue.push([workflow({ status: `running`, deviceId: `dev-1` })], [{ id: `device-row` }])
    expect(await caller.reportMetrics({ id: WF, deltas: { mergeIns: 1 } })).toEqual({ ok: true })
    expect(written).toHaveLength(1)
    expect(written[0]!.op).toBe(`update`)
  })

  it(`reportMetrics still refuses an unknown counter`, async () => {
    selectQueue.push([workflow({ status: `running`, deviceId: `dev-1` })], [{ id: `device-row` }])
    const error = await rejection(
      caller.reportMetrics({ id: WF, deltas: { nodes: 1 } as never })
    )
    expect(error?.code).toBe(`BAD_REQUEST`)
  })

  // EXP-983: a speculative dependent's PR can be up before its blocker landed.
  it(`lands in topological order: never before a blocker`, async () => {
    h.loadWorkflowEdges.mockResolvedValueOnce({
      nodes: [
        { id: `node-0`, state: `in_review` },
        { id: `node-1`, state: `in_review` },
      ],
      edges: [[`node-0`, `node-1`]],
    } as never)
    selectQueue.push(
      [node({ approvedAt: new Date() })],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: false,
      reason: `Waiting for its blockers to land`,
      retargeted: [],
    })
  })

  // The stack heal or a person can move a node's PR off the integration
  // branch; merging it there would bypass the workflow's final PR.
  it(`never merges a node whose PR is not based on the integration branch`, async () => {
    h.ensureNodePrOnIntegrationBranch.mockResolvedValueOnce({
      ok: false,
      reason: `Its pull request is not based on the workflow branch yet`,
    })
    selectQueue.push(
      [node({ approvedAt: new Date() })],
      [workflow({ status: `running`, deviceId: `dev-1`, integrationBranch: `exp/wf-22222222` })],
      [{ id: `device-row` }],
      [{ prState: `open` }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: false,
      reason: `Its pull request is not based on the workflow branch yet`,
      retargeted: [],
    })
    expect(h.ensureNodePrOnIntegrationBranch).toHaveBeenCalledWith(
      fakeDb,
      expect.objectContaining({ issueId: A, integrationBranch: `exp/wf-22222222` })
    )
    expect(h.mergePr).not.toHaveBeenCalled()
    expect(written).toEqual([])
  })

  it(`merges once the base is right, lands the node and releases its dependents`, async () => {
    h.retargetReleasedDependents.mockResolvedValueOnce([`node-9`])
    selectQueue.push(
      [node({ approvedAt: new Date() })],
      [workflow({ status: `running`, deviceId: `dev-1`, integrationBranch: `exp/wf-22222222` })],
      [{ id: `device-row` }],
      [{ prState: `open` }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: true,
      reason: null,
      retargeted: [`node-9`],
    })
    expect(h.mergePr).toHaveBeenCalledWith({ issueId: A, endSessions: true })
    expect(written[0]!.values).toEqual({ state: `landed`, note: null })
  })

  // EXP-1007: a person merged the node's PR from Reviews or GitHub while the
  // node was `running`/`updating` — no approval, and the web offers Approve
  // only on `in_review`. The code is in the integration branch: the train's
  // step is done, whatever the gate or the landing order say.
  it(`lands a node whose PR merged outside the train, unapproved and past the order`, async () => {
    h.loadWorkflowEdges.mockResolvedValueOnce({
      nodes: [
        { id: `node-0`, state: `in_review` },
        { id: `node-1`, state: `updating` },
      ],
      edges: [[`node-0`, `node-1`]],
    } as never)
    selectQueue.push(
      [node({ state: `updating`, approvedAt: null })],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }],
      [{ prState: `merged` }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: true,
      reason: null,
      retargeted: [],
    })
    expect(h.mergePr).not.toHaveBeenCalled()
    expect(h.ensureNodePrOnIntegrationBranch).not.toHaveBeenCalled()
    expect(written[0]!.values).toEqual({ state: `landed`, note: null })
    expect(h.retargetReleasedDependents).toHaveBeenCalledWith(fakeDb, WF, `node-1`, `user-1`)
  })

  // EXP-1010: merged into a BLOCKER's branch (a speculative start a person
  // merged from Reviews): the code is not in the integration branch yet.
  const stackedOn = (state: string) =>
    ({
      nodes: [
        { id: `node-0`, issueId: B, state },
        { id: `node-1`, issueId: A, state: `in_review` },
      ],
      edges: [[`node-0`, `node-1`]],
    }) as never
  const mergedIntoBlocker = () =>
    selectQueue.push(
      [node({ mergedInto: `exp/APP-10` })],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }],
      [{ prState: `merged`, prMergedAt: null }],
      [{ id: B, branch: `exp/APP-10` }]
    )

  it(`keeps the wait for a node that merged into its unlanded blocker's branch`, async () => {
    h.loadWorkflowEdges.mockResolvedValueOnce(stackedOn(`in_review`))
    mergedIntoBlocker()
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: false,
      reason: `Waiting for its blockers to land`,
      retargeted: [],
    })
    expect(written).toEqual([])
  })

  it(`fails that node once the blocker is skipped: the final PR would lack its code`, async () => {
    h.loadWorkflowEdges.mockResolvedValueOnce(stackedOn(`skipped`))
    mergedIntoBlocker()
    expect((await caller.landNode({ nodeId: NODE })).merged).toBe(false)
    expect(written[0]!.values).toMatchObject({
      state: `failed`,
      note: expect.stringContaining(`which was skipped`),
    })
    expect(h.retargetReleasedDependents).not.toHaveBeenCalled()
  })

  it(`a merge from before the workflow started lands nothing`, async () => {
    selectQueue.push(
      [node()],
      [workflow({ status: `running`, deviceId: `dev-1`, startedAt: new Date(`2026-09-21T10:00:00Z`) })],
      [{ id: `device-row` }],
      [{ prState: `merged`, prMergedAt: new Date(`2026-09-01T10:00:00Z`) }]
    )
    expect((await caller.landNode({ nodeId: NODE })).reason).toBe(`Waiting for a person to approve`)
    expect(written).toEqual([])
  })

  it(`a concurrent skip wins: the landed write claims nothing and counts nothing`, async () => {
    selectQueue.push(
      [node()],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }],
      [{ prState: `merged`, prMergedAt: null }]
    )
    updateQueue.push([])
    expect((await caller.landNode({ nodeId: NODE })).merged).toBe(true)
    expect(written).toHaveLength(1)
    expect(h.retargetReleasedDependents).not.toHaveBeenCalled()
  })

  it(`a paused workflow lands nothing, merged or not`, async () => {
    selectQueue.push(
      [node({ state: `updating` })],
      [workflow({ status: `paused`, deviceId: `dev-1` })],
      [{ id: `device-row` }],
      [{ prState: `merged` }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: false,
      reason: `The workflow is not running`,
      retargeted: [],
    })
    expect(written).toEqual([])
  })

  it(`a retry forgets the old approval and review with the old attempt`, async () => {
    selectQueue.push([node({ state: `failed` })], [workflow({ status: `running` })])
    await caller.resolveNode({ nodeId: NODE, action: `retry` })
    expect(written[0]!.values).toEqual({
      state: `blocked`,
      attempt: 0,
      sessionId: null,
      note: null,
      approvedAt: null,
      review: null,
      reviewRound: 0,
      mergedInto: null,
      retriedAt: expect.any(Date),
    })
    expect(h.retargetReleasedDependents).not.toHaveBeenCalled()
  })

  it(`a skip releases the dependents like a landing does`, async () => {
    selectQueue.push([node({ state: `failed` })], [workflow({ status: `running` })])
    await caller.resolveNode({ nodeId: NODE, action: `skip` })
    expect(written[0]!.values).toEqual({ state: `skipped`, note: null })
    expect(h.retargetReleasedDependents).toHaveBeenCalledWith(fakeDb, WF, `node-1`, `user-1`)
  })
})

// EXP-984: the agent review gate is bound to the reviewer RUN.
describe(`workflows.submitReview`, () => {
  const NODE = `55555555-5555-4555-8555-555555555555`
  const AUTHOR_RUN = `66666666-6666-4666-8666-666666666666`
  const REVIEW_RUN = `77777777-7777-4777-8777-777777777777`
  const node = (over: Record<string, unknown> = {}) => ({
    id: `node-1`,
    workflowId: WF,
    issueId: A,
    kind: `leaf`,
    state: `in_review`,
    approvedAt: null,
    sessionId: AUTHOR_RUN,
    ...over,
  })
  const reviewer = (over: Record<string, unknown> = {}) => ({
    id: REVIEW_RUN,
    actionName: `Review node`,
    startedReason: `workflow`,
    ...over,
  })
  const running = () => workflow({ status: `running`, deviceId: `dev-1` })
  const submit = (sessionId: string, over: Record<string, unknown> = {}) =>
    caller.submitReview({
      nodeId: NODE,
      sessionId,
      verdict: `approve`,
      oracle: { command: `bun test`, passed: true },
      ...over,
    } as never)

  it(`refuses the node's own (author) run`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [reviewer({ id: AUTHOR_RUN })])
    const error = await rejection(submit(AUTHOR_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(error?.message).toContain(`cannot review itself`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it.each([
    [`a run that is not the review builtin`, reviewer({ actionName: `Fix merge conflicts` })],
    [`a review run a person started`, reviewer({ startedReason: null })],
    [`an unknown run`, null],
  ])(`refuses %s`, async (_name, row) => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], row ? [row] : [])
    const error = await rejection(submit(REVIEW_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  // FEED-51: an account switch resumes the reviewer through the MCP path,
  // which re-brands the successor `agent` under the switching chat. The
  // successor is still the workflow's reviewer: the guard walks back.
  const RESUMED_RUN = `88888888-8888-4888-8888-888888888888`
  const resumed = (over: Record<string, unknown> = {}) =>
    reviewer({ id: RESUMED_RUN, startedReason: `agent`, resumedFromId: REVIEW_RUN, ...over })

  it(`accepts a reviewer resumed after an account switch`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [resumed()], [reviewer()])
    updateQueue.push([{ round: 2 }])
    const result = await submit(RESUMED_RUN, { head: `e39378c61d3` })
    expect(result).toMatchObject({ round: 2, approve: true, state: `in_review` })
    expect(written[1]!.values).toMatchObject({
      approvedAt: expect.any(Date),
      review: expect.objectContaining({ head: `e39378c61d3`, round: 2 }),
    })
  })

  it(`follows a chain of resumes to the run the workflow started`, async () => {
    const MIDDLE = `99999999-9999-4999-8999-999999999999`
    selectQueue.push(
      [node()],
      [running()],
      [{ id: `device-row` }],
      [resumed({ resumedFromId: MIDDLE })],
      [resumed({ id: MIDDLE })],
      [reviewer()]
    )
    updateQueue.push([{ round: 1 }])
    await expect(submit(RESUMED_RUN)).resolves.toMatchObject({ round: 1 })
  })

  it.each([
    [`a resume chain that never reaches a workflow-started review run`, [resumed()], [reviewer({ startedReason: null })]],
    [`a resume of the node's own run`, [resumed({ resumedFromId: AUTHOR_RUN })], [reviewer({ id: AUTHOR_RUN })]],
    [`a resume whose predecessor is not the review builtin`, [resumed()], [reviewer({ actionName: `Chat` })]],
    [`a resume whose predecessor vanished`, [resumed()], []],
  ])(`refuses %s`, async (_name, ...rows) => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], ...rows)
    const error = await rejection(submit(RESUMED_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`stops on a resume loop instead of spinning`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [resumed({ resumedFromId: RESUMED_RUN })])
    const error = await rejection(submit(RESUMED_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.select).toHaveBeenCalledTimes(4)
  })

  it(`claims the round in SQL, stores the reviewed head and approves on evidence`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [reviewer()])
    updateQueue.push([{ round: 1 }])
    const result = await submit(REVIEW_RUN, { head: `abc1234def` })
    expect(result).toMatchObject({ round: 1, approve: true, state: `in_review` })
    // 1st update = the round claim (a SQL increment, not a literal).
    expect(written[0]!.op).toBe(`update`)
    const claim = (written[0]!.values as { reviewRound: unknown }).reviewRound
    expect(typeof claim).toBe(`object`)
    expect(claim).not.toBe(1)
    expect(written[1]!.values).toMatchObject({
      state: `in_review`,
      approvedAt: expect.any(Date),
      review: expect.objectContaining({
        verdict: `approve`,
        round: 1,
        head: `abc1234def`,
        oracle: { command: `bun test`, passed: true },
      }),
    })
  })

  it(`rejects a malformed head`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [reviewer()])
    const error = await rejection(submit(REVIEW_RUN, { head: `not-a-sha` }))
    expect(error?.code).toBe(`BAD_REQUEST`)
  })

  it(`leaves a paused or waiting node alone`, async () => {
    selectQueue.push([node({ state: `paused` })], [running()], [{ id: `device-row` }], [reviewer()])
    updateQueue.push([])
    const error = await rejection(submit(REVIEW_RUN))
    expect(error?.message).toContain(`not under review`)
    expect(written).toHaveLength(1)
  })

  it(`refuses an approval past the round cap`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [reviewer()])
    updateQueue.push([{ round: 4 }])
    const error = await rejection(submit(REVIEW_RUN))
    expect(error?.message).toContain(`used up`)
    expect(written).toHaveLength(1)
  })
})

// EXP-1010 — a PR merged outside the train: whose attempt, and into what.
describe(`mergeBelongsToAttempt`, () => {
  const startedAt = `2026-09-21T10:00:00Z`
  it(`ignores a merge from before the workflow started or the node was retried`, () => {
    expect(mergeBelongsToAttempt({ mergedAt: `2026-09-20T10:00:00Z`, startedAt, retriedAt: null })).toBe(false)
    expect(
      mergeBelongsToAttempt({
        mergedAt: `2026-09-21T11:00:00Z`,
        startedAt,
        retriedAt: `2026-09-21T12:00:00Z`,
      })
    ).toBe(false)
  })
  it(`counts this attempt's merge, and one nobody dated`, () => {
    expect(mergeBelongsToAttempt({ mergedAt: `2026-09-21T11:00:00Z`, startedAt, retriedAt: null })).toBe(true)
    expect(mergeBelongsToAttempt({ mergedAt: null, startedAt, retriedAt: null })).toBe(true)
  })
})

describe(`mergedNodeOutcome`, () => {
  const integrationBranch = `exp/wf-22222222`
  const on = (mergedInto: string | null, state?: string) =>
    mergedNodeOutcome({
      mergedInto,
      integrationBranch,
      carriers: new Map(state ? [[`exp/APP-6`, state]] : []),
    })
  it(`lands a merge into the integration branch, an unknown base or a foreign branch`, () => {
    expect(on(null).step).toBe(`land`)
    expect(on(integrationBranch).step).toBe(`land`)
    expect(on(`master`, `in_review`).step).toBe(`land`)
  })
  it(`waits for the blocker whose branch took the merge, and lands with it`, () => {
    expect(on(`exp/APP-6`, `in_review`).step).toBe(`wait`)
    expect(on(`exp/APP-6`, `failed`).step).toBe(`wait`)
    expect(on(`exp/APP-6`, `landed`).step).toBe(`land`)
  })
  it(`fails once that blocker is skipped, or the base was a throwaway`, () => {
    expect(on(`exp/APP-6`, `skipped`)).toMatchObject({ step: `fail` })
    expect(on(`${integrationBranch}-base-APP-10`)).toMatchObject({ step: `fail` })
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

// EXP-984 — what a submitted review does to its node.
describe(`reviewOutcome`, () => {
  it(`an approval clears the node, the contract included`, () => {
    expect(reviewOutcome({ verdict: `approve`, oraclePassed: true, round: 1 })).toMatchObject({
      approve: true,
      state: `in_review`,
    })
    expect(reviewOutcome({ verdict: `approve`, oraclePassed: null, round: 1 }).approve).toBe(true)
  })

  it(`keeps an approval its own checks contradict advisory`, () => {
    const outcome = reviewOutcome({ verdict: `approve`, oraclePassed: false, round: 1 })
    expect(outcome.approve).toBe(false)
    expect(outcome.note).toContain(`needs a person`)
  })

  it(`bounces to the author up to the round cap, then waits for a person`, () => {
    expect(
      reviewOutcome({ verdict: `request_changes`, oraclePassed: false, round: 2 }).state
    ).toBe(`updating`)
    expect(
      reviewOutcome({ verdict: `request_changes`, oraclePassed: null, round: 3 })
    ).toMatchObject({ state: `waiting`, note: `Review did not converge after 3 rounds` })
  })
})
