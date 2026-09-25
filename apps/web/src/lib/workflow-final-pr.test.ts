import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

const h = vi.hoisted(() => ({
  resolveRepoToken: vi.fn(async (..._args: unknown[]): Promise<string | null> => `tok`),
  retargetPullRequest: vi.fn(async (..._args: unknown[]) => {}),
  getPullRequest: vi.fn(async (..._args: unknown[]) => ({ baseRef: `main` })),
  applyPrLifecycleStatusInTx: vi.fn(async (..._args: unknown[]) => {}),
  loadWorkflowEdges: vi.fn(
    async (..._args: unknown[]) => ({
      nodes: [] as Array<Record<string, unknown>>,
      edges: [] as Array<[string, string]>,
    })
  ),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/integrations/github-pr`, () => ({
  createPullRequest: vi.fn(),
  resolveRepoToken: h.resolveRepoToken,
  retargetPullRequest: h.retargetPullRequest,
  getPullRequest: h.getPullRequest,
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrLifecycleStatusInTx: h.applyPrLifecycleStatusInTx,
}))
vi.mock(`@/lib/workflows`, () => ({ loadWorkflowEdges: h.loadWorkflowEdges }))
vi.mock(`@/lib/trpc/repositories`, () => ({ effectiveDefaultBranch: () => `main` }))
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoDefaultBranchCached: vi.fn(),
}))

import {
  applyWorkflowFinalPrState,
  ensureNodePrOnIntegrationBranch,
  finalPrBody,
  NODE_PR_OFF_BRANCH_REASON,
  pickAuditNodes,
  retargetReleasedDependents,
} from "@/lib/workflow-final-pr"

// Fake db: a FIFO of select results; every UPDATE's `set` and every `where`
// argument is recorded (the latter rendered to SQL to prove the predicate).
const selectQueue: unknown[][] = []
const updates: Array<Record<string, unknown>> = []
// What the next UPDATE ... RETURNING resolves with (FIFO; default = one row).
const updateQueue: unknown[][] = []
const wheres: string[] = []
const dialect = new PgDialect()
type Chain = Promise<unknown[]> & Record<string, (arg?: unknown) => unknown>
function chain(result: unknown[]): Chain {
  const p = Promise.resolve(result) as Chain
  for (const m of [`from`, `innerJoin`, `limit`, `orderBy`, `returning`]) p[m] = () => p
  p.where = (arg?: unknown) => {
    if (arg) {
      const q = dialect.sqlToQuery(arg as never)
      wheres.push(`${q.sql} ${JSON.stringify(q.params)}`)
    }
    return p
  }
  return p
}
const fakeDb = {
  select: () => chain(selectQueue.shift() ?? []),
  update: () => {
    const p = chain(updateQueue.shift() ?? [{ id: `wf-1` }])
    p.set = (values: unknown) => {
      updates.push(values as Record<string, unknown>)
      return p
    }
    return p
  },
  transaction: async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeDb),
} as never

beforeEach(() => {
  selectQueue.length = 0
  updates.length = 0
  updateQueue.length = 0
  wheres.length = 0
  vi.clearAllMocks()
  h.resolveRepoToken.mockResolvedValue(`tok`)
  h.retargetPullRequest.mockResolvedValue(undefined)
  h.getPullRequest.mockResolvedValue({ baseRef: `main` })
})

const WF = `wf-1`
const INTEGRATION = `exp/wf-11111111`

// EXP-982 — the final PR's body and its audit pick.
describe(`pickAuditNodes`, () => {
  const nodes = Array.from({ length: 10 }, (_, i) => `n${i}`)

  it(`picks k distinct nodes, the same ones for the same workflow`, () => {
    const first = pickAuditNodes(nodes, `wf-1`, 3)
    expect(first).toHaveLength(3)
    expect(new Set(first).size).toBe(3)
    expect(pickAuditNodes(nodes, `wf-1`, 3)).toEqual(first)
    expect(pickAuditNodes(nodes, `wf-2`, 3)).not.toEqual(first)
  })

  it(`never asks for more than there is`, () => {
    expect(pickAuditNodes([`a`, `b`], `wf`, 3).sort()).toEqual([`a`, `b`])
    expect(pickAuditNodes([], `wf`, 3)).toEqual([])
  })
})

describe(`finalPrBody`, () => {
  it(`lists every node as an issue ref, the audit picks as a checklist, and the decisions`, () => {
    const body = finalPrBody({
      name: `Mobile polish`,
      nodes: [
        { identifier: `APP-6`, title: `Contract`, prUrl: `https://gh/pr/1`, kind: `contract` },
        { identifier: `APP-7`, title: `Leaf`, prUrl: null, kind: `leaf` },
      ],
      audit: [{ identifier: `APP-7`, prUrl: null }],
      decisions: `2026-09-19: use cursor pagination`,
    })
    expect(body).toContain(`- #APP-6 (contract): https://gh/pr/1`)
    expect(body).toContain(`- #APP-7`)
    expect(body).toContain(`- [ ] #APP-7`)
    expect(body).toContain(`claims, not evidence`)
    expect(body).toContain(`## Decisions\n2026-09-19: use cursor pagination`)
  })

  it(`leaves the optional sections out when empty`, () => {
    const body = finalPrBody({ name: `X`, nodes: [], audit: [], decisions: `` })
    expect(body).not.toContain(`## Audit`)
    expect(body).not.toContain(`## Decisions`)
  })
})

