import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue, SyncedWorkflow, WorkflowNode } from "@/db/schema"
import { BUILTIN_PLAN_WORKFLOW_ID } from "@/lib/builtin-actions"
import {
  ADMIT_NODE_LABEL,
  AGENT_REVIEW_TITLE,
  APPROVE_NODE_LABEL,
  CANCEL_WORKFLOW_LABEL,
  CONTRACT_PUBLISHED_LABEL,
  DELETE_WORKFLOW_LABEL,
  DISMISS_NODE_LABEL,
  MERGE_FINAL_PR_CONFIRM,
  MERGE_FINAL_PR_LABEL,
  MERGE_TRAIN_EMPTY,
  METRICS_TITLE,
  NODE_MODEL_LABEL,
  NODE_UNSYNCED_TITLE,
  PAUSE_WORKFLOW_LABEL,
  PLAN_WORKFLOW_LABEL,
  PROPOSED_NODE_NOTE,
  RESUME_WORKFLOW_LABEL,
  RETRY_NODE_LABEL,
  SKIP_NODE_CONFIRM,
  SKIP_NODE_LABEL,
  RUNNING_NOW_LABEL,
  START_WORKFLOW_LABEL,
  WITHDRAW_APPROVAL_LABEL,
} from "@/lib/workflow-view"
import {
  CHANGES_FACE_LABEL,
  ISSUE_FACE_LABEL,
  RUN_FACE_LABEL,
} from "@/lib/work-faces"

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
//
// EXP-984: the reviewer's verdict reads off the node, a `proposed` follow-up is
// decided on rather than run, and the workflow
// carries its counters.

