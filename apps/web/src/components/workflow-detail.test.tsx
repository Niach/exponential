import { act, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue, SyncedWorkflow, WorkflowNode } from "@/db/schema"
import {
  ALL_SELECTION,
  pruneSelection,
  selectNode,
  stepSelection,
  stripStepKey,
  workflowBody,
} from "@/components/workflow-detail-selection"
import {
  DELETE_WORKFLOW_LABEL,
  PICK_DEVICE_LABEL,
  PLAN_WORKFLOW_LABEL,
  REVIEW_FINAL_PR_LABEL,
  RUNS_ON_LABEL,
  STOP_WORKFLOW_LABEL,
  PAUSE_WORKFLOW_LABEL,
  RESUME_WORKFLOW_LABEL,
  RETRY_NODE_LABEL,
  SKIP_NODE_LABEL,
  START_WORKFLOW_LABEL,
} from "@/lib/workflow-view"
import { CHANGES_FACE_LABEL, RUNS_FACE_LABEL } from "@/lib/work-faces"

// EXP-1084: the workflow page — the node strip as the picker (All + the nodes
// in DAG order; click, cmd-click, shift-click; ←/→ and j/k keep the face), the
// body = selection × face, the ONE primary action per status, the open-question
// banner only while a question is open, and Retry / Skip only on a failed chip.

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const state = vi.hoisted(() => ({
  nodes: [] as unknown[],
  issues: [] as unknown[],
  sessions: [] as unknown[],
  questions: [] as unknown[],
  devices: [] as unknown[],
}))
const navigate = vi.hoisted(() => vi.fn())
const openComposer = vi.hoisted(() => vi.fn())
const mutates = vi.hoisted(() => ({
  update: vi.fn().mockResolvedValue({ txId: 1 }),
  delete: vi.fn().mockResolvedValue({ txId: 1 }),
  start: vi.fn().mockResolvedValue({ txId: 1 }),
  pause: vi.fn().mockResolvedValue({ txId: 1 }),
  resume: vi.fn().mockResolvedValue({ txId: 1 }),
  cancel: vi.fn().mockResolvedValue({ txId: 1 }),
  resolveNode: vi.fn().mockResolvedValue({ txId: 1 }),
  admitNode: vi.fn().mockResolvedValue({ txId: 1 }),
  mergeFinalPr: vi.fn().mockResolvedValue({ merged: true }),
}))

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({ children, ...rest }: { children: React.ReactNode }) => (
    <a {...rest}>{children}</a>
  ),
  useNavigate: () => navigate,
}))
// A live query answers by its alias: `s` = the workflow's coding sessions.
vi.mock(`@tanstack/react-db`, () => ({
  eq: () => true,
  useLiveQuery: (build: (query: unknown) => unknown) => {
    let alias = ``
    const query = {
      from: (source: Record<string, unknown>) => {
        alias = Object.keys(source)[0] ?? ``
        return query
      },
      where: () => query,
    }
    build(query)
    return { data: alias === `s` ? state.sessions : [] }
  },
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => false,
}))
vi.mock(`@/lib/collections`, () => ({
  codingSessionCollection: {},
  issueLabelCollection: {},
  workflowEventCollection: {},
  workflowCollection: { utils: { awaitTxId: vi.fn().mockResolvedValue(true) } },
}))
vi.mock(`@/hooks/use-workflows`, () => ({
  useWorkflowNodes: () => state.nodes,
}))
vi.mock(`@/hooks/use-team-issue-graph`, () => ({
  useTeamIssueGraph: () => ({
    relations: [],
    issues: state.issues,
    counts: new Map(),
  }),
}))
vi.mock(`@/hooks/use-open-composer`, () => ({
  useOpenComposer: () => openComposer,
}))
vi.mock(`@/hooks/use-remote-start`, () => ({
  useRemoteStart: () => ({ devices: state.devices }),
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `me` } } }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => [{ id: `b1`, slug: `app` }],
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
  useTeamLabels: () => [],
  useTeamUsers: () => ({ users: [], userMap: new Map() }),
}))
vi.mock(`@/hooks/use-team-permissions`, () => ({
  useTeamPermissions: () => ({ canMutateIssue: () => true, isModerator: true }),
}))
vi.mock(`@/hooks/use-team-statuses`, () => ({
  useTeamStatuses: () => ({ options: [], resolve: () => ({}) }),
}))
vi.mock(`@/lib/board-view`, () => ({
  buildIssueLabelMap: () => new Map(),
  buildVisibleIssueGroups: () => [],
  nestIssueGroups: (groups: unknown) => groups,
}))
vi.mock(`@/hooks/use-agents-data`, () => ({
  rowPrState: () => null,
  useSessionListRows: (_teamId: string, sessions: { id: string }[]) =>
    sessions.map((session) => ({ session })),
  useSessionRow: (_team: string, _user: string, sessionId: string) => {
    const session = (state.sessions as { id: string }[]).find(
      (row) => row.id === sessionId
    )
    return { row: session ? { session } : null, session: session ?? null }
  },
}))
vi.mock(`@/hooks/use-review-files`, () => ({
  useReviewFiles: () => ({ state: { kind: `files`, files: [] } }),
}))
vi.mock(`@/lib/session-identity`, () => ({
  sessionIdentity: () => ({ identifier: null, subject: `Run` }),
}))
const steerSnapshot = { phase: { kind: `live` }, config: null }
vi.mock(`@/lib/steer-session-store`, () => ({
  acquireSteerSession: () => ({
    subscribe: () => () => {},
    getSnapshot: () => steerSnapshot,
    connect: () => {},
    sendMessage: () => true,
  }),
}))
vi.mock(`@/lib/workflows/open-questions`, () => ({
  workflowOpenQuestions: () => state.questions,
}))
vi.mock(`@/components/agent-session`, () => ({
  AgentSessionView: ({ session }: { session: { id: string } }) => (
    <div data-testid={`steer-${session.id}`} />
  ),
}))
vi.mock(`@/components/changes-view`, () => ({
  ChangesView: () => <div data-testid="changes-view" />,
}))
vi.mock(`@/components/issue-blocks-badge`, () => ({
  IssueBlocksPopover: ({ trigger }: { trigger: React.ReactNode }) => trigger,
}))
vi.mock(`@/components/issue-detail-view`, () => ({
  IssueDetailView: ({ issue }: { issue: { identifier: string } }) => (
    <div data-testid={`issue-detail-${issue.identifier}`} />
  ),
}))
vi.mock(`@/components/issue-list`, () => ({
  IssueList: () => <div data-testid="issue-list" />,
}))
vi.mock(`@/components/issue-coding-rows`, () => ({
  SessionStatusBadge: () => <span />,
}))
vi.mock(`@/components/session-tree`, () => ({
  SessionTree: ({
    rows,
    onOpen,
  }: {
    rows: { session: { id: string } }[]
    onOpen: (session: { id: string }) => void
  }) => (
    <div data-testid="session-tree">
      {rows.map((row) => (
        <button
          key={row.session.id}
          type="button"
          data-testid={`tree-${row.session.id}`}
          onClick={() => onOpen(row.session)}
        />
      ))}
    </div>
  ),
}))
vi.mock(`@/components/steer-composer`, () => ({
  SteerComposer: () => <div data-testid="question-composer" />,
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    workflows: Object.fromEntries(
      Object.entries(mutates).map(([name, mutate]) => [name, { mutate }])
    ),
  },
}))