// The merge train's base assertion (landNode): a node PR merges into the
// integration branch or not at all.
describe(`ensureNodePrOnIntegrationBranch`, () => {
  const args = {
    issueId: `issue-d`,
    repositoryId: `repo-1`,
    teamId: `team-1`,
    integrationBranch: INTEGRATION,
    actorUserId: `user-1`,
  }

  it(`passes a PR already based on the integration branch without asking GitHub`, async () => {
    selectQueue.push([{ prNumber: 5, prState: `open`, prBaseBranch: INTEGRATION }])
    expect(await ensureNodePrOnIntegrationBranch(fakeDb, args)).toEqual({
      ok: true,
      retargeted: false,
    })
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
    expect(h.getPullRequest).not.toHaveBeenCalled()
  })

  it(`retargets a PR the stack heal moved onto the default branch, then passes`, async () => {
    selectQueue.push(
      [{ prNumber: 5, prState: `open`, prBaseBranch: `main` }],
      [{ fullName: `owner/repo` }]
    )
    expect(await ensureNodePrOnIntegrationBranch(fakeDb, args)).toEqual({
      ok: true,
      retargeted: true,
    })
    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 5,
      base: INTEGRATION,
      token: `tok`,
    })
    expect(updates).toEqual([{ prBaseBranch: INTEGRATION }])
  })

  it(`asks GitHub when the base is unknown locally and records what it says`, async () => {
    selectQueue.push(
      [{ prNumber: 5, prState: `open`, prBaseBranch: null }],
      [{ fullName: `owner/repo` }]
    )
    h.getPullRequest.mockResolvedValueOnce({ baseRef: INTEGRATION })
    expect(await ensureNodePrOnIntegrationBranch(fakeDb, args)).toEqual({
      ok: true,
      retargeted: false,
    })
    expect(h.getPullRequest).toHaveBeenCalledWith(`owner/repo`, 5, `tok`)
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
    expect(updates).toEqual([{ prBaseBranch: INTEGRATION }])
  })

  it(`refuses (waiting) without a token, a PATCH failure, or without an open PR`, async () => {
    selectQueue.push(
      [{ prNumber: 5, prState: `open`, prBaseBranch: `main` }],
      [{ fullName: `owner/repo` }]
    )
    h.resolveRepoToken.mockResolvedValueOnce(null)
    expect(await ensureNodePrOnIntegrationBranch(fakeDb, args)).toEqual({
      ok: false,
      reason: NODE_PR_OFF_BRANCH_REASON,
    })

    const errorSpy = vi.spyOn(console, `error`).mockImplementation(() => undefined)
    selectQueue.push(
      [{ prNumber: 5, prState: `open`, prBaseBranch: `main` }],
      [{ fullName: `owner/repo` }]
    )
    h.retargetPullRequest.mockRejectedValueOnce(new Error(`422`))
    expect(await ensureNodePrOnIntegrationBranch(fakeDb, args)).toEqual({
      ok: false,
      reason: NODE_PR_OFF_BRANCH_REASON,
    })
    errorSpy.mockRestore()
    expect(updates).toEqual([])

    selectQueue.push([{ prNumber: null, prState: null, prBaseBranch: null }])
    expect((await ensureNodePrOnIntegrationBranch(fakeDb, args)).ok).toBe(false)
  })
})

