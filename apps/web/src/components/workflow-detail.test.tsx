import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue, SyncedWorkflow, WorkflowNode } from "@/db/schema"
import { BUILTIN_PLAN_WORKFLOW_ID } from "@/lib/builtin-actions"
import { DELETE_WORKFLOW_LABEL, PLAN_WORKFLOW_LABEL } from "@/lib/workflow-view"

// EXP-981: the workflow detail — the graph positioned by the SERVER's
// wave/lane, a compound node drawn as a stacked card, the cycle note, and the
// two actions. The layout is never computed here, so the assertions read the
// grid's `data-wave`/`data-lane` straight back.

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
    },
  },
}))

import { WorkflowDetail } from "@/components/workflow-detail"

const issue = (id: string, identifier: string): Issue =>
  ({
    id,
    identifier,
    title: `Issue ${identifier}`,
    status: `backlog`,
    priority: `none`,
    boardId: `b1`,
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
    launch: {},
    metrics: { nodes: 2, edges: 1, depth: 2, width: 1, cycles: [] },
    ...over,
  }) as unknown as SyncedWorkflow

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

  // P2 ships DRAFT workflows: the engine, and with it a Start button, is next.
  it(`has no Start control`, () => {
    nodeRows.rows = []
    mount()
    // "Start" exists only as the start-RULE picker row in How it runs.
    expect(
      screen
        .getAllByRole(`button`)
        .map((button) => button.textContent?.trim())
        .filter((text) => text === `Start` || text === `Start workflow`)
    ).toEqual([])
  })
})
