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
  // EXP-1032 — the completion path.
  openWorkflowFinalPr: vi.fn(async (..._args: unknown[]) => ({ url: `https://gh/pr/9` })),
  applyWorkflowFinalPrState: vi.fn(async (..._args: unknown[]) => true),
  loadRepository: vi.fn(async (..._args: unknown[]) => ({
    id: `repo-1`,
    // = TEAM below: `mergeFinalPr` refuses another team's repository.
    teamId: `11111111-1111-4111-8111-111111111111`,
    fullName: `o/r`,
  })),
  mergeRepositoryPull: vi.fn(async (..._args: unknown[]) => ({ merged: true as const })),
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
vi.mock(`@/lib/workflows`, async () => ({
  nodeEdges: () => [],
  loadWorkflowEdges: h.loadWorkflowEdges,
  replanWorkflow: h.replanWorkflow,
  workflowIntegrationBranch: (id: string) => `exp/wf-${id.slice(0, 8)}`,
  // The real matcher: the review gate's branch evidence (FEED-51).
  isWorkflowReviewBranch: (await vi.importActual<typeof import("@/lib/workflows")>(`@/lib/workflows`))
    .isWorkflowReviewBranch,
}))
vi.mock(`@/lib/workflow-final-pr`, () => ({
  ensureNodePrOnIntegrationBranch: h.ensureNodePrOnIntegrationBranch,
  retargetReleasedDependents: h.retargetReleasedDependents,
  openWorkflowFinalPr: h.openWorkflowFinalPr,
  applyWorkflowFinalPrState: h.applyWorkflowFinalPrState,
}))
vi.mock(`@/lib/trpc/repositories`, () => ({
  loadRepository: h.loadRepository,
  mergeRepositoryPull: h.mergeRepositoryPull,
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
vi.mock(`@/lib/steer-child-messages`, () => ({
  oneLine: (text: string) => text,
  MAX_SESSION_CHAIN_DEPTH: 20,
}))

import {
  appendDecisionLine,
  launchFromDeviceDefaults,
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
// compat: what a launch is STORED as — the normalized keys plus the legacy
// per-phase pins a desktop/CLI 0.14.49/0.14.50 engine still reads (each
// `strongModel`, `subagentModel` = `model`). Collapses back to `launch`
// once CLIENT_MIN_VERSION_DESKTOP/CLI >= 0.14.51.
const stored = (launch: { model: string; strongModel: string } & Record<string, unknown>) => ({
  ...launch,
  contractModel: launch.strongModel,
  integrationModel: launch.strongModel,
  riskModel: launch.strongModel,
  reviewModel: launch.strongModel,
  subagentModel: launch.model,
})
const claudeDefaults = { agent: `claude`, model: `opus`, strongModel: `fable` }
const codexDefaults = { agent: `codex`, model: `gpt-5.6-sol`, strongModel: `gpt-5.6-luna` }

beforeEach(() => {
  selectQueue.length = 0
  updateQueue.length = 0
  written.length = 0
  vi.clearAllMocks()
  h.assertTeamMember.mockResolvedValue({ role: `member` })
  h.ensureNodePrOnIntegrationBranch.mockResolvedValue({ ok: true, retargeted: false })
  h.retargetReleasedDependents.mockResolvedValue([])
  h.loadWorkflowEdges.mockResolvedValue({ nodes: [], edges: [] })
  h.openWorkflowFinalPr.mockResolvedValue({ url: `https://gh/pr/9` })
  h.applyWorkflowFinalPrState.mockResolvedValue(true)
  h.mergeRepositoryPull.mockResolvedValue({ merged: true as const })
})

describe(`workflows.create`, () => {
  it(`names the draft after its lowest issue NUMBER and lays it out`, async () => {
    selectQueue.push([issue(B), issue(A)])
    const result = await caller.create({ teamId: TEAM, issueIds: [B, A] })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, TEAM)
    expect(written[0]!.values).toMatchObject({ name: `APP-6 +1`, repositoryId: `repo-1` })
    // EXP-1029: two models, and `startOn` is no longer a choice. A workflow
    // is BORN with no runner, so the contract defaults stand — binding a
    // device is what re-seeds them. compat: the legacy pins ride beside the
    // two models for the 0.14.49/0.14.50 engines (`stored`).
    expect(written[0]!.values).toMatchObject({
      launch: stored(claudeDefaults),
      startOn: `contract`,
    })
    expect(
      Object.keys((written[0]!.values as { launch: object }).launch).sort()
    ).toEqual([
      `agent`,
      `contractModel`,
      `integrationModel`,
      `model`,
      `reviewModel`,
      `riskModel`,
      `strongModel`,
      `subagentModel`,
    ])
    expect((written[0]!.values as { deviceId?: unknown }).deviceId).toBeUndefined()
    expect(h.replanWorkflow).toHaveBeenCalledTimes(1)
    expect(result.workflow.metrics).toMatchObject({ nodes: 2 })
  })

  // EXP-1032: the IDE binds its own machine at creation (`deviceId`); the
  // launch is seeded from it exactly as `update({deviceId})` seeds it.
  it(`binds the runner sent with the draft and seeds the launch from that machine`, async () => {
    // The picked issues, then `launchForDevice`'s read of the devices row.
    selectQueue.push(
      [issue(A)],
      [
        {
          launchDefaults: {
            defaultAgent: `codex`,
            defaultAccount: `p-7`,
            workflow: { model: `gpt-5.6-luna`, strongModel: `gpt-5.6-luna` },
          },
        },
      ]
    )
    await caller.create({ teamId: TEAM, issueIds: [A], deviceId: `dev-codex` })
    expect(written[0]!.values).toMatchObject({
      deviceId: `dev-codex`,
      launch: stored({
        agent: `codex`,
        account: `p-7`,
        model: `gpt-5.6-luna`,
        strongModel: `gpt-5.6-luna`,
      }),
    })
    // Ownership + cap, the seeded agent, then the final check on the agent
    // the workflow will run on.
    expect(h.assertDeviceUsable).toHaveBeenCalledWith(
      `dev-codex`,
      TEAM,
      `user-1`,
      null,
      expect.objectContaining({ cap: `workflows` })
    )
    expect(h.assertDeviceUsable).toHaveBeenLastCalledWith(
      `dev-codex`,
      TEAM,
      `user-1`,
      `codex`,
      expect.objectContaining({ cap: `workflows` })
    )
  })

  it(`refuses a runner the caller cannot use, before anything is written`, async () => {
    h.assertDeviceUsable.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN`, message: `Not your device` })
    )
    selectQueue.push([issue(A)])
    const error = await rejection(
      caller.create({ teamId: TEAM, issueIds: [A], deviceId: `dev-stranger` })
    )
    expect(error?.message).toBe(`Not your device`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
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
    const error = await rejection(caller.update({ id: WF, deviceId: `dev-1` }))
    expect(error?.code).toBe(`PRECONDITION_FAILED`)
  })

  // EXP-1029: `startOn` is fixed to `contract`; an old client still sends it.
  it(`ignores startOn at any status, writing nothing`, async () => {
    for (const status of [`draft`, `running`]) {
      const row = workflow({ status })
      selectQueue.push([row], [row])
      const result = await caller.update({ id: WF, startOn: `landed` })
      expect(result.workflow).toEqual(row)
    }
    expect(fakeDb.update).not.toHaveBeenCalled()
    expect(written).toEqual([])
  })

  it(`validates the NORMALIZED launch against the agent's vocabulary`, async () => {
    selectQueue.push([workflow()])
    const model = await rejection(caller.update({ id: WF, launch: { model: `gpt-nope` } }))
    expect(model?.message).toBe(`Unknown claude model`)

    selectQueue.push([workflow()])
    const strong = await rejection(
      caller.update({ id: WF, launch: { agent: `codex`, strongModel: `opus` } })
    )
    expect(strong?.message).toBe(`Unknown codex model`)

  })

  // compat: iOS 0.14.42, Android 0.14.43 and desktop 0.14.50 offer the CLAUDE
  // list for the review model whatever the agent and re-send the whole launch
  // on every save; the base server never validated `reviewModel`. A folded
  // pin outside the agent's vocabulary heals to the agent's default strong
  // model; an explicit `strongModel` is still held to the vocabulary.
  it(`heals a deprecated pin outside the vocabulary instead of refusing the save`, async () => {
    selectQueue.push([workflow()])
    await caller.update({ id: WF, launch: { riskModel: `nope` } })
    expect(written[0]!.values).toEqual({ launch: stored(claudeDefaults) })

    written.length = 0
    selectQueue.push([workflow()])
    await caller.update({
      id: WF,
      launch: { agent: `codex`, model: `gpt-5.6-sol`, reviewModel: `opus` },
    })
    expect(written[0]!.values).toEqual({ launch: stored(codexDefaults) })

    // A pin INSIDE the vocabulary still folds in as before.
    written.length = 0
    selectQueue.push([workflow()])
    await caller.update({ id: WF, launch: { agent: `codex`, reviewModel: `gpt-5.6-terra` } })
    expect(written[0]!.values).toEqual({
      launch: stored({ ...codexDefaults, strongModel: `gpt-5.6-terra` }),
    })

    // The heal never covers an explicit strongModel.
    selectQueue.push([workflow()])
    const explicit = await rejection(
      caller.update({ id: WF, launch: { agent: `codex`, strongModel: `opus`, reviewModel: `opus` } })
    )
    expect(explicit?.message).toBe(`Unknown codex model`)
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

  // EXP-1029: a launch is stored as the NORMALIZED one, whatever vintage the
  // client that sent it is: the deprecated pins fold into `strongModel`, and
  // (compat) are written back OUT of it for the 0.14.49/0.14.50 engines, so a
  // pin never survives on its own.
  describe(`the stored launch is the NORMALIZED one`, () => {
    it(`folds an old client's phase pins into strongModel and re-derives the rest from it`, async () => {
      selectQueue.push([workflow()])
      await caller.update({
        id: WF,
        launch: {
          agent: `claude`,
          model: `sonnet`,
          // `riskModel` outranks the phase pins in the fold.
          riskModel: `fable`,
          contractModel: `opus`,
          integrationModel: `opus`,
          subagentModel: `fable`,
          effort: `high`,
          maxParallel: 5,
        },
      })
      expect(written[0]!.values).toEqual({
        launch: stored({ agent: `claude`, model: `sonnet`, strongModel: `fable` }),
      })
    })

    it(`never carries a stored pin forward: the launch is replaced WHOLE`, async () => {
      selectQueue.push([
        workflow({ launch: { agent: `claude`, model: `opus`, riskModel: `sonnet` } }),
      ])
      await caller.update({ id: WF, launch: { agent: `claude`, model: `sonnet` } })
      expect(written[0]!.values).toEqual({
        launch: stored({ agent: `claude`, model: `sonnet`, strongModel: `fable` }),
      })
    })
  })

  // EXP-1032: binding the runner IS the launch choice — the workflow screen
  // has no settings panel left.
  describe(`binding a runner device re-seeds the launch`, () => {
    const claudeLaunch = { agent: `claude`, model: `opus`, contractModel: `fable` }
    const codexOnly = async (...args: unknown[]) => {
      if (args[3] === `claude`) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `claude is not available on that device` })
      }
    }
    // loadWorkflow, then `launchForDevice`'s read of the devices row.
    const bind = (launchDefaults: unknown, stored: unknown = claudeLaunch) => {
      selectQueue.push([workflow({ launch: stored })], [{ launchDefaults }])
    }

    it(`takes agent, account and BOTH models from the machine's defaults`, async () => {
      bind({
        defaultAgent: `codex`,
        defaultAccount: `p-7`,
        workflow: { model: `gpt-5.6-luna`, strongModel: `gpt-5.6-luna` },
      })
      await caller.update({ id: WF, deviceId: `dev-codex` })
      expect(written[0]!.values).toEqual({
        deviceId: `dev-codex`,
        launch: stored({
          agent: `codex`,
          account: `p-7`,
          model: `gpt-5.6-luna`,
          strongModel: `gpt-5.6-luna`,
        }),
      })
    })

    it(`falls back to the contract defaults for a machine that advertises none`, async () => {
      bind(null)
      await caller.update({ id: WF, deviceId: `dev-1` })
      expect(written[0]!.values).toEqual({
        deviceId: `dev-1`,
        launch: stored(claudeDefaults),
      })
    })

    it(`stands the first RUNNABLE agent in when the advertised one cannot run`, async () => {
      h.assertDeviceUsable.mockImplementation(codexOnly)
      bind({ defaultAgent: `claude`, defaultAccount: `p-1`, workflow: { model: `sonnet` } })
      await caller.update({ id: WF, deviceId: `dev-codex` })
      expect(written[0]!.values).toEqual({
        deviceId: `dev-codex`,
        // The account belonged to claude, so it goes with it.
        launch: stored(codexDefaults),
      })
      h.assertDeviceUsable.mockReset()
    })

    it(`still refuses a device that runs no agent at all`, async () => {
      h.assertDeviceUsable.mockImplementation(async (...args: unknown[]) => {
        if (args[3]) throw new TRPCError({ code: `BAD_REQUEST`, message: `${String(args[3])} is not available on that device` })
      })
      bind(null)
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

describe(`launchFromDeviceDefaults`, () => {
  it(`ignores a model outside the agent's own vocabulary`, () => {
    expect(
      launchFromDeviceDefaults({
        defaultAgent: `codex`,
        workflow: { model: `opus`, strongModel: `gpt-5.6-luna` },
      })
    ).toEqual({ agent: `codex`, model: `gpt-5.6-sol`, strongModel: `gpt-5.6-luna` })
  })

  it(`drops an account nobody advertised and an unknown agent`, () => {
    expect(launchFromDeviceDefaults({ defaultAgent: `pi` })).toEqual({
      agent: `claude`,
      model: `opus`,
      strongModel: `fable`,
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
    // A launch that reads fine is left alone.
    expect((written[1]!.values as { launch?: unknown }).launch).toBeUndefined()
  })

  // compat: a row an old client saved with a claude review pin on a codex
  // workflow (see `workflows.update`) folds to a strongModel codex cannot
  // start on; starting heals it to codex's default and writes that back.
  it(`heals a stored launch whose folded strongModel is outside the agent's vocabulary`, async () => {
    selectQueue.push([
      workflow({ ...ready, launch: { agent: `codex`, model: `gpt-5.6-sol`, reviewModel: `opus` } }),
    ])
    await caller.start({ id: WF })
    expect(h.assertDeviceUsable).toHaveBeenCalledWith(
      `dev-1`,
      TEAM,
      `user-1`,
      `codex`,
      expect.objectContaining({ cap: `workflows` })
    )
    expect(written[1]!.values).toMatchObject({
      status: `running`,
      launch: stored(codexDefaults),
    })
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

  // EXP-1065: nobody approves a node by hand; the procedure only stays
  // registered for clients that still show the button.
  it(`refuses a person's approval and writes nothing`, async () => {
    for (const approved of [true, false]) {
      selectQueue.push([node({ approvedAt: null })], [workflow()])
      const error = await rejection(caller.approveNode({ nodeId: NODE, approved } as never))
      expect(error?.code).toBe(`BAD_REQUEST`)
      expect(error?.message).toContain(`Nobody approves a node by hand`)
    }
    expect(written).toEqual([])
  })

  it(`holds an unapproved node at the gate, server-side`, async () => {
    selectQueue.push(
      [node()],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toEqual({
      merged: false,
      reason: `Waiting for the agent review to clear it`,
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
    expect((await caller.landNode({ nodeId: NODE })).reason).toBe(`Waiting for the agent review to clear it`)
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
    branch: `exp/wf-22222222-review-APP-6-r1`,
    resumedFromId: null,
    ...over,
  })
  // FEED-51: a reviewer resumed from a chat run (an account switch) carries
  // `agent` + the chat as its parent; only its branch and `resumed_from_id`
  // still say what it is.
  const RESUMED_RUN = `88888888-8888-4888-8888-888888888888`
  const resumed = (over: Record<string, unknown> = {}) =>
    reviewer({ id: RESUMED_RUN, startedReason: `agent`, resumedFromId: REVIEW_RUN, ...over })
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
    // The refusal tells the agent what to do next.
    expect(error?.message).toContain(`Check nodeId against the node named in your prompt`)
    expect(error?.message).toContain(`exponential_report_bug`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  // Every engine-started reviewer carries its review branch, so the branch
  // decides: `started_reason = workflow` on ANOTHER node's branch is another
  // node's reviewer, never a pass for this one.
  it(`refuses the workflow-started reviewer of node A submitting for node B`, async () => {
    selectQueue.push(
      [node({ issueId: B })],
      [running()],
      [{ id: `device-row` }],
      // The reviewer of APP-6 (node A), the engine's own start.
      [reviewer()],
      [{ identifier: `APP-10` }]
    )
    const error = await rejection(submit(REVIEW_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  // compat: a desktop/CLI 0.14.49/0.14.50 host spawns a second reviewer after
  // an account switch; both pass the gate and both submit for the same head.
  it(`answers a second verdict for the reviewed head with the stored outcome, claiming no round`, async () => {
    const review = {
      verdict: `approve`,
      findings: ``,
      oracle: { command: `bun test`, passed: true },
      model: null,
      round: 1,
      at: `2026-09-25T10:00:00.000Z`,
      head: `abc1234def`,
    }
    selectQueue.push(
      [node({ review, reviewRound: 1 })],
      [running()],
      [{ id: `device-row` }],
      [reviewer()],
      [{ identifier: `APP-6` }]
    )
    expect(await submit(REVIEW_RUN, { head: `abc1234def`, verdict: `request_changes` })).toEqual({
      round: 1,
      approve: true,
      state: `in_review`,
      note: `Agent review passed, backed by its checks`,
    })
    expect(fakeDb.transaction).not.toHaveBeenCalled()
    expect(written).toEqual([])

    // A NEW head is a fresh review, and so is the same head once a round was
    // claimed since (the author pushed and the engine re-reviewed).
    for (const over of [
      { review, reviewRound: 1, head: `ffff1234ab` },
      { review, reviewRound: 2, head: `abc1234def` },
    ]) {
      const { head, ...row } = over
      selectQueue.push([node(row)], [running()], [{ id: `device-row` }], [reviewer()], [{ identifier: `APP-6` }])
      updateQueue.push([{ round: row.reviewRound + 1 }])
      expect((await submit(REVIEW_RUN, { head })).round).toBe(row.reviewRound + 1)
    }
    expect(fakeDb.transaction).toHaveBeenCalledTimes(2)
  })

  it.each([
    [`a run that is not the review builtin`, reviewer({ actionName: `Fix merge conflicts` })],
    [`a review run a person started`, reviewer({ startedReason: null, branch: null })],
    [`an unknown run`, null],
  ])(`refuses %s`, async (_name, row) => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], row ? [row] : [])
    const error = await rejection(submit(REVIEW_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  // FEED-51: an account switch resumes the reviewer through the MCP path,
  // which re-brands the successor `agent` under the switching chat. The
  // successor is still the workflow's reviewer: its branch says so, and
  // failing that the guard walks `resumed_from_id` back.
  it(`accepts a reviewer resumed after an account switch through its succession`, async () => {
    selectQueue.push(
      [node()],
      [running()],
      [{ id: `device-row` }],
      [resumed({ branch: null })],
      [reviewer()],
      [{ identifier: `APP-6` }]
    )
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
      [resumed({ branch: null, resumedFromId: MIDDLE })],
      [resumed({ id: MIDDLE, branch: null })],
      [reviewer()],
      [{ identifier: `APP-6` }]
    )
    updateQueue.push([{ round: 1 }])
    await expect(submit(RESUMED_RUN)).resolves.toMatchObject({ round: 1 })
  })

  it.each([
    [`a resume chain that never reaches a workflow-started review run`, [resumed({ branch: null })], [reviewer({ startedReason: null, branch: null })]],
    [`a resume of the node's own run`, [resumed({ branch: null, resumedFromId: AUTHOR_RUN })], [reviewer({ id: AUTHOR_RUN })]],
    [`a resume whose predecessor is not the review builtin`, [resumed({ branch: null })], [reviewer({ actionName: `Chat` })]],
    [`a resume whose predecessor vanished`, [resumed({ branch: null })], []],
  ])(`refuses %s`, async (_name, ...rows) => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], ...rows)
    const error = await rejection(submit(RESUMED_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`stops on a resume loop instead of spinning`, async () => {
    selectQueue.push(
      [node()],
      [running()],
      [{ id: `device-row` }],
      [resumed({ branch: null, resumedFromId: RESUMED_RUN })]
    )
    const error = await rejection(submit(RESUMED_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.select).toHaveBeenCalledTimes(4)
  })

  it(`accepts a resumed reviewer on the node's review branch without walking the succession`, async () => {
    selectQueue.push(
      [node()],
      [running()],
      [{ id: `device-row` }],
      [resumed({ branch: `exp/wf-22222222-review-APP-6-r3` })],
      [{ identifier: `APP-6` }]
    )
    updateQueue.push([{ round: 3 }])
    const result = await submit(RESUMED_RUN, { head: `e39378c61d30` })
    expect(result).toMatchObject({ round: 3, approve: true })
    // node, workflow, device, reviewer, issue — never the predecessor.
    expect(fakeDb.select).toHaveBeenCalledTimes(5)
  })

  it(`follows a twice-resumed reviewer on another workflow's branch back to the run the engine started`, async () => {
    const MID = `99999999-9999-4999-8999-999999999999`
    selectQueue.push(
      [node()],
      [running()],
      [{ id: `device-row` }],
      [resumed({ branch: `exp/wf-00000000-review-APP-6-r1`, resumedFromId: MID })],
      [{ identifier: `APP-6` }],
      [{ actionName: `Review node`, startedReason: `agent`, resumedFromId: REVIEW_RUN }],
      [{ actionName: `Review node`, startedReason: `workflow`, resumedFromId: null }]
    )
    updateQueue.push([{ round: 2 }])
    expect(await submit(RESUMED_RUN)).toMatchObject({ round: 2, approve: true })
  })

  it(`refuses an agent-started review run on another node's branch with no succession`, async () => {
    selectQueue.push(
      [node()],
      [running()],
      [{ id: `device-row` }],
      // `APP-60`'s branch shares `APP-6`'s prefix up to the digit rule.
      [resumed({ branch: `exp/wf-22222222-review-APP-60-r1`, resumedFromId: null })],
      [{ identifier: `APP-6` }]
    )
    const error = await rejection(submit(RESUMED_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`refuses a succession that reaches a run which is not the review builtin`, async () => {
    selectQueue.push(
      [node()],
      [running()],
      [{ id: `device-row` }],
      [resumed({ branch: null })],
      [{ actionName: `Chat`, startedReason: null, resumedFromId: null }]
    )
    const error = await rejection(submit(RESUMED_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`refuses a resumed AUTHOR run even when it names the review branch`, async () => {
    // A node re-pointed at its author's resume: the author is never a review builtin.
    selectQueue.push(
      [node({ sessionId: AUTHOR_RUN })],
      [running()],
      [{ id: `device-row` }],
      [resumed({ actionName: null, branch: `exp/wf-22222222-review-APP-6-r1` })]
    )
    const error = await rejection(submit(RESUMED_RUN))
    expect(error?.code).toBe(`FORBIDDEN`)
  })

  it(`takes an oracle command longer than the old 500-character cap`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [reviewer()], [{ identifier: `APP-6` }])
    updateQueue.push([{ round: 1 }])
    const command = Array.from({ length: 20 }, (_, i) => `bun run test -- suite-${i}.test.ts`).join(` && `)
    expect(command.length).toBeGreaterThan(500)
    await submit(REVIEW_RUN, { oracle: { command, passed: true } })
    expect(written[1]!.values).toMatchObject({
      review: expect.objectContaining({ oracle: { command, passed: true } }),
    })
  })

  it(`claims the round in SQL, stores the reviewed head and approves on evidence`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [reviewer()], [{ identifier: `APP-6` }])
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
    selectQueue.push([node({ state: `paused` })], [running()], [{ id: `device-row` }], [reviewer()], [{ identifier: `APP-6` }])
    updateQueue.push([])
    const error = await rejection(submit(REVIEW_RUN))
    expect(error?.message).toContain(`not under review`)
    expect(written).toHaveLength(1)
  })

  it(`refuses an approval past the round cap`, async () => {
    selectQueue.push([node()], [running()], [{ id: `device-row` }], [reviewer()], [{ identifier: `APP-6` }])
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

  it(`sends an approval its own checks contradict back to the author`, () => {
    const outcome = reviewOutcome({ verdict: `approve`, oraclePassed: false, round: 1 })
    expect(outcome.approve).toBe(false)
    expect(outcome.state).toBe(`updating`)
    expect(outcome.note).toContain(`back to the author`)
  })

  // EXP-1065: the cap is a bound on bouncing, never a hand-off to a person.
  it(`bounces to the author up to the round cap, then lands with the findings carried`, () => {
    expect(
      reviewOutcome({ verdict: `request_changes`, oraclePassed: false, round: 2 })
    ).toMatchObject({ state: `updating`, note: `Changes requested by the agent review (round 2 of 3)` })
    expect(
      reviewOutcome({ verdict: `request_changes`, oraclePassed: null, round: 3 })
    ).toMatchObject({
      approve: false,
      state: `updating`,
      note: `Review round 3 of 3: findings go to the author once more, then it lands with them carried`,
    })
    expect(reviewOutcome({ verdict: `approve`, oraclePassed: false, round: 3 })).toMatchObject({
      approve: false,
      state: `updating`,
    })
    for (const verdict of [`approve`, `request_changes`] as const) {
      for (const round of [1, 2, 3, 4]) {
        for (const oraclePassed of [true, false, null]) {
          expect(reviewOutcome({ verdict, oraclePassed, round }).state).not.toBe(`waiting`)
        }
      }
    }
  })
})

// EXP-1032 — "the workflow never completes". The END STATE (status `done`,
// `ended_at`, every covered issue on the team's PR-merge target) is locked
// against the real writer in workflow-final-pr.test.ts; these two prove that
// both paths REACH it, webhook or no webhook.
describe(`workflow completion (EXP-1032)`, () => {
  const NODE = `55555555-5555-4555-8555-555555555555`
  const running = (over: Record<string, unknown> = {}) =>
    workflow({ status: `running`, deviceId: `dev-1`, ...over })

  it(`every node landed + final PR merged -> status completed, endedAt set, member issues done`, async () => {
    // Every node landed: the engine opens the ONE final pull request.
    selectQueue.push([running({ finalPrUrl: null })])
    expect(await caller.openFinalPr({ id: WF })).toEqual({ url: `https://gh/pr/9` })
    expect(h.openWorkflowFinalPr).toHaveBeenCalledWith(fakeDb, WF, `user-1`)

    // A member merges it from the workflow screen — GitHub's acceptance
    // completes the workflow RIGHT HERE (no inbound webhook required).
    selectQueue.push([
      running({ finalPrUrl: `https://gh/pr/9`, finalPrNumber: 9, finalPrState: `open` }),
    ])
    expect(await caller.mergeFinalPr({ id: WF })).toEqual({ merged: true })
    expect(h.mergeRepositoryPull).toHaveBeenCalledWith(
      expect.objectContaining({ prNumber: 9, prUrl: `https://gh/pr/9`, userId: `user-1` })
    )
    expect(h.applyWorkflowFinalPrState).toHaveBeenCalledWith(fakeDb, `https://gh/pr/9`, `merged`)
  })

  it(`never merges a final PR twice, and refuses one that does not exist`, async () => {
    selectQueue.push([
      running({ finalPrUrl: `https://gh/pr/9`, finalPrNumber: 9, finalPrState: `merged` }),
    ])
    expect(await caller.mergeFinalPr({ id: WF })).toEqual({ merged: true })
    expect(h.mergeRepositoryPull).not.toHaveBeenCalled()
    expect(h.applyWorkflowFinalPrState).not.toHaveBeenCalled()

    selectQueue.push([running({ finalPrUrl: null })])
    const error = await rejection(caller.mergeFinalPr({ id: WF }))
    expect(error?.code).toBe(`PRECONDITION_FAILED`)
  })

  it(`refuses to merge a cancelled workflow's final PR`, async () => {
    selectQueue.push([
      running({
        status: `cancelled`,
        finalPrUrl: `https://gh/pr/9`,
        finalPrNumber: 9,
        finalPrState: `open`,
      }),
    ])
    const error = await rejection(caller.mergeFinalPr({ id: WF }))
    expect(error?.code).toBe(`PRECONDITION_FAILED`)
    expect(error?.message).toContain(`cancelled`)
    expect(h.mergeRepositoryPull).not.toHaveBeenCalled()
  })

  it(`never merges into a repository of another team`, async () => {
    h.loadRepository.mockResolvedValueOnce({ id: `repo-1`, teamId: `other-team`, fullName: `o/r` })
    selectQueue.push([
      running({ finalPrUrl: `https://gh/pr/9`, finalPrNumber: 9, finalPrState: `open` }),
    ])
    const error = await rejection(caller.mergeFinalPr({ id: WF }))
    expect(error?.code).toBe(`NOT_FOUND`)
    expect(h.mergeRepositoryPull).not.toHaveBeenCalled()
    expect(h.applyWorkflowFinalPrState).not.toHaveBeenCalled()
  })

  it(`a node merged outside the train still lets the workflow complete`, async () => {
    // One node's PR was merged by hand into the integration branch: no
    // approval, and the merge train never ran for it.
    h.loadWorkflowEdges.mockResolvedValueOnce({
      nodes: [{ id: `node-1`, state: `in_review` }],
      edges: [],
    } as never)
    selectQueue.push(
      [{ id: `node-1`, workflowId: WF, issueId: A, kind: `leaf`, state: `in_review`, approvedAt: null }],
      [running()],
      [{ id: `device-row` }],
      [{ prState: `merged`, prMergedAt: null }]
    )
    expect(await caller.landNode({ nodeId: NODE })).toMatchObject({ merged: true })
    expect(written[0]!.values).toEqual({ state: `landed`, note: null })

    // It counts as landed like any other, so the final PR opens and the
    // merge completes the workflow.
    written.length = 0
    selectQueue.push([running({ finalPrUrl: null })])
    expect(await caller.openFinalPr({ id: WF })).toEqual({ url: `https://gh/pr/9` })
    selectQueue.push([
      running({ finalPrUrl: `https://gh/pr/9`, finalPrNumber: 9, finalPrState: `open` }),
    ])
    await caller.mergeFinalPr({ id: WF })
    expect(h.applyWorkflowFinalPrState).toHaveBeenCalledWith(fakeDb, `https://gh/pr/9`, `merged`)
  })
})

// EXP-1082 §3 — the engine's event log.
describe(`workflows.appendEvent`, () => {
  it(`refuses a caller that does not own the runner`, async () => {
    selectQueue.push([workflow({ status: `running`, deviceId: `dev-1` })], [])
    const error = await rejection(
      caller.appendEvent({ workflowId: WF, kind: `node_started`, message: `Started APP-6` })
    )
    expect(error?.code).toBe(`FORBIDDEN`)
    expect(fakeDb.insert).not.toHaveBeenCalled()
    expect(fakeDb.delete).not.toHaveBeenCalled()
  })

  it(`appends and trims to the newest 50`, async () => {
    const { PgDialect } = await import(`drizzle-orm/pg-core`)
    const wheres: unknown[] = []
    fakeDb.delete.mockImplementationOnce(() => {
      const p = chain([])
      p.where = (arg?: unknown) => {
        wheres.push(arg)
        return p
      }
      return p
    })
    selectQueue.push([workflow({ status: `running`, deviceId: `dev-1` })], [{ id: `device-row` }])
    const NODE = `55555555-5555-4555-8555-555555555555`
    await caller.appendEvent({
      workflowId: WF,
      nodeId: NODE,
      kind: `landed`,
      message: `Landed APP-6`,
    })
    expect(written).toContainEqual({
      op: `insert`,
      values: {
        workflowId: WF,
        teamId: TEAM,
        nodeId: NODE,
        sessionId: null,
        kind: `landed`,
        message: `Landed APP-6`,
      },
    })
    expect(wheres).toHaveLength(1)
    const query = new PgDialect().sqlToQuery(wheres[0] as never)
    expect(query.sql).toMatch(/NOT IN/)
    expect(query.sql).toMatch(/ORDER BY "workflow_events"\."at" DESC, "workflow_events"\."id" DESC/)
    expect(query.params).toContain(50)
    expect(query.params.filter((p) => p === WF)).toHaveLength(2)
  })

  it(`refuses an unknown kind`, async () => {
    const error = await rejection(
      caller.appendEvent({ workflowId: WF, kind: `exploded` as never, message: `` })
    )
    expect(error?.code).toBe(`BAD_REQUEST`)
  })
})

// EXP-1082 §4 / EXP-1065 — node states + questions: the review cap clears a
// node without anyone's approval and CARRIES what it left open; an open
// question is a badge, never a state.
describe(`node states (EXP-1082 §4)`, () => {
  const NODE = `55555555-5555-4555-8555-555555555555`
  const capped = (review: Record<string, unknown>) => ({
    id: NODE,
    workflowId: WF,
    issueId: A,
    kind: `leaf`,
    state: `in_review`,
    approvedAt: null,
    sessionId: `66666666-6666-4666-8666-666666666666`,
    reviewRound: 3,
    review: { round: 3, findings: ``, oracle: null, model: null, at: ``, ...review },
  })
  const land = async (node: Record<string, unknown>) => {
    selectQueue.push(
      [node],
      [workflow({ status: `running`, deviceId: `dev-1` })],
      [{ id: `device-row` }],
      [{ prState: `open` }],
      // the landed write's decisions-log update
      [{ identifier: `APP-6` }],
      [{ decisions: `2026-09-19: ship the API first` }]
    )
    return caller.landNode({ nodeId: NODE })
  }
  const decisionsWritten = () =>
    written
      .filter((w) => w.op === `update`)
      .map((w) => (w.values as { decisions?: string }).decisions)
      .find((d) => typeof d === `string`)

  it(`a node at the review cap lands after the author's last push and carries its findings`, async () => {
    expect(
      await land(capped({ verdict: `request_changes`, findings: `src/a.ts:4 off by one\nsrc/b.ts: no test` }))
    ).toEqual({ merged: true, reason: null, retargeted: [] })
    expect(h.mergePr).toHaveBeenCalledWith({ issueId: A, endSessions: true })
    expect(written[0]!.values).toEqual({ state: `landed`, note: null })
    expect(decisionsWritten()).toBe(
      `2026-09-19: ship the API first\n${new Date().toISOString().slice(0, 10)}: Unresolved review findings (APP-6, round 3): src/a.ts:4 off by one src/b.ts: no test`
    )
    expect(written).toContainEqual({
      op: `insert`,
      values: expect.objectContaining({ kind: `cleared_at_cap`, nodeId: NODE, workflowId: WF }),
    })
  })

  it(`a failed oracle at the cap lands and carries the failure`, async () => {
    expect(
      await land(
        capped({ verdict: `approve`, findings: `flaky`, oracle: { command: `bun test`, passed: false } })
      )
    ).toMatchObject({ merged: true })
    expect(decisionsWritten()).toContain(
      `Unresolved review findings (APP-6, round 3): flaky Checks failed: bun test`
    )
  })

  it(`a node below the cap, or one whose checks passed, still waits for its review`, async () => {
    expect(
      (await land(capped({ verdict: `request_changes`, round: 2 }))).reason
    ).toBe(`Waiting for the agent review to clear it`)
    expect(h.mergePr).not.toHaveBeenCalled()
  })

  it(`an open question never changes the node state, only the badge`, async () => {
    const { workflowOpenQuestions } = await vi.importActual<
      typeof import("@/lib/workflows/open-questions")
    >(`@/lib/workflows/open-questions`)
    const askedAt = `2026-09-25T10:00:00Z`
    // The badge comes off the run's own row …
    expect(
      workflowOpenQuestions(
        [
          {
            id: `run-1`,
            workflowId: WF,
            workflowNodeId: NODE,
            pendingQuestion: { question: `Proposal: keep the enum?`, askedAt },
            status: `running`,
          },
        ],
        WF
      )
    ).toEqual([{ nodeId: NODE, sessionId: `run-1`, question: `Proposal: keep the enum?`, askedAt }])
    // … and nothing the server writes to a node ever reads `waiting`.
    expect(reviewOutcome({ verdict: `request_changes`, oraclePassed: null, round: 3 }).state).toBe(
      `updating`
    )
  })
})

// EXP-1082 §7 — the final PR (EXP-1072 / EXP-1065 implement these).
describe(`final PR (EXP-1082 §7)`, () => {
  it.skip(`a closed final PR is reopened once`, () => {
    // A final PR closed unmerged is reopened ONCE (`final_pr_reopened`),
    // a second close leaves it closed.
  })
  it.skip(`an all-skipped workflow is cancelled with a note`, () => {
    // Every node skipped = nothing to ship: status `cancelled` + a decision line.
  })
  it.skip(`final merge flips member issues through the PR-merge helper`, () => {
    // mergeFinalPr → every member issue moves via the team's PR-merge target.
  })
  it(`the final PR body carries findings, decisions and results`, async () => {
    const { finalPrBody } = await vi.importActual<typeof import("@/lib/workflow-final-pr")>(
      `@/lib/workflow-final-pr`
    )
    const body = finalPrBody({
      name: `Login rework`,
      nodes: [{ identifier: `APP-6`, title: `Leaf`, prUrl: null, kind: `leaf` }],
      audit: [],
      decisions: `2026-09-25: Unresolved review findings (APP-6, round 3): src/a.ts:4 off by one`,
      findings: [
        { identifier: `APP-6`, round: 3, findings: `src/a.ts:4 off by one`, oracleCommand: `bun test` },
      ],
      results: [
        { identifier: `APP-6`, topic: `login`, label: `web`, url: `https://app.test/api/attachments/a1` },
      ],
    })
    expect(body).toContain(`## Unresolved review findings\n`)
    expect(body).toContain(`- [ ] #APP-6 (review round 3)\n  src/a.ts:4 off by one\n  Checks failed: bun test`)
    expect(body).toContain(`## Decisions\n2026-09-25: Unresolved review findings`)
    expect(body).toContain(`## Results\n`)
    expect(body).toContain(`### APP-6 · login\n- [web](https://app.test/api/attachments/a1)`)
  })
})