// EXP-983: the node's recorded base follows the PR, never the other way.
describe(`retargetReleasedDependents`, () => {
  const released = {
    id: `node-d`,
    issueId: `issue-d`,
    memberIssueIds: [],
    state: `in_review`,
    baseBranch: `exp/APP-6`,
  }
  const graph = () => ({
    nodes: [
      { id: `node-a`, issueId: `issue-a`, memberIssueIds: [], state: `landed`, baseBranch: null },
      released,
    ],
    edges: [[`node-a`, `node-d`]] as Array<[string, string]>,
  })
  const workflowRow = { teamId: `team-1`, repositoryId: `repo-1`, integrationBranch: INTEGRATION }

  it(`flips the node's base only after GitHub took the new base`, async () => {
    h.loadWorkflowEdges.mockResolvedValueOnce(graph())
    selectQueue.push(
      [workflowRow],
      [{ fullName: `owner/repo` }],
      [{ id: `issue-d`, prNumber: 9, prState: `open`, prBaseBranch: `exp/APP-6` }]
    )
    expect(await retargetReleasedDependents(fakeDb, WF, `node-a`, `user-1`)).toEqual([`node-d`])
    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 9,
      base: INTEGRATION,
      token: `tok`,
    })
    expect(updates).toEqual([{ prBaseBranch: INTEGRATION }, { baseBranch: INTEGRATION }])
  })

  it(`leaves the node untouched without a token or when the PATCH fails`, async () => {
    const errorSpy = vi.spyOn(console, `error`).mockImplementation(() => undefined)
    h.loadWorkflowEdges.mockResolvedValueOnce(graph())
    h.resolveRepoToken.mockResolvedValueOnce(null)
    selectQueue.push(
      [workflowRow],
      [{ fullName: `owner/repo` }],
      [{ id: `issue-d`, prNumber: 9, prState: `open`, prBaseBranch: `exp/APP-6` }]
    )
    expect(await retargetReleasedDependents(fakeDb, WF, `node-a`, `user-1`)).toEqual([])
    expect(updates).toEqual([])

    h.loadWorkflowEdges.mockResolvedValueOnce(graph())
    h.retargetPullRequest.mockRejectedValueOnce(new Error(`422`))
    selectQueue.push(
      [workflowRow],
      [{ fullName: `owner/repo` }],
      [{ id: `issue-d`, prNumber: 9, prState: `open`, prBaseBranch: `exp/APP-6` }]
    )
    expect(await retargetReleasedDependents(fakeDb, WF, `node-a`, `user-1`)).toEqual([])
    expect(updates).toEqual([])
    errorSpy.mockRestore()
  })

  it(`moves a node with no PR yet (nothing on GitHub to move)`, async () => {
    h.loadWorkflowEdges.mockResolvedValueOnce(graph())
    h.resolveRepoToken.mockResolvedValueOnce(null)
    selectQueue.push(
      [workflowRow],
      [{ fullName: `owner/repo` }],
      [{ id: `issue-d`, prNumber: null, prState: null, prBaseBranch: null }]
    )
    expect(await retargetReleasedDependents(fakeDb, WF, `node-a`, `user-1`)).toEqual([`node-d`])
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
    expect(updates).toEqual([{ baseBranch: INTEGRATION }])
  })
})

