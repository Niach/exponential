import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue, SyncedWorkflow, WorkflowNode } from "@/db/schema"
import { BUILTIN_PLAN_WORKFLOW_ID } from "@/lib/builtin-actions"
import {
  APPROVE_NODE_LABEL,
  CANCEL_WORKFLOW_LABEL,
  CONTRACT_PUBLISHED_LABEL,
  DELETE_WORKFLOW_LABEL,
  MERGE_TRAIN_EMPTY,
  PAUSE_WORKFLOW_LABEL,
  PLAN_WORKFLOW_LABEL,
  RESUME_WORKFLOW_LABEL,
  RETRY_NODE_LABEL,
  SKIP_NODE_CONFIRM,
  SKIP_NODE_LABEL,
  START_WORKFLOW_LABEL,
  WITHDRAW_APPROVAL_LABEL,
} from "@/lib/workflow-view"

// EXP-981: the workflow detail — the graph positioned by the SERVER's
// wave/lane, a compound node drawn as a stacked card, the cycle note, and the
// two actions. The layout is never computed here, so the assertions read the
// grid's `data-wave`/`data-lane` straight back.
//
// EXP-982: the same page once the workflow RUNS — the header's actions follow
// the status, the merge train says what lands next, the final-PR node closes
// the graph, and the node panel carries approve/withdraw and retry/skip.
//
// EXP-983: an edge is drawn by its STYLE (`workflowEdgeStyle`), a serialization
// edge dashed on top of it, and the panel says what a speculative start added:
// the published contract, and the nodes this one merges in first.

const nodeRows = vi.hoisted(() => ({ rows: [] as unknown[] }))
const graphState = vi.hoisted(() => ({
  relations: [] as { type: string; issueId: string; relatedIssueId: string }[],
  issues: [] as unknown[],
}))
const openComposer = vi.hoisted(() => vi.fn())
const navigate = vi.hoisted(() => vi.fn())
const deleteMutate = vi.hoisted(() => vi.fn().mockResolvedValue({ txId: 1 }))
const updateMutate = vi.hoisted(() =>
  vi.fn().mockResolvedValue({ txId: 1, workflow: {} })
)
const runMutates = vi.hoisted(() => ({
  start: vi.fn().mockResolvedValue({ txId: 1, workflow: {} }),
  pause: vi.fn().mockResolvedValue({ txId: 1 }),
  resume: vi.fn().mockResolvedValue({ txId: 1 }),
  cancel: vi.fn().mockResolvedValue({ txId: 1 }),
  approveNode: vi.fn().mockResolvedValue({ txId: 1 }),
  resolveNode: vi.fn().mockResolvedValue({ txId: 1 }),
}))

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({ children, ...rest }: { children: React.ReactNode }) => (
    <a {...rest}>{children}</a>
  ),
  useNavigate: () => navigate,
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => false,
}))
vi.mock(`@/hooks/use-workflows`, () => ({
  useWorkflowNodes: () => nodeRows.rows,
}))
vi.mock(`@/hooks/use-team-issue-graph`, () => ({
  useTeamIssueGraph: () => ({ ...graphState, counts: new Map() }),
}))
vi.mock(`@/hooks/use-open-composer`, () => ({
  useOpenComposer: () => openComposer,
}))
vi.mock(`@/hooks/use-remote-start`, () => ({
  useRemoteStart: () => ({ devices: [], starting: false, sentTo: null }),
}))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: null }) }))
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamBoards: () => [] }))
vi.mock(`@/components/issue-chip`, () => ({
  IssueChip: ({ issue }: { issue: { identifier: string } }) => (
    <span data-testid={`chip-${issue.identifier}`}>{issue.identifier}</span>
  ),
}))
vi.mock(`@/lib/collections`, () => ({
  workflowCollection: { utils: { awaitTxId: vi.fn().mockResolvedValue(true) } },
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    workflows: {
      update: { mutate: updateMutate },
      delete: { mutate: deleteMutate },
      updateNode: { mutate: vi.fn().mockResolvedValue({ txId: 1 }) },
      start: { mutate: runMutates.start },
      pause: { mutate: runMutates.pause },
      resume: { mutate: runMutates.resume },
      cancel: { mutate: runMutates.cancel },
      approveNode: { mutate: runMutates.approveNode },
      resolveNode: { mutate: runMutates.resolveNode },
    },
  },
}))