import { WorkflowDetail } from "@/components/workflow-detail"

const issue = (id: string, identifier: string): Issue =>
  ({
    id,
    identifier,
    title: `Issue ${identifier}`,
    status: `backlog`,
    boardId: `b1`,
    prNumber: null,
    prState: null,
  }) as unknown as Issue

const node = (id: string, over: Partial<WorkflowNode> = {}): WorkflowNode =>
  ({
    id,
    workflowId: `wf`,
    teamId: `t1`,
    issueId: `i-${id}`,
    memberIssueIds: [],
    state: `blocked`,
    wave: 0,
    lane: 0,
    sessionId: null,
    note: null,
    afterNodeIds: [],
    ...over,
  }) as unknown as WorkflowNode

const workflow = (over: Partial<SyncedWorkflow> = {}): SyncedWorkflow =>
  ({
    id: `wf`,
    teamId: `t1`,
    name: `Checkout`,
    status: `running`,
    deviceId: `mac`,
    repositoryId: `r1`,
    decisions: ``,
    metrics: { nodes: 3, edges: 2, depth: 2, width: 2, cycles: [] },
    finalPrUrl: null,
    finalPrNumber: null,
    finalPrState: null,
    ...over,
  }) as unknown as SyncedWorkflow

beforeEach(() => {
  vi.clearAllMocks()
  state.nodes = [
    node(`a`, { wave: 0, lane: 0 }),
    node(`b`, { wave: 1, lane: 0, sessionId: `s-b`, state: `running` }),
    node(`c`, { wave: 1, lane: 1, state: `failed` }),
  ]
  state.issues = [issue(`i-a`, `EXP-1`), issue(`i-b`, `EXP-2`), issue(`i-c`, `EXP-3`)]
  state.sessions = [
    {
      id: `s-b`,
      userId: `me`,
      workflowId: `wf`,
      workflowNodeId: `b`,
      agentBusy: true,
      results: null,
      agent: null,
    } as unknown as CodingSession,
  ]
  state.questions = []
  state.devices = [
    { deviceId: `mac`, deviceLabel: `MacBook`, caps: [`workflows`], online: true },
  ]
})