// The final PR's merge ships what LANDED, nothing a person skipped or never
// admitted.
describe(`applyWorkflowFinalPrState`, () => {
  it(`moves only the issues of landed nodes on merge`, async () => {
    selectQueue.push(
      [{ id: WF, teamId: `team-1`, finalPrState: `open` }],
      [{ issueId: `issue-a`, members: [`issue-a1`] }],
      [
        { id: `issue-a`, status: `in_progress` },
        { id: `issue-a1`, status: `in_progress` },
      ]
    )
    expect(await applyWorkflowFinalPrState(fakeDb, `https://gh/pr/1`, `merged`)).toBe(true)
    expect(updates[0]).toMatchObject({ finalPrState: `merged`, status: `done` })
    // The node read is filtered to state = landed.
    expect(wheres.some((w) => w.includes(`"state" = $`) && w.includes(`"landed"`))).toBe(true)
    expect(h.applyPrLifecycleStatusInTx).toHaveBeenCalledTimes(2)
    expect(h.applyPrLifecycleStatusInTx).toHaveBeenCalledWith(
      fakeDb,
      expect.objectContaining({ issueId: `issue-a1`, event: `merged` })
    )
  })

  // EXP-1032 — completion is what the merge of the final PR MEANS, and it
  // must not depend on the webhook: the in-app merge and the poller both
  // land here, so applying twice has to be a no-op.
  it(`completes the workflow on merge: status done and endedAt stamped`, async () => {
    selectQueue.push(
      [{ id: WF, teamId: `team-1`, finalPrState: `open` }],
      [{ issueId: `issue-a`, members: [] }],
      [{ id: `issue-a`, status: `in_review` }]
    )
    await applyWorkflowFinalPrState(fakeDb, `https://gh/pr/1`, `merged`)
    expect(updates[0]).toMatchObject({ finalPrState: `merged`, status: `done` })
    expect(updates[0]!.endedAt).toBeInstanceOf(Date)
  })

  it(`applies a merge exactly once: a row that already reads merged writes nothing`, async () => {
    selectQueue.push([{ id: WF, teamId: `team-1`, finalPrState: `merged` }])
    expect(await applyWorkflowFinalPrState(fakeDb, `https://gh/pr/1`, `merged`)).toBe(true)
    expect(updates).toEqual([])
    expect(h.applyPrLifecycleStatusInTx).not.toHaveBeenCalled()
  })

  // `mergeFinalPr` and the webhook both read `open`, then race to write: the
  // UPDATE is the claim, and only the caller whose UPDATE took the row runs
  // the covered-issue fan-out.
  it(`claims the merge atomically: the caller whose UPDATE matched no row fans out nothing`, async () => {
    selectQueue.push(
      [{ id: WF, teamId: `team-1`, finalPrState: `open` }],
      [{ issueId: `issue-a`, members: [] }],
      [{ id: `issue-a`, status: `in_review` }]
    )
    // The other caller claimed it between this read and this write.
    updateQueue.push([])
    expect(await applyWorkflowFinalPrState(fakeDb, `https://gh/pr/1`, `merged`)).toBe(true)
    expect(updates).toHaveLength(1)
    expect(h.applyPrLifecycleStatusInTx).not.toHaveBeenCalled()
    // The predicate itself: id AND not yet merged.
    const claim = wheres.find((w) => w.includes(`"workflows"."id" = $`))
    expect(claim).toMatch(/"final_pr_state" is distinct from 'merged'/i)
  })

  it(`records a close or a reopen without completing anything`, async () => {
    for (const state of [`closed`, `open`] as const) {
      updates.length = 0
      selectQueue.push([{ id: WF, teamId: `team-1`, finalPrState: `open` }])
      await applyWorkflowFinalPrState(fakeDb, `https://gh/pr/1`, state)
      expect(updates[0]).toEqual({ finalPrState: state })
      expect(h.applyPrLifecycleStatusInTx).not.toHaveBeenCalled()
    }
  })

  it(`is not a workflow's PR: says so and touches nothing`, async () => {
    selectQueue.push([])
    expect(await applyWorkflowFinalPrState(fakeDb, `https://gh/pr/7`, `merged`)).toBe(false)
    expect(updates).toEqual([])
  })
})