import { WorkflowDetail } from "@/components/workflow-detail"

const issue = (
  id: string,
  identifier: string,
  over: Partial<Issue> = {}
): Issue =>
  ({
    id,
    identifier,
    title: `Issue ${identifier}`,
    status: `backlog`,
    priority: `none`,
    boardId: `b1`,
    prNumber: null,
    prState: null,
    ...over,
  }) as unknown as Issue

const node = (
  id: string,
  over: Partial<WorkflowNode> = {}
): WorkflowNode =>
  ({
    id,
    workflowId: `wf`,
    teamId: `t1`,
    issueId: `i-${id}`,
    memberIssueIds: [],
    kind: `leaf`,
    state: `blocked`,
    risk: `low`,
    wave: 0,
    lane: 0,
    onCycle: false,
    touches: [],
    sessionId: null,
    approvedAt: null,
    checkpointAt: null,
    afterNodeIds: [],
    note: null,
    ...over,
  }) as unknown as WorkflowNode

const workflow = (over: Partial<SyncedWorkflow> = {}): SyncedWorkflow =>
  ({
    id: `wf`,
    teamId: `t1`,
    name: `APP-1 +2`,
    status: `draft`,
    gate: `human`,
    startOn: `contract`,
    deviceId: null,
    repositoryId: null,
    finalPrUrl: null,
    finalPrNumber: null,
    finalPrState: null,
    launch: {},
    metrics: { nodes: 2, edges: 1, depth: 2, width: 1, cycles: [] },
    ...over,
  }) as unknown as SyncedWorkflow

/** A draft with nothing left in the way of Start (`workflowStartBlocker`).
 *  EXP-983: the start rule is NOT one of those things any more — the default
 *  `contract` starts exactly like `landed` does. */
const startable = (over: Partial<SyncedWorkflow> = {}): Partial<SyncedWorkflow> =>
  ({
    repositoryId: `r1`,
    deviceId: `dev-1`,
    ...over,
  }) as Partial<SyncedWorkflow>

function mount(over: Partial<SyncedWorkflow> = {}) {
  return render(<WorkflowDetail workflow={workflow(over)} teamSlug="acme" />)
}