const renderPage = (over: Partial<SyncedWorkflow> = {}) =>
  render(<WorkflowDetail workflow={workflow(over)} teamSlug="acme" />)

const pressed = (id: string) =>
  screen.getByTestId(`workflow-node-${id}`).getAttribute(`aria-pressed`)

describe(`strip selection model`, () => {
  const order = [`a`, `b`, `c`, `d`]

  it(`a plain click picks exactly one node; All clears`, () => {
    const one = selectNode(ALL_SELECTION, `b`, order)
    expect(one.ids).toEqual([`b`])
    expect(selectNode(one, `c`, order).ids).toEqual([`c`])
    expect(selectNode(one, null, order)).toBe(ALL_SELECTION)
  })

  it(`cmd/ctrl-click toggles, and the last one off is All`, () => {
    let sel = selectNode(ALL_SELECTION, `c`, order)
    sel = selectNode(sel, `a`, order, { toggle: true })
    expect(sel.ids).toEqual([`a`, `c`])
    sel = selectNode(sel, `a`, order, { toggle: true })
    expect(sel.ids).toEqual([`c`])
    expect(selectNode(sel, `c`, order, { toggle: true })).toBe(ALL_SELECTION)
  })

  it(`shift-click extends a range in DAG order from the anchor`, () => {
    const sel = selectNode(ALL_SELECTION, `b`, order)
    expect(selectNode(sel, `d`, order, { extend: true }).ids).toEqual([`b`, `c`, `d`])
    expect(selectNode(sel, `a`, order, { extend: true }).ids).toEqual([`a`, `b`])
  })

  it(`steps through All + the nodes, clamped at both ends`, () => {
    let sel = stepSelection(ALL_SELECTION, 1, order)
    expect(sel.ids).toEqual([`a`])
    sel = stepSelection(sel, 1, order)
    expect(sel.ids).toEqual([`b`])
    expect(stepSelection(selectNode(ALL_SELECTION, `a`, order), -1, order)).toBe(
      ALL_SELECTION
    )
    expect(stepSelection(ALL_SELECTION, -1, order)).toBe(ALL_SELECTION)
    expect(stepSelection(selectNode(ALL_SELECTION, `d`, order), 1, order).ids).toEqual([
      `d`,
    ])
    // A multi-pick steps from the node last clicked.
    const multi = selectNode(selectNode(ALL_SELECTION, `c`, order), `a`, order, {
      toggle: true,
    })
    expect(stepSelection(multi, 1, order).ids).toEqual([`b`])
  })

  it(`maps ←/→ and j/k, never a modified key`, () => {
    expect(stripStepKey({ key: `ArrowRight` })).toBe(1)
    expect(stripStepKey({ key: `j` })).toBe(1)
    expect(stripStepKey({ key: `ArrowLeft` })).toBe(-1)
    expect(stripStepKey({ key: `k` })).toBe(-1)
    expect(stripStepKey({ key: `j`, metaKey: true })).toBeNull()
    expect(stripStepKey({ key: `x` })).toBeNull()
  })

  it(`prunes nodes that left the workflow`, () => {
    const sel = selectNode(selectNode(ALL_SELECTION, `a`, order), `b`, order, {
      toggle: true,
    })
    expect(pruneSelection(sel, [`b`]).ids).toEqual([`b`])
    expect(pruneSelection(sel, [`c`])).toBe(ALL_SELECTION)
  })

  it(`routes selection × face to a body`, () => {
    expect(workflowBody(`issue`, 0)).toBe(`issue-list`)
    expect(workflowBody(`issue`, 1)).toBe(`issue-detail`)
    expect(workflowBody(`issue`, 2)).toBe(`issue-list`)
    expect(workflowBody(`runs`, 0)).toBe(`runs`)
    expect(workflowBody(`changes`, 1)).toBe(`changes`)
    expect(workflowBody(`results`, 3)).toBe(`results`)
  })
})