// Radix positions its popovers with ResizeObserver and cmdk scrolls the active
// row into view; jsdom has neither.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const nodeRows = vi.hoisted(() => ({ rows: [] as unknown[] }))
const nodeRuns = vi.hoisted(() => ({ byNodeId: new Map<string, unknown>() }))
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
  admitNode: vi.fn().mockResolvedValue({ txId: 1 }),
  updateNode: vi.fn().mockResolvedValue({ txId: 1 }),
  mergeFinalPr: vi.fn().mockResolvedValue({ merged: true }),
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
  useWorkflowNodeRuns: () => nodeRuns.byNodeId,
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
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => [{ id: `b1`, slug: `app` }],
}))
vi.mock(`@/components/issue-chip`, () => ({
  IssueChip: ({
    issue,
    testId,
    link,
  }: {
    issue: { identifier: string }
    testId?: string
    link?: unknown
  }) => (
    <span
      data-testid={testId ?? `chip-${issue.identifier}`}
      data-linked={link ? `true` : undefined}
    >
      {issue.identifier}
    </span>
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
      updateNode: { mutate: runMutates.updateNode },
      admitNode: { mutate: runMutates.admitNode },
      start: { mutate: runMutates.start },
      pause: { mutate: runMutates.pause },
      resume: { mutate: runMutates.resume },
      cancel: { mutate: runMutates.cancel },
      approveNode: { mutate: runMutates.approveNode },
      resolveNode: { mutate: runMutates.resolveNode },
      mergeFinalPr: { mutate: runMutates.mergeFinalPr },
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
    reviewRound: 0,
    review: null,
    ...over,
  }) as unknown as WorkflowNode

const workflow = (over: Partial<SyncedWorkflow> = {}): SyncedWorkflow =>
  ({
    id: `wf`,
    teamId: `t1`,
    name: `APP-1 +2`,
    status: `draft`,
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

/** EXP-1014: the chip's ONE glyph slot — always drawn, empty when nothing
 *  resolves into it (an unsynced issue). */
function glyphSlot(id: string): HTMLElement | null {
  return screen.queryByTestId(`workflow-node-${id}-glyph`)
}

/** The glass picker row a `Combobox triggerVariant="row"` draws, by its
 *  leading label. */
function pickerRow(label: string): HTMLElement {
  const row = Array.from(
    document.querySelectorAll(`[data-slot=glass-picker-row]`)
  ).find((element) => element.textContent?.startsWith(label))
  if (!row) throw new Error(`no picker row for ${label}`)
  return row as HTMLElement
}

const commandRows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

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
    // EXP-1033: a node is the app's ISSUE CHIP — mono identifier, the issue's
    // title beside it — and a DRAFT says nothing else: no "Contract", no
    // "Leaf · high risk" sub-subtitle under the box.
    expect(screen.getByTestId(`workflow-node-n1-title`).textContent).toBe(`APP-1`)
    expect(screen.getByTestId(`workflow-node-n1-name`).textContent).toBe(
      `Issue APP-1`
    )
    expect(screen.getByTestId(`workflow-node-n1-caption`).textContent).toBe(``)
    expect(screen.getByTestId(`workflow-node-n3-caption`).textContent).toBe(``)
    expect(screen.getByTestId(`workflow-detail`).textContent).not.toContain(
      `high risk`
    )
  })

  // EXP-1033: the graph FILLS its column — it is scaled down to fit and never
  // hides half of itself behind an inner scrollbar.
  it(`scrolls nowhere: the graph has no inner scroller`, () => {
    nodeRows.rows = [node(`n1`), node(`n2`, { wave: 1 })]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-n2`, `APP-2`)]
    graphState.relations = []
    mount()
    const graph = screen.getByTestId(`workflow-graph`)
    expect(graph.className).not.toContain(`overflow-auto`)
    expect(graph.className).not.toContain(`overflow-x`)
  })

  it(`draws a compound node as a stacked chip titled with its members`, () => {
    nodeRows.rows = [node(`n1`, { memberIssueIds: [`i-a`, `i-b`, `i-c`] })]
    graphState.issues = [issue(`i-n1`, `APP-14`)]
    graphState.relations = []
    mount()
    expect(screen.getByTestId(`workflow-node-n1-stack`)).toBeTruthy()
    expect(screen.getByTestId(`workflow-node-n1-title`).textContent).toBe(
      `APP-14 +3`
    )
  })

  // EXP-1014: an unsynced node reads the same ×4 — the first 8 characters of
  // the issue id in the mono slot (`+n` for a compound one), the ONE line that
  // says why there is no title, and an EMPTY glyph slot.
  it(`keeps a node whose issue has not synced, caption and all`, () => {
    nodeRows.rows = [
      node(`n1`, { state: `ready`, issueId: `abcd1234-5e6f-4a7b-8c9d-0e1f2a3b4c5d` }),
    ]
    graphState.issues = []
    graphState.relations = []
    mount({ status: `running` })
    expect(screen.getByTestId(`workflow-node-n1`)).toBeTruthy()
    expect(screen.getByTestId(`workflow-node-n1-caption`).textContent).toBe(
      `Ready`
    )
    expect(screen.getByTestId(`workflow-node-n1-title`).textContent).toBe(
      `abcd1234`
    )
    expect(screen.getByTestId(`workflow-node-n1-name`).textContent).toBe(
      NODE_UNSYNCED_TITLE
    )
    // The slot is still drawn, and it is EMPTY: nothing resolves a status for
    // an issue that has not arrived.
    expect(glyphSlot(`n1`)!.childElementCount).toBe(0)
  })

  it(`titles an unsynced COMPOUND node with its member count`, () => {
    nodeRows.rows = [
      node(`n1`, {
        issueId: `abcd1234-5e6f-4a7b-8c9d-0e1f2a3b4c5d`,
        memberIssueIds: [`i-a`, `i-b`],
      }),
    ]
    graphState.issues = []
    graphState.relations = []
    mount()
    expect(screen.getByTestId(`workflow-node-n1-title`).textContent).toBe(
      `abcd1234 +2`
    )
  })

  // EXP-1014, the ONE glyph rule ×4: a live run shows the live dot, a started
  // workflow's state shows its own glyph WHEN it has one, and everything else
  // — a draft, or blocked / ready / proposed / skipped — shows the ISSUE's own
  // status glyph, so an unstarted node reads like the same issue anywhere else.
  it(`falls back to the issue's status glyph wherever the state has none`, () => {
    nodeRows.rows = [
      node(`n1`, { wave: 0, state: `ready` }),
      node(`n2`, { wave: 1, state: `landed` }),
    ]
    graphState.issues = [
      issue(`i-n1`, `APP-1`, { status: `in_progress` }),
      issue(`i-n2`, `APP-2`, { status: `in_progress` }),
    ]
    graphState.relations = []

    // A draft has no states at all: both chips read as their issue.
    const draft = mount()
    for (const id of [`n1`, `n2`]) {
      expect(glyphSlot(id)!.querySelector(`svg`)!.getAttribute(`class`)).toContain(
        `text-yellow-500`
      )
    }
    draft.unmount()

    // Started: `landed` has a glyph of its own — painted in the node's tone —
    // while `ready` still has none and stays the issue's status.
    mount({ status: `running` })
    expect(glyphSlot(`n1`)!.querySelector(`svg`)!.getAttribute(`class`)).toContain(
      `text-yellow-500`
    )
    expect(
      glyphSlot(`n2`)!.querySelector(`svg`)!.getAttribute(`class`)
    ).not.toContain(`text-yellow-500`)
    expect(glyphSlot(`n2`)!.className).toContain(`text-emerald-500`)
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
    // EXP-1024: the planner's `touches` globs are bookkeeping, not a panel row.
    expect(screen.queryByTestId(`workflow-node-touches`)).toBeNull()
    expect(panel.textContent).not.toContain(`apps/web/**`)
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

  // EXP-1033: the settings panel is GONE. The screen configures nothing —
  // every model is derived, the gate and the start rule are fixed — so the
  // only pick left is the runner device, up in the header row and frozen the
  // moment the workflow leaves draft.
  it(`has no settings block at all, and freezes the runner device after draft`, () => {
    reset()
    const draft = mount(startable())
    expect(screen.queryByTestId(`workflow-how-it-runs`)).toBeNull()
    for (const gone of [
      `Contract model`,
      `Integration model`,
      `High-risk model`,
      `Review model`,
      `Subagent model`,
      `Max parallel`,
      `Same as Model`,
    ]) {
      expect(screen.getByTestId(`workflow-detail`).textContent).not.toContain(gone)
    }
    const device = () =>
      screen.getByTestId(`workflow-device`).querySelector<HTMLButtonElement>(
        `button`
      )
    expect(device()?.disabled).toBe(false)
    draft.unmount()

    mount({ ...startable(), status: `running` })
    expect(device()?.disabled).toBe(true)
  })

  // `workflows.update` may still be called from here with a name and a device
  // — never with a launch or a start rule.
  it(`writes nothing but the name and the runner device`, async () => {
    reset()
    updateMutate.mockClear()
    mount(startable())
    const name = screen.getByTestId(`workflow-name`) as HTMLInputElement
    fireEvent.focus(name)
    fireEvent.change(name, { target: { value: `Renamed` } })
    fireEvent.blur(name)
    await vi.waitFor(() =>
      expect(updateMutate).toHaveBeenCalledWith(
        { id: `wf`, name: `Renamed` },
        expect.anything()
      )
    )
    for (const [input] of updateMutate.mock.calls) {
      expect(Object.keys(input as object).sort()).not.toContain(`launch`)
      expect(Object.keys(input as object).sort()).not.toContain(`startOn`)
    }
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
      node(`n1`, { wave: 0, lane: 0, state: `in_review`, approvedAt: new Date(`2026-09-19T00:00:00Z`) }),
      node(`n2`, { wave: 1, lane: 0, state: `in_review`, approvedAt: new Date(`2026-09-19T00:00:00Z`) }),
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
    // EXP-1010: a node no review approved yet waits, whatever its kind.
    mount({ ...startable(), status: `running` })
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
    // EXP-1033: the same chip as every node, plus the run's ONE human review.
    expect(card.textContent).toBe(
      `Final pull request#42 · Open${MERGE_FINAL_PR_LABEL}`
    )
    expect(
      screen.getByTestId(`workflow-final-pr-link`).getAttribute(`href`)
    ).toBe(`https://github.com/acme/app/pull/42`)
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

  it(`asks nobody to approve a node a review already cleared`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    open({ state: `in_review`, approvedAt: new Date(`2026-09-19T00:00:00Z`) })
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

  // EXP-1002: the node's surfaces are the app's own faces, in `availableFaces`
  // order, and only the ones this node HAS. EXP-1024: drawn by the work
  // header's `WorkFaceToggle` itself, with no face selected.
  it(`offers Issue, Run and Changes as the work header's face toggle`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`, { prNumber: 7, prState: `open` })]
    const both = open({ state: `in_review`, sessionId: `s-1` })
    const strip = screen.getByTestId(`workflow-node-faces`)
    const toggle = strip.querySelector(`[data-testid="work-face-toggle"]`)
    expect(toggle).not.toBeNull()
    expect(toggle!.textContent).toBe(
      `${ISSUE_FACE_LABEL}${RUN_FACE_LABEL}${CHANGES_FACE_LABEL}`
    )
    // The reader is on the graph, not on a face: every segment is inactive.
    const segments = toggle!.querySelectorAll(`[data-slot="tabs-trigger"]`)
    expect(segments).toHaveLength(3)
    for (const segment of segments) {
      expect(segment.getAttribute(`data-state`)).toBe(`inactive`)
    }
    // Changes opens the issue's review page (and closes the panel, so the
    // segment is held across the two events).
    const changes = screen.getByText(CHANGES_FACE_LABEL)
    fireEvent.mouseDown(changes)
    fireEvent.click(changes)
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/reviews/$issueIdentifier`,
      params: { teamSlug: `acme`, issueIdentifier: `APP-1` },
    })
    both.unmount()

    // No run and no pull request: the chip is the only way out, and a
    // one-segment toggle is not drawn — the same rule as the work header.
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    open({ state: `blocked` })
    expect(screen.queryByTestId(`workflow-node-faces`)).toBeNull()
  })

  it(`marks a node whose run is up and lists it one tap from its session`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    nodeRuns.byNodeId = new Map([
      [`n1`, { sessionId: `s-1`, live: true, state: `running`, working: true }],
    ])
    try {
      open({ state: `running`, sessionId: `s-1` })
      expect(
        screen.getByTestId(`workflow-node-n1-card`).getAttribute(`data-running`)
      ).toBe(`true`)
      const strip = screen.getByTestId(`workflow-running-strip`)
      expect(strip.textContent).toContain(RUNNING_NOW_LABEL)
      expect(screen.getByTestId(`workflow-running-n1`).textContent).toBe(`APP-1`)
      // The panel's badge is the way into the issue; no separate button.
      expect(
        screen.getByTestId(`workflow-node-issue`).getAttribute(`data-linked`)
      ).toBe(`true`)
      expect(screen.queryByText(`Open issue`)).toBeNull()
    } finally {
      nodeRuns.byNodeId = new Map()
    }
  })

  it(`draws no running strip while nothing runs`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    open({ state: `blocked` })
    expect(screen.queryByTestId(`workflow-running-strip`)).toBeNull()
    expect(
      screen.getByTestId(`workflow-node-n1-card`).getAttribute(`data-running`)
    ).toBeNull()
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

  // EXP-984 — the agent reviewer's latest verdict.
  it(`reads an approval in the success tone, with its findings and check`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    open({
      state: `in_review`,
      review: {
        verdict: `approve`,
        findings: `The contract is honoured and the edges are covered.`,
        oracle: { command: `bun run test:contract`, passed: true },
        model: `opus`,
        round: 1,
        at: new Date().toISOString(),
      },
    })
    const block = screen.getByTestId(`workflow-node-review`)
    expect(block.textContent).toContain(AGENT_REVIEW_TITLE)
    const line = screen.getByTestId(`workflow-node-review-line`)
    expect(line.textContent).toBe(`Approved · round 1 · checks passed`)
    expect(line.className).toContain(`text-emerald-500`)
    expect(
      screen.getByTestId(`workflow-node-review-findings`).textContent
    ).toBe(`The contract is honoured and the edges are covered.`)
    expect(screen.getByTestId(`workflow-node-review-oracle`).textContent).toBe(
      `bun run test:contract`
    )
    // Short findings need no fold.
    expect(screen.queryByTestId(`workflow-node-review-more`)).toBeNull()
  })

  it(`reads requested changes in the destructive tone, and folds long findings`, () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    const findings = `line\n`.repeat(20)
    const changes = open({
      state: `updating`,
      reviewRound: 2,
      review: {
        verdict: `request_changes`,
        findings,
        oracle: null,
        model: `opus`,
        round: 2,
        at: new Date().toISOString(),
      },
    })
    const line = screen.getByTestId(`workflow-node-review-line`)
    expect(line.textContent).toBe(`Changes requested · round 2`)
    expect(line.className).toContain(`text-destructive`)
    // No oracle ran, so there is no command to show.
    expect(screen.queryByTestId(`workflow-node-review-oracle`)).toBeNull()
    const body = screen.getByTestId(`workflow-node-review-findings`)
    expect(body.className).toContain(`line-clamp-4`)
    const more = screen.getByTestId(`workflow-node-review-more`)
    expect(more.textContent).toBe(`Show more`)
    fireEvent.click(more)
    expect(
      screen.getByTestId(`workflow-node-review-findings`).className
    ).not.toContain(`line-clamp-4`)
    expect(screen.getByTestId(`workflow-node-review-more`).textContent).toBe(
      `Show less`
    )
    changes.unmount()

    // A node nobody reviewed carries no block at all.
    open({ state: `in_review` })
    expect(screen.queryByTestId(`workflow-node-review`)).toBeNull()
  })

  // EXP-984 — a follow-up filed mid-run is decided on, not run.
  it(`admits a proposed node, dismisses it, and offers nothing else`, async () => {
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    const admitted = open({
      state: `proposed`,
      note: `Filed by APP-1's run: the migration needs a backfill.`,
    })
    // Dashed while it is only a proposal.
    expect(
      screen.getByTestId(`workflow-node-n1-card`).getAttribute(`data-proposed`)
    ).toBe(`true`)
    expect(
      screen.getByTestId(`workflow-node-proposed-note`).textContent
    ).toBe(PROPOSED_NODE_NOTE)
    // The engine's own reason still reads above it.
    expect(screen.getByTestId(`workflow-node-note`).textContent).toContain(
      `needs a backfill`
    )
    // Nothing about it is approved, retried or skipped yet.
    expect(screen.queryByTestId(`workflow-node-approve`)).toBeNull()
    expect(screen.queryByTestId(`workflow-node-retry`)).toBeNull()
    expect(screen.queryByTestId(`workflow-node-skip`)).toBeNull()
    expect(screen.getByTestId(`workflow-node-admit`).textContent).toBe(
      ADMIT_NODE_LABEL
    )
    fireEvent.click(screen.getByTestId(`workflow-node-admit`))
    await vi.waitFor(() =>
      expect(runMutates.admitNode).toHaveBeenCalledWith(
        { nodeId: `n1`, admit: true },
        expect.anything()
      )
    )
    admitted.unmount()

    open({ state: `proposed` })
    expect(screen.getByTestId(`workflow-node-dismiss`).textContent).toBe(
      DISMISS_NODE_LABEL
    )
    fireEvent.click(screen.getByTestId(`workflow-node-dismiss`))
    await vi.waitFor(() =>
      expect(runMutates.admitNode).toHaveBeenCalledWith(
        { nodeId: `n1`, admit: false },
        expect.anything()
      )
    )
  })

  it(`keeps a proposed node out of the merge train`, () => {
    for (const mutate of Object.values(runMutates)) mutate.mockClear()
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, state: `proposed` }),
      node(`n2`, { wave: 1, lane: 0, state: `in_review`, approvedAt: new Date(`2026-09-19T00:00:00Z`) }),
    ]
    graphState.issues = [issue(`i-n1`, `APP-1`), issue(`i-n2`, `APP-2`)]
    graphState.relations = []
    const queued = mount({ ...startable(), status: `running` })
    expect(screen.queryByTestId(`workflow-train-n1`)).toBeNull()
    expect(screen.getByTestId(`workflow-train-n2`).textContent).toBe(
      `APP-2Landing next`
    )
    queued.unmount()

    // Nor does it hold the final pull request back: every node that IS part of
    // the run is in.
    nodeRows.rows = [
      node(`n1`, { wave: 0, lane: 0, state: `proposed` }),
      node(`n2`, { wave: 1, lane: 0, state: `landed` }),
    ]
    mount({ ...startable(), status: `running` })
    expect(screen.getByTestId(`workflow-final-pr`).textContent).toBe(
      `Final pull requestOpening the pull request`
    )
  })

})