describe(`WorkflowDetail graph`, () => {
  it(`positions nodes from the synced wave and lane`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, kind: `contract` }),
      node(`n2`, { wave: 1, lane: 0 }),
      node(`n3`, { wave: 1, lane: 1, risk: `high` }),
    ]
    graphState.issues = [
      issue(`i-n1`, `APP-1`),
      issue(`i-n2`, `APP-2`),
      issue(`i-n3`, `APP-3`),
    ]
    graphState.relations = [
      { type: `blocks`, issueId: `i-n1`, relatedIssueId: `i-n2` },
      { type: `blocks`, issueId: `i-n1`, relatedIssueId: `i-n3` },
    ]
    mount()
    const at = (id: string) => {
      const box = screen.getByTestId(`workflow-node-${id}`)
      return [box.getAttribute(`data-wave`), box.getAttribute(`data-lane`)]
    }
    expect(at(`n1`)).toEqual([`0`, `0`])
    expect(at(`n2`)).toEqual([`1`, `0`])
    expect(at(`n3`)).toEqual([`1`, `1`])
    // One edge per node pair, from the team's synced `blocks` rows.
    expect(screen.getAllByTestId(`workflow-graph-edge`)).toHaveLength(2)
    // A draft's caption names the PLAN, not a state.
    expect(
      screen.getByTestId(`workflow-node-n1-caption`).textContent
    ).toBe(`Contract`)
    expect(
      screen.getByTestId(`workflow-node-n3-caption`).textContent
    ).toBe(`Leaf · high risk`)
  })

  it(`draws a compound node as a stacked card titled with its members`, () => {
    nodeRows.rows = [node(`n1`, { memberIssueIds: [`i-a`, `i-b`, `i-c`] })]
    graphState.issues = [issue(`i-n1`, `APP-14`)]
    graphState.relations = []
    mount()
    expect(screen.getByTestId(`workflow-node-n1-stack`)).toBeTruthy()
    expect(screen.getByTestId(`chip-APP-14 +3`)).toBeTruthy()
  })

  it(`keeps a node whose issue has not synced, caption and all`, () => {
    nodeRows.rows = [node(`n1`, { state: `ready` })]
    graphState.issues = []
    graphState.relations = []
    mount({ status: `running` })
    expect(screen.getByTestId(`workflow-node-n1`)).toBeTruthy()
    expect(screen.getByTestId(`workflow-node-n1-caption`).textContent).toBe(
      `Ready`
    )
  })

  it(`spells out a blocking cycle and paints its edges red`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, onCycle: true }),
      node(`n2`, { wave: 1, lane: 0, onCycle: true }),
    ]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-n2`, `APP-2`)]
    graphState.relations = [
      { type: `blocks`, issueId: `i-n1`, relatedIssueId: `i-n2` },
      { type: `blocks`, issueId: `i-n2`, relatedIssueId: `i-n1` },
    ]
    mount({
      metrics: {
        nodes: 2,
        edges: 2,
        depth: 1,
        width: 2,
        cycles: [[`APP-1`, `APP-2`]],
        cycleEdges: [`n1\nn2`, `n2\nn1`],
      },
    })
    expect(screen.getByTestId(`workflow-cycle-note`).textContent).toBe(
      `These issues block each other in a cycle: APP-1, APP-2. Remove one relation to start.`
    )
    expect(screen.getAllByTestId(`workflow-graph-cycle-edge`)).toHaveLength(2)
  })

  it(`opens a node's panel with its kind and risk pickers`, () => {
    nodeRows.rows = [
      node(`n1`, { memberIssueIds: [`i-m`], touches: [`apps/web/**`] }),
    ]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-m`, `APP-9`)]
    graphState.relations = []
    mount()
    expect(screen.queryByTestId(`workflow-node-panel`)).toBeNull()
    fireEvent.click(screen.getByTestId(`workflow-node-n1-card`))
    const panel = screen.getByTestId(`workflow-node-panel`)
    expect(panel.textContent).toContain(`Kind`)
    expect(panel.textContent).toContain(`Risk`)
    expect(screen.getByTestId(`workflow-node-members`).textContent).toContain(
      `APP-9`
    )
    expect(screen.getByTestId(`workflow-node-touches`).textContent).toBe(
      `apps/web/**`
    )
  })
})

describe(`WorkflowDetail actions`, () => {
  it(`Plan seeds the composer with the hidden builtin and the workflow`, () => {
    nodeRows.rows = []
    graphState.issues = []
    graphState.relations = []
    mount()
    expect(screen.getByTestId(`workflow-plan`).textContent).toContain(
      PLAN_WORKFLOW_LABEL
    )
    fireEvent.click(screen.getByTestId(`workflow-plan`))
    expect(openComposer).toHaveBeenCalledWith({
      actionId: BUILTIN_PLAN_WORKFLOW_ID,
      workflowId: `wf`,
    })
  })

  it(`Delete asks first, then deletes and returns to the list`, async () => {
    nodeRows.rows = []
    mount()
    fireEvent.click(screen.getByTestId(`workflow-delete`))
    expect(screen.getAllByText(DELETE_WORKFLOW_LABEL).length).toBeGreaterThan(1)
    fireEvent.click(screen.getByTestId(`workflow-delete-confirm`))
    await vi.waitFor(() =>
      expect(deleteMutate).toHaveBeenCalledWith(
        { id: `wf` },
        expect.anything()
      )
    )
    await vi.waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(
        expect.objectContaining({ to: `/t/$teamSlug/workflows` })
      )
    )
  })

})

describe(`WorkflowDetail run actions`, () => {
  const reset = () => {
    nodeRows.rows = []
    graphState.issues = []
    graphState.relations = []
    for (const mutate of Object.values(runMutates)) mutate.mockClear()
  }

  it(`disables Start and says why while a draft is not ready`, () => {
    reset()
    // Everything else is in place; the runner device is not picked yet.
    mount(startable({ deviceId: null }))
    const start = screen.getByTestId(`workflow-start`) as HTMLButtonElement
    expect(start.textContent).toContain(START_WORKFLOW_LABEL)
    expect(start.disabled).toBe(true)
    expect(screen.getByTestId(`workflow-start-blocker`).textContent).toBe(
      `Pick the device that runs this workflow first.`
    )
  })

  // EXP-983: all three start rules run, so none of them holds Start back.
  it(`starts a ready draft on every start rule`, async () => {
    reset()
    for (const startOn of [`contract`, `pr_open`, `landed`] as const) {
      const view = mount(startable({ startOn }))
      const button = screen.getByTestId(`workflow-start`) as HTMLButtonElement
      expect(button.disabled).toBe(false)
      expect(screen.queryByTestId(`workflow-start-blocker`)).toBeNull()
      view.unmount()
    }
    mount(startable())
    const start = screen.getByTestId(`workflow-start`) as HTMLButtonElement
    expect(start.disabled).toBe(false)
    fireEvent.click(start)
    await vi.waitFor(() =>
      expect(runMutates.start).toHaveBeenCalledWith(
        { id: `wf` },
        expect.anything()
      )
    )
  })

  it(`offers Pause and Cancel while running, and nothing destructive else`, async () => {
    reset()
    mount({ ...startable(), status: `running` })
    expect(screen.queryByTestId(`workflow-start`)).toBeNull()
    expect(screen.queryByTestId(`workflow-plan`)).toBeNull()
    expect(screen.queryByTestId(`workflow-delete`)).toBeNull()
    expect(screen.getByTestId(`workflow-pause`).textContent).toContain(
      PAUSE_WORKFLOW_LABEL
    )
    fireEvent.click(screen.getByTestId(`workflow-pause`))
    await vi.waitFor(() =>
      expect(runMutates.pause).toHaveBeenCalledWith(
        { id: `wf` },
        expect.anything()
      )
    )
    expect(screen.getByTestId(`workflow-cancel`).textContent).toContain(
      CANCEL_WORKFLOW_LABEL
    )
  })

  it(`resumes a paused workflow`, async () => {
    reset()
    mount({ ...startable(), status: `paused` })
    expect(screen.getByTestId(`workflow-resume`).textContent).toContain(
      RESUME_WORKFLOW_LABEL
    )
    fireEvent.click(screen.getByTestId(`workflow-resume`))
    await vi.waitFor(() =>
      expect(runMutates.resume).toHaveBeenCalledWith(
        { id: `wf` },
        expect.anything()
      )
    )
  })

  it(`cancels only after the confirm`, async () => {
    reset()
    mount({ ...startable(), status: `running` })
    fireEvent.click(screen.getByTestId(`workflow-cancel`))
    expect(
      screen.getByText(
        `Its live runs end and its branch is deleted. Nothing reached the default branch.`
      )
    ).toBeTruthy()
    expect(runMutates.cancel).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`workflow-cancel-confirm`))
    await vi.waitFor(() =>
      expect(runMutates.cancel).toHaveBeenCalledWith(
        { id: `wf` },
        expect.anything()
      )
    )
  })

  it(`leaves a finished workflow nothing but Delete`, () => {
    reset()
    mount({ ...startable(), status: `done` })
    expect(screen.queryByTestId(`workflow-start`)).toBeNull()
    expect(screen.queryByTestId(`workflow-pause`)).toBeNull()
    expect(screen.queryByTestId(`workflow-cancel`)).toBeNull()
    expect(screen.queryByTestId(`workflow-plan`)).toBeNull()
    expect(screen.getByTestId(`workflow-delete`)).toBeTruthy()
  })

  it(`freezes the configuration once the workflow left draft`, () => {
    reset()
    const rows = (): HTMLButtonElement[] => [
      ...screen
        .getByTestId(`workflow-how-it-runs`)
        .querySelectorAll<HTMLButtonElement>(`[data-slot="glass-picker-row"]`),
    ]
    const draft = mount(startable())
    expect(rows().length).toBeGreaterThan(0)
    expect(rows().every((row) => row.disabled)).toBe(false)
    draft.unmount()
    mount({ ...startable(), status: `running` })
    expect(rows().every((row) => row.disabled)).toBe(true)
  })

  // EXP-981 leftover: with no model picked the row used to render EMPTY for an
  // agent that cannot launch blank.
  it(`reads "Default" on the Model row when no model is picked`, () => {
    reset()
    mount()
    const model = [
      ...screen
        .getByTestId(`workflow-how-it-runs`)
        .querySelectorAll(`[data-slot="glass-picker-row"]`),
    ].find((row) => row.textContent?.startsWith(`Model`))
    expect(model?.textContent).toBe(`ModelDefault`)
  })
})

describe(`WorkflowDetail running graph`, () => {
  it(`paints an edge out of a landed node green`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, state: `landed` }),
      node(`n2`, { wave: 1, lane: 0, state: `running` }),
    ]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-n2`, `APP-2`)]
    graphState.relations = [
      { type: `blocks`, issueId: `i-n1`, relatedIssueId: `i-n2` },
    ]
    mount({ ...startable(), status: `running` })
    expect(screen.getAllByTestId(`workflow-graph-landed-edge`)).toHaveLength(1)
    expect(screen.queryByTestId(`workflow-graph-edge`)).toBeNull()
    expect(screen.getByTestId(`workflow-node-n2-caption`).textContent).toBe(
      `Running`
    )
  })

  // EXP-983: one edge per style, each of them its own test id and
  // `data-style`; only the speculative one is dashed.
  it(`draws every edge style, the speculative one dashed`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, state: `landed` }),
      node(`n2`, { wave: 1, lane: 0, state: `running` }),
      node(`n3`, { wave: 1, lane: 1, state: `blocked` }),
      node(`n4`, { wave: 2, lane: 0, state: `updating` }),
      node(`n5`, { wave: 2, lane: 1, state: `blocked` }),
    ]
    graphState.issues = [
      issue(`i-n1`, `APP-1`),
      issue(`i-n2`, `APP-2`),
      issue(`i-n3`, `APP-3`),
      issue(`i-n4`, `APP-4`),
      issue(`i-n5`, `APP-5`),
    ]
    graphState.relations = [
      // landed → running: the blocker is in.
      { type: `blocks`, issueId: `i-n1`, relatedIssueId: `i-n2` },
      // blocked → running: the dependent started before its blocker landed.
      { type: `blocks`, issueId: `i-n3`, relatedIssueId: `i-n2` },
      // running → updating: the upstream moved, the dependent merges it in.
      { type: `blocks`, issueId: `i-n2`, relatedIssueId: `i-n4` },
      // blocked → blocked: nothing to say yet.
      { type: `blocks`, issueId: `i-n3`, relatedIssueId: `i-n5` },
    ]
    mount({ ...startable(), status: `running` })
    const edge = (name: string) => screen.getByTestId(name)
    expect(edge(`workflow-graph-landed-edge`).getAttribute(`data-style`)).toBe(
      `landed`
    )
    expect(edge(`workflow-graph-stale-edge`).getAttribute(`data-style`)).toBe(
      `stale`
    )
    expect(edge(`workflow-graph-edge`).getAttribute(`data-style`)).toBe(`plain`)
    // Only the speculative edge is dashed.
    expect(
      edge(`workflow-graph-speculative-edge`).getAttribute(`stroke-dasharray`)
    ).toBeTruthy()
    for (const name of [
      `workflow-graph-edge`,
      `workflow-graph-landed-edge`,
      `workflow-graph-stale-edge`,
    ]) {
      expect(edge(name).getAttribute(`stroke-dasharray`)).toBeNull()
    }
  })

  it(`draws a serialization edge dashed beside the blocks edges`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, state: `in_review` }),
      node(`n2`, { wave: 0, lane: 1, state: `running`, afterNodeIds: [`n1`] }),
    ]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-n2`, `APP-2`)]
    // No `blocks` relation at all: the engine serialized two SIBLINGS.
    graphState.relations = []
    mount({ ...startable(), status: `running` })
    const edge = screen.getByTestId(`workflow-graph-speculative-edge`)
    expect(edge.getAttribute(`stroke-dasharray`)).toBeTruthy()
    expect(screen.queryByTestId(`workflow-graph-edge`)).toBeNull()
  })

  it(`keeps a plain edge plain on a draft`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0 }),
      node(`n2`, { wave: 1, lane: 0 }),
    ]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-n2`, `APP-2`)]
    graphState.relations = [
      { type: `blocks`, issueId: `i-n1`, relatedIssueId: `i-n2` },
    ]
    mount(startable())
    const edge = screen.getByTestId(`workflow-graph-edge`)
    expect(edge.getAttribute(`data-style`)).toBe(`plain`)
    expect(edge.getAttribute(`stroke-dasharray`)).toBeNull()
  })

  it(`lists the merge train in landing order`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, state: `in_review` }),
      node(`n2`, { wave: 1, lane: 0, state: `in_review` }),
      node(`n3`, { wave: 2, lane: 0, state: `in_review`, kind: `contract` }),
      node(`n4`, { wave: 3, lane: 0, state: `running` }),
    ]
    graphState.issues = [
      issue(`i-n1`, `APP-1`),
      issue(`i-n2`, `APP-2`),
      issue(`i-n3`, `APP-3`),
      issue(`i-n4`, `APP-4`),
    ]
    graphState.relations = []
    // `none` gates nothing but the contract node, which always waits.
    mount({ ...startable(), status: `running`, gate: `none` })
    expect(screen.getByTestId(`workflow-train-n1`).textContent).toBe(
      `APP-1Landing next`
    )
    expect(screen.getByTestId(`workflow-train-n2`).textContent).toBe(
      `APP-2Queued`
    )
    expect(screen.getByTestId(`workflow-train-n3`).textContent).toBe(
      `APP-3Needs approval`
    )
    // A node still coding is not up for landing.
    expect(screen.queryByTestId(`workflow-train-n4`)).toBeNull()
  })

  it(`says the train is empty, and hides it on a draft`, () => {
    nodeRows.rows = [node(`n1`, { state: `running` })]
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    graphState.relations = []
    const running = mount({ ...startable(), status: `running` })
    expect(screen.getByTestId(`workflow-merge-train`).textContent).toContain(
      MERGE_TRAIN_EMPTY
    )
    running.unmount()
    mount(startable())
    expect(screen.queryByTestId(`workflow-merge-train`)).toBeNull()
  })

  it(`closes the graph with the final pull request once everything is in`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, state: `landed` }),
      node(`n2`, { wave: 1, lane: 0, state: `skipped` }),
    ]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-n2`, `APP-2`)]
    graphState.relations = []
    const opening = mount({ ...startable(), status: `running` })
    expect(screen.getByTestId(`workflow-final-pr`).textContent).toBe(
      `Final pull requestOpening the pull request`
    )
    opening.unmount()
    mount({
      ...startable(),
      status: `running`,
      finalPrNumber: 42,
      finalPrState: `open`,
      finalPrUrl: `https://github.com/acme/app/pull/42`,
    })
    const card = screen.getByTestId(`workflow-final-pr`)
    expect(card.textContent).toBe(`Final pull request#42 · Open`)
    expect(card.getAttribute(`href`)).toBe(
      `https://github.com/acme/app/pull/42`
    )
    // It sits one wave past the last node, in lane 0.
    const box = screen.getByTestId(`workflow-node-final-pr`)
    expect([box.getAttribute(`data-wave`), box.getAttribute(`data-lane`)]).toEqual(
      [`2`, `0`]
    )
  })

  it(`draws no final-PR node while nodes are still out`, () => {
    nodeRows.rows = [
      node(`n1`, { state: `landed` }),
      node(`n2`, { wave: 1, state: `running` }),
    ]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-n2`, `APP-2`)]
    graphState.relations = []
    mount({ ...startable(), status: `running` })
    expect(screen.queryByTestId(`workflow-final-pr`)).toBeNull()
  })
})

describe(`WorkflowDetail node panel actions`, () => {
  const open = (
    over: Partial<WorkflowNode>,
    workflowOver: Partial<SyncedWorkflow> = {}
  ) => {
    for (const mutate of Object.values(runMutates)) mutate.mockClear()
    nodeRows.rows = [node(`n1`, over)]
    graphState.relations = []
    const view = mount({ ...startable(), status: `running`, ...workflowOver })
    fireEvent.click(screen.getByTestId(`workflow-node-n1-card`))
    return view
  }

  it(`approves a gated node's PR and takes the approval back`, async () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    const gated = open({ state: `in_review` })
    fireEvent.click(screen.getByTestId(`workflow-node-approve`))
    expect(screen.getByTestId(`workflow-node-approve`).textContent).toBe(
      APPROVE_NODE_LABEL
    )
    await vi.waitFor(() =>
      expect(runMutates.approveNode).toHaveBeenCalledWith(
        { nodeId: `n1`, approved: true },
        expect.anything()
      )
    )
    expect(screen.queryByTestId(`workflow-node-withdraw`)).toBeNull()
    gated.unmount()

    open({ state: `in_review`, approvedAt: new Date() })
    expect(screen.queryByTestId(`workflow-node-approve`)).toBeNull()
    expect(screen.getByTestId(`workflow-node-withdraw`).textContent).toBe(
      WITHDRAW_APPROVAL_LABEL
    )
    fireEvent.click(screen.getByTestId(`workflow-node-withdraw`))
    await vi.waitFor(() =>
      expect(runMutates.approveNode).toHaveBeenCalledWith(
        { nodeId: `n1`, approved: false },
        expect.anything()
      )
    )
  })

  it(`asks nobody to approve an ungated leaf`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    open({ state: `in_review` }, { gate: `none` })
    expect(screen.queryByTestId(`workflow-node-approve`)).toBeNull()
  })

  it(`retries a failed node, and skips it only after the confirm`, async () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    const failed = open({ state: `failed`, note: `The tests never went green.` })
    expect(screen.getByTestId(`workflow-node-note`).textContent).toBe(
      `The tests never went green.`
    )
    expect(screen.getByTestId(`workflow-node-retry`).textContent).toBe(
      RETRY_NODE_LABEL
    )
    fireEvent.click(screen.getByTestId(`workflow-node-retry`))
    await vi.waitFor(() =>
      expect(runMutates.resolveNode).toHaveBeenCalledWith(
        { nodeId: `n1`, action: `retry` },
        expect.anything()
      )
    )
    failed.unmount()

    open({ state: `failed` })
    fireEvent.click(screen.getByTestId(`workflow-node-skip`))
    expect(screen.getByText(SKIP_NODE_CONFIRM)).toBeTruthy()
    expect(runMutates.resolveNode).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`workflow-node-skip-confirm`))
    await vi.waitFor(() =>
      expect(runMutates.resolveNode).toHaveBeenCalledWith(
        { nodeId: `n1`, action: `skip` },
        expect.anything()
      )
    )
    expect(screen.getByTestId(`workflow-node-skip`).textContent).toBe(
      SKIP_NODE_LABEL
    )
  })

  it(`links to the node's run and to its pull request`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`, { prNumber: 7, prState: `open` })]
    open({ state: `in_review`, sessionId: `s-1` })
    expect(screen.getByTestId(`workflow-node-run`).textContent).toBe(`Open run`)
    expect(screen.getByTestId(`workflow-node-pr`).textContent).toBe(`PR #7`)
  })

  // EXP-983: what a speculative start adds to the panel.
  it(`says when the node published its contract`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    const published = open({
      state: `running`,
      checkpointAt: new Date(Date.now() - 5 * 60_000),
    })
    const line = screen.getByTestId(`workflow-node-checkpoint`).textContent
    expect(line).toContain(CONTRACT_PUBLISHED_LABEL)
    expect(line).toContain(`5 minutes ago`)
    published.unmount()

    open({ state: `running` })
    expect(screen.queryByTestId(`workflow-node-checkpoint`)).toBeNull()
  })

  it(`lists the nodes a serialized node merges in first`, () => {
    for (const mutate of Object.values(runMutates)) mutate.mockClear()
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, state: `running`, afterNodeIds: [`n2`] }),
      node(`n2`, {
        wave: 0,
        lane: 1,
        state: `in_review`,
        memberIssueIds: [`i-m`],
      }),
    ]
    graphState.issues = [
      issue(`i-n1`, `APP-1`),
      issue(`i-n2`, `APP-2`),
      issue(`i-m`, `APP-9`),
    ]
    graphState.relations = []
    const serialized = mount({ ...startable(), status: `running` })
    fireEvent.click(screen.getByTestId(`workflow-node-n1-card`))
    const line = screen.getByTestId(`workflow-node-merges-first`)
    expect(line.textContent).toContain(`Merges in first`)
    // A compound node is named the way it is everywhere else.
    expect(line.textContent).toContain(`APP-2 +1`)
    serialized.unmount()

    // The other node merges nothing in, so it has no line at all.
    nodeRows.rows = [node(`n1`, { state: `running` })]
    mount({ ...startable(), status: `running` })
    fireEvent.click(screen.getByTestId(`workflow-node-n1-card`))
    expect(screen.queryByTestId(`workflow-node-merges-first`)).toBeNull()
  })

  it(`offers no run or PR link on a node that has neither`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    open({ state: `ready` })
    expect(screen.queryByTestId(`workflow-node-run`)).toBeNull()
    expect(screen.queryByTestId(`workflow-node-pr`)).toBeNull()
    expect(screen.queryByTestId(`workflow-node-note`)).toBeNull()
  })
})