describe(`WorkflowDetail`, () => {
  it(`draws All first, then the nodes wave by wave`, () => {
    renderPage()
    const strip = screen.getByTestId(`workflow-strip`)
    const ids = [...strip.querySelectorAll(`[data-testid^="workflow-node-"]`)]
      .map((el) => el.getAttribute(`data-testid`))
      .filter((id) => !id!.endsWith(`-menu`))
    expect(ids).toEqual([
      `workflow-node-all`,
      `workflow-node-a`,
      `workflow-node-b`,
      `workflow-node-c`,
    ])
    expect(screen.getByTestId(`workflow-caption`).textContent).toBe(
      `on MacBook · 0 of 3 done · 1 running`
    )
  })

  it(`click picks one, cmd-click toggles, shift-click extends`, () => {
    renderPage()
    fireEvent.click(screen.getByTestId(`workflow-node-a`))
    expect(pressed(`a`)).toBe(`true`)
    expect(pressed(`all`)).toBe(`false`)
    fireEvent.click(screen.getByTestId(`workflow-node-c`), { metaKey: true })
    expect([pressed(`a`), pressed(`b`), pressed(`c`)]).toEqual([`true`, `false`, `true`])
    fireEvent.click(screen.getByTestId(`workflow-node-a`))
    fireEvent.click(screen.getByTestId(`workflow-node-c`), { shiftKey: true })
    expect([pressed(`a`), pressed(`b`), pressed(`c`)]).toEqual([`true`, `true`, `true`])
    fireEvent.click(screen.getByTestId(`workflow-node-all`))
    expect(pressed(`all`)).toBe(`true`)
  })

  it(`routes All × Issue to the list and one node × Issue to its issue`, () => {
    renderPage()
    expect(screen.getByTestId(`workflow-body-issue-list`)).toBeTruthy()
    expect(screen.getByTestId(`issue-list`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`workflow-node-b`))
    expect(screen.getByTestId(`issue-detail-EXP-2`)).toBeTruthy()
  })

  it(`the keyboard steps the pick and keeps the face`, () => {
    renderPage()
    fireEvent.mouseDown(screen.getByText(CHANGES_FACE_LABEL))
    fireEvent.click(screen.getByText(CHANGES_FACE_LABEL))
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
    act(() => {
      fireEvent.keyDown(document, { key: `ArrowRight` })
    })
    expect(pressed(`a`)).toBe(`true`)
    act(() => {
      fireEvent.keyDown(document, { key: `j` })
    })
    expect(pressed(`b`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
    act(() => {
      fireEvent.keyDown(document, { key: `k` })
      fireEvent.keyDown(document, { key: `ArrowLeft` })
    })
    expect(pressed(`all`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
  })

  it(`one node × Runs opens its run in place`, () => {
    renderPage()
    fireEvent.mouseDown(screen.getByText(RUNS_FACE_LABEL))
    fireEvent.click(screen.getByText(RUNS_FACE_LABEL))
    expect(screen.getByTestId(`session-tree`)).toBeTruthy()
    expect(screen.queryByTestId(`steer-s-b`)).toBeNull()
    fireEvent.click(screen.getByTestId(`tree-s-b`))
    expect(screen.getByTestId(`steer-s-b`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`workflow-node-b`))
    expect(screen.getByTestId(`steer-s-b`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`workflow-node-a`))
    expect(screen.queryByTestId(`tree-s-b`)).toBeNull()
  })

  it(`marks a busy node live`, () => {
    renderPage()
    const chip = screen.getByTestId(`workflow-node-b`)
    expect(chip.querySelector(`[data-ping="true"]`)).toBeTruthy()
    expect(
      screen.getByTestId(`workflow-node-a`).querySelector(`[data-ping="true"]`)
    ).toBeNull()
  })

  it.each([
    [{ status: `draft`, deviceId: null }, `workflow-device`],
    [{ status: `draft` }, `workflow-start`],
    [{ status: `running` }, `workflow-pause`],
    [{ status: `paused` }, `workflow-resume`],
    [{ status: `done` }, `workflow-review`],
  ] as const)(`primary action for %o`, (over, testId) => {
    renderPage(over as Partial<SyncedWorkflow>)
    expect(screen.getByTestId(testId)).toBeTruthy()
    for (const other of [
      `workflow-device`,
      `workflow-start`,
      `workflow-pause`,
      `workflow-resume`,
      `workflow-review`,
    ]) {
      if (other !== testId) expect(screen.queryByTestId(other)).toBeNull()
    }
  })

  it(`the primary buttons call their intents`, async () => {
    const { unmount } = renderPage({ status: `draft` })
    fireEvent.click(screen.getByText(START_WORKFLOW_LABEL))
    expect(mutates.start).toHaveBeenCalledWith({ id: `wf` }, expect.anything())
    unmount()
    const running = renderPage({ status: `running` })
    fireEvent.click(screen.getByText(PAUSE_WORKFLOW_LABEL))
    expect(mutates.pause).toHaveBeenCalled()
    running.unmount()
    const paused = renderPage({ status: `paused` })
    fireEvent.click(screen.getByText(RESUME_WORKFLOW_LABEL))
    expect(mutates.resume).toHaveBeenCalled()
    paused.unmount()
    renderPage({ status: `done` })
    fireEvent.click(screen.getByTestId(`workflow-review`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/reviews`,
      params: { teamSlug: `acme` },
    })
  })

  it(`failed and cancelled offer no primary action`, () => {
    renderPage({ status: `cancelled` })
    expect(screen.queryByTestId(`workflow-start`)).toBeNull()
    expect(screen.queryByTestId(`workflow-review`)).toBeNull()
    expect(screen.getByTestId(`workflow-caption`).textContent).toBe(
      `Cancelled · 0 done · 1 failed`
    )
  })

  it(`shows the question banner only while a question is open`, () => {
    const { unmount } = renderPage()
    expect(screen.queryByTestId(`workflow-questions`)).toBeNull()
    unmount()
    state.questions = [
      { nodeId: `b`, sessionId: `s-b`, question: `Which API?`, askedAt: `2026-09-25` },
    ]
    renderPage()
    expect(screen.getByTestId(`workflow-question-b`).textContent).toContain(`Which API?`)
    expect(screen.getByTestId(`question-composer`)).toBeTruthy()
    // The asking node wears the badge.
    expect(
      screen.getByTestId(`workflow-node-b`).querySelector(`[aria-label="needs you"]`)
    ).toBeTruthy()
  })

  it(`offers Retry / Skip on a failed chip only`, async () => {
    renderPage()
    expect(screen.queryByTestId(`workflow-node-a-menu`)).toBeNull()
    expect(screen.queryByTestId(`workflow-node-b-menu`)).toBeNull()
    const trigger = screen.getByTestId(`workflow-node-c-menu`)
    fireEvent.pointerDown(trigger, { button: 0, pointerType: `mouse` })
    expect(await screen.findByText(RETRY_NODE_LABEL)).toBeTruthy()
    expect(screen.getByText(SKIP_NODE_LABEL)).toBeTruthy()
    fireEvent.click(screen.getByText(RETRY_NODE_LABEL))
    expect(mutates.resolveNode).toHaveBeenCalledWith(
      { nodeId: `c`, action: `retry` },
      expect.anything()
    )
  })

  it(`offers Admit / Dismiss on a proposed chip`, async () => {
    state.nodes = [node(`p`, { state: `proposed` })]
    renderPage()
    fireEvent.pointerDown(screen.getByTestId(`workflow-node-p-menu`), {
      button: 0,
      pointerType: `mouse`,
    })
    fireEvent.click(await screen.findByTestId(`workflow-node-p-admit`))
    expect(mutates.admitNode).toHaveBeenCalledWith(
      { nodeId: `p`, admit: true },
      expect.anything()
    )
  })

  it(`never offers Approve`, () => {
    renderPage({ status: `running` })
    expect(screen.queryByText(/Approve/)).toBeNull()
  })

  it(`the primary buttons read the shared labels`, () => {
    const { unmount } = renderPage({ status: `draft`, deviceId: null })
    expect(screen.getByText(PICK_DEVICE_LABEL)).toBeTruthy()
    unmount()
    renderPage({ status: `done` })
    expect(screen.getByText(REVIEW_FINAL_PR_LABEL)).toBeTruthy()
  })

  it(`a chip shows its caption on hover`, () => {
    state.nodes = [node(`n`, { state: `failed`, note: `Tests broke` })]
    renderPage()
    expect(screen.getByTestId(`workflow-node-n`).getAttribute(`title`)).toBe(
      `Tests broke`
    )
  })

  const openOverflow = async () => {
    fireEvent.pointerDown(screen.getByTestId(`workflow-more`), {
      button: 0,
      pointerType: `mouse`,
    })
    await screen.findByRole(`menu`)
    return screen
      .getAllByRole(`menuitem`)
      .map((item) => item.textContent)
  }

  it(`a draft's overflow: Plan, Runs on, Delete`, async () => {
    renderPage({ status: `draft` })
    expect(await openOverflow()).toEqual([
      PLAN_WORKFLOW_LABEL,
      RUNS_ON_LABEL,
      DELETE_WORKFLOW_LABEL,
    ])
    fireEvent.click(screen.getByText(PLAN_WORKFLOW_LABEL))
    expect(openComposer).toHaveBeenCalledWith({
      actionId: `builtin:plan-workflow`,
      workflowId: `wf`,
    })
  })

  it(`Runs on opens the device picker`, async () => {
    renderPage({ status: `draft` })
    await openOverflow()
    fireEvent.click(screen.getByText(RUNS_ON_LABEL))
    expect(await screen.findByText(`MacBook`)).toBeTruthy()
  })

  it.each([`running`, `paused`])(`a %s overflow: Stop only`, async (status) => {
    renderPage({ status })
    expect(await openOverflow()).toEqual([STOP_WORKFLOW_LABEL])
    fireEvent.click(screen.getByText(STOP_WORKFLOW_LABEL))
    fireEvent.click(await screen.findByTestId(`workflow-cancel-confirm`))
    expect(mutates.cancel).toHaveBeenCalledWith({ id: `wf` }, expect.anything())
  })

  it.each([`done`, `failed`, `cancelled`])(`a %s overflow: Delete only`, async (status) => {
    renderPage({ status })
    expect(await openOverflow()).toEqual([DELETE_WORKFLOW_LABEL])
  })
})