// EXP-1033: the node panel's ONE model line, the final PR's merge, and the
// counters the run accumulated.
describe(`WorkflowDetail node model, final merge and metrics`, () => {
  // EXP-1033: nobody pins a model on this screen any more. The panel READS
  // which of the launch's two models this node's run spawns on
  // (`modelForNode`): the cheap one for a leaf, the strong one for a contract,
  // an integration or any high-risk node.
  it(`reads the node's model off the launch, and pins nothing`, () => {
    for (const mutate of Object.values(runMutates)) mutate.mockClear()
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    graphState.relations = []
    const launch = { agent: `claude`, model: `sonnet`, strongModel: `opus` }

    nodeRows.rows = [node(`n1`, { kind: `leaf`, risk: `low` })]
    const leaf = mount({ ...startable(), status: `running`, launch })
    fireEvent.click(screen.getByTestId(`workflow-node-n1-card`))
    expect(
      screen.getByTestId(`workflow-node-row-${NODE_MODEL_LABEL}`).textContent
    ).toBe(`${NODE_MODEL_LABEL}Sonnet`)
    // Kind is a plain reading once the run owns the plan; RISK stays a PICK —
    // the server takes it at any status and it is the one lever onto the
    // strong model (EXP-1029).
    expect(screen.getByTestId(`workflow-node-row-Kind`).textContent).toBe(
      `KindLeaf`
    )
    expect(screen.queryByTestId(`workflow-node-row-Risk`)).toBeNull()
    expect(pickerRow(`Risk`).textContent).toContain(`Low`)
    leaf.unmount()

    nodeRows.rows = [node(`n1`, { kind: `leaf`, risk: `high` })]
    mount({ ...startable(), status: `running`, launch })
    fireEvent.click(screen.getByTestId(`workflow-node-n1-card`))
    expect(
      screen.getByTestId(`workflow-node-row-${NODE_MODEL_LABEL}`).textContent
    ).toBe(`${NODE_MODEL_LABEL}Opus`)
  })

  // EXP-1029: risk is the ONE lever that moves a node onto the strong model,
  // and `updateNode` asserts a draft for `kind`/`touches` ONLY — so the Risk
  // pick stays live for the whole run, not just while the plan is a draft.
  it(`re-picks a node's risk on a RUNNING workflow`, async () => {
    for (const mutate of Object.values(runMutates)) mutate.mockClear()
    nodeRows.rows = [node(`n1`, { kind: `leaf`, risk: `low` })]
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    graphState.relations = []
    mount({ ...startable(), status: `running` })
    fireEvent.click(screen.getByTestId(`workflow-node-n1-card`))

    fireEvent.click(pickerRow(`Risk`))
    const high = commandRows().find((row) => row.textContent?.includes(`High`))
    fireEvent.click(high!)
    await vi.waitFor(() =>
      expect(runMutates.updateNode).toHaveBeenCalledWith(
        { workflowId: `wf`, issueId: `i-n1`, risk: `high` },
        expect.anything()
      )
    )
  })

  // EXP-1033: the one human review of the whole run, from the chip that IS
  // the pull request — and only after the confirm.
  it(`merges the final pull request after the confirm`, async () => {
    for (const mutate of Object.values(runMutates)) mutate.mockClear()
    nodeRows.rows = [node(`n1`, { state: `landed` })]
    graphState.issues = [issue(`i-n1`, `APP-1`)]
    graphState.relations = []
    const open = mount({
      ...startable(),
      status: `running`,
      finalPrNumber: 42,
      finalPrState: `open`,
      finalPrUrl: `https://github.com/acme/app/pull/42`,
    })
    fireEvent.click(screen.getByTestId(`workflow-final-pr-merge`))
    expect(screen.getByText(MERGE_FINAL_PR_CONFIRM)).toBeTruthy()
    expect(runMutates.mergeFinalPr).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`workflow-final-pr-merge-confirm`))
    await vi.waitFor(() =>
      expect(runMutates.mergeFinalPr).toHaveBeenCalledWith(
        { id: `wf` },
        expect.anything()
      )
    )
    open.unmount()

    // A merged (or not yet opened) pull request offers no button at all.
    mount({
      ...startable(),
      status: `running`,
      finalPrNumber: 42,
      finalPrState: `merged`,
      finalPrUrl: `https://github.com/acme/app/pull/42`,
    })
    expect(screen.getByTestId(`workflow-final-pr`).textContent).toBe(
      `Final pull request#42 · Merged`
    )
    expect(screen.queryByTestId(`workflow-final-pr-merge`)).toBeNull()
  })

  it(`counts the run's metrics, and none of them on a draft`, () => {
    nodeRows.rows = []
    graphState.issues = []
    graphState.relations = []
    const metrics = {
      nodes: 4,
      edges: 3,
      depth: 2,
      width: 2,
      cycles: [],
      landed: 2,
      reviewRounds: 5,
      defectsByOracle: 3,
      defectsByAgentReview: 1,
    }
    const draft = mount(startable({ metrics }))
    expect(screen.queryByTestId(`workflow-metrics`)).toBeNull()
    draft.unmount()

    mount({ ...startable(), status: `running`, metrics })
    expect(screen.getByTestId(`workflow-metrics`).textContent).toContain(
      METRICS_TITLE
    )
    const row = (label: string) =>
      screen.getByTestId(`workflow-metric-${label}`).textContent
    expect(row(`Critical path`)).toBe(`Critical path2 waves for 4 nodes`)
    expect(row(`Landed`)).toBe(`Landed2`)
    expect(row(`Review rounds`)).toBe(`Review rounds5`)
    expect(row(`Defects found`)).toBe(
      `Defects found3 by checks · 1 by agent review`
    )
    // A counter with nothing to say draws no row.
    expect(screen.queryByTestId(`workflow-metric-Escalations`)).toBeNull()
  })
})
