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
  DISMISS_NODE_LABEL,
  MERGE_FINAL_PR_CONFIRM,
  NODE_UNSYNCED_TITLE,
  PICK_DEVICE_LABEL,
  PLAN_WORKFLOW_LABEL,
  REVIEW_FINAL_PR_LABEL,
  NO_CHANGES_LABEL,
  NO_RESULTS_LABEL,
  NO_RUNS_LABEL,
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
  reviewFiles: (_issueId: string | null): unknown => ({ kind: `files`, files: [] }),
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
  openFinalPr: vi.fn().mockResolvedValue({ url: `https://github.com/acme/app/pull/42` }),
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
  useReviewFiles: (issue: { id: string } | null) => ({
    state: state.reviewFiles(issue?.id ?? null),
  }),
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
  IssueBlocksPopover: ({
    trigger,
    issueId,
    openOnHover,
  }: {
    trigger: React.ReactNode
    issueId: string
    openOnHover?: boolean
  }) => (
    <span
      data-testid={`blocks-popover-${issueId}`}
      data-open-on-hover={String(openOnHover ?? true)}
    >
      {trigger}
    </span>
  ),
}))
vi.mock(`@/components/issue-detail-view`, () => ({
  IssueDetailView: ({ issue }: { issue: { identifier: string } }) => (
    <div data-testid={`issue-detail-${issue.identifier}`}>
      <textarea data-testid={`issue-editor-${issue.identifier}`} />
    </div>
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
    emptyNote,
  }: {
    rows: { session: { id: string } }[]
    onOpen: (session: { id: string }) => void
    emptyNote?: string
  }) => (
    <div data-testid="session-tree">
      {rows.length === 0 && emptyNote}
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
  state.reviewFiles = () => ({ kind: `files`, files: [] })
  state.devices = [
    { deviceId: `mac`, deviceLabel: `MacBook`, caps: [`workflows`], online: true },
  ]
})

const renderPage = (
  over: Partial<SyncedWorkflow> = {},
  seed: { initialFace?: `issue` | `runs` | `changes` | `results`; initialNodeId?: string } = {}
) => render(<WorkflowDetail workflow={workflow(over)} teamSlug="acme" {...seed} />)

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
    expect(stripStepKey({ key: `ArrowRight`, shiftKey: true })).toBeNull()
    expect(stripStepKey({ key: `ArrowLeft`, ctrlKey: true })).toBeNull()
    expect(stripStepKey({ key: `ArrowLeft`, altKey: true })).toBeNull()
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

  it(`the strip steps the pick on ←/→ and j/k and keeps the face`, () => {
    renderPage()
    fireEvent.mouseDown(screen.getByText(CHANGES_FACE_LABEL))
    fireEvent.click(screen.getByText(CHANGES_FACE_LABEL))
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
    const strip = screen.getByTestId(`workflow-strip`)
    act(() => {
      fireEvent.keyDown(strip, { key: `ArrowRight` })
    })
    expect(pressed(`a`)).toBe(`true`)
    act(() => {
      fireEvent.keyDown(strip, { key: `j` })
    })
    expect(pressed(`b`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
    act(() => {
      fireEvent.keyDown(strip, { key: `k` })
      fireEvent.keyDown(strip, { key: `ArrowLeft` })
    })
    expect(pressed(`all`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
  })

  it(`never steps on a modified key, and leaves unconsumed keys alone`, () => {
    renderPage()
    const strip = screen.getByTestId(`workflow-strip`)
    // fireEvent returns false when the handler called preventDefault.
    expect(fireEvent.keyDown(strip, { key: `ArrowRight`, shiftKey: true })).toBe(true)
    expect(fireEvent.keyDown(strip, { key: `ArrowRight`, metaKey: true })).toBe(true)
    expect(fireEvent.keyDown(strip, { key: `ArrowDown` })).toBe(true)
    expect(pressed(`all`)).toBe(`true`)
  })

  it(`a face tab keeps its arrows; j/k still step from the page`, () => {
    renderPage()
    const tab = screen.getByRole(`tab`, { name: CHANGES_FACE_LABEL })
    fireEvent.mouseDown(tab)
    fireEvent.click(tab)
    tab.focus()
    act(() => {
      fireEvent.keyDown(tab, { key: `ArrowRight` })
      fireEvent.keyDown(tab, { key: `j` })
    })
    expect(pressed(`all`)).toBe(`true`)
    // A document-level arrow never steps (it scrolls the diff/transcript).
    act(() => {
      fireEvent.keyDown(document.body, { key: `ArrowRight` })
    })
    expect(pressed(`all`)).toBe(`true`)
    act(() => {
      fireEvent.keyDown(document.body, { key: `j` })
    })
    expect(pressed(`a`)).toBe(`true`)
    // Typing never steps.
    const name = screen.getByTestId(`workflow-name`)
    act(() => {
      fireEvent.keyDown(name, { key: `j` })
    })
    expect(pressed(`a`)).toBe(`true`)
  })

  it(`roves the strip's one tab stop with the pick`, () => {
    renderPage()
    expect(screen.getByTestId(`workflow-node-all`).tabIndex).toBe(0)
    expect(screen.getByTestId(`workflow-node-a`).tabIndex).toBe(-1)
    fireEvent.click(screen.getByTestId(`workflow-node-b`))
    expect(screen.getByTestId(`workflow-node-all`).tabIndex).toBe(-1)
    expect(screen.getByTestId(`workflow-node-b`).tabIndex).toBe(0)
  })

  it(`the first click picks a chip, a second click lets its mini-graph open`, () => {
    renderPage()
    // false = preventDefault: the popover's toggle is skipped on a pick.
    expect(fireEvent.click(screen.getByTestId(`workflow-node-a`))).toBe(false)
    expect(pressed(`a`)).toBe(`true`)
    expect(fireEvent.click(screen.getByTestId(`workflow-node-a`))).toBe(true)
    expect(pressed(`a`)).toBe(`true`)
  })

  it(`writes the pick and the face back to the URL, replacing history`, () => {
    renderPage()
    expect(navigate).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`workflow-node-b`))
    expect(navigate).toHaveBeenLastCalledWith(
      expect.objectContaining({
        search: { node: `b`, face: undefined },
        replace: true,
      })
    )
    fireEvent.mouseDown(screen.getByText(CHANGES_FACE_LABEL))
    fireEvent.click(screen.getByText(CHANGES_FACE_LABEL))
    expect(navigate).toHaveBeenLastCalledWith(
      expect.objectContaining({ search: { node: `b`, face: `changes` } })
    )
  })

  it(`seeds the pick and the face from ?node= and ?face=`, () => {
    renderPage({}, { initialNodeId: `b`, initialFace: `changes` })
    expect(pressed(`b`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
    expect(navigate).not.toHaveBeenCalled()
  })

  it(`falls back to All for a ?node= that is not in the strip`, () => {
    renderPage({}, { initialNodeId: `gone` })
    expect(pressed(`all`)).toBe(`true`)
    expect(navigate).toHaveBeenLastCalledWith(
      expect.objectContaining({ search: { node: undefined, face: undefined } })
    )
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
    renderPage({
      status: `done`,
      finalPrUrl: `https://github.com/acme/app/pull/42`,
      finalPrNumber: 42,
      finalPrState: `merged`,
    })
    fireEvent.click(screen.getByTestId(`workflow-node-a`))
    fireEvent.click(screen.getByTestId(`workflow-review`))
    // Review = All × Changes on this page, where the final PR row lives.
    expect(pressed(`all`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
    expect(screen.getByTestId(`workflow-final-pr`)).toBeTruthy()
    expect(navigate).not.toHaveBeenCalledWith(
      expect.objectContaining({ to: `/t/$teamSlug/reviews` })
    )
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

  it(`dismisses a proposed chip only after the confirm`, async () => {
    state.nodes = [node(`p`, { state: `proposed` })]
    renderPage()
    fireEvent.pointerDown(screen.getByTestId(`workflow-node-p-menu`), {
      button: 0,
      pointerType: `mouse`,
    })
    fireEvent.click(await screen.findByTestId(`workflow-node-p-dismiss`))
    const confirm = await screen.findByTestId(`workflow-node-dismiss-confirm`)
    expect(confirm.textContent).toBe(DISMISS_NODE_LABEL)
    expect(mutates.admitNode).not.toHaveBeenCalled()
    fireEvent.click(confirm)
    await vi.waitFor(() =>
      expect(mutates.admitNode).toHaveBeenCalledWith(
        { nodeId: `p`, admit: false },
        expect.anything()
      )
    )
  })

  it(`skips a failed chip only after the confirm`, async () => {
    renderPage()
    fireEvent.pointerDown(screen.getByTestId(`workflow-node-c-menu`), {
      button: 0,
      pointerType: `mouse`,
    })
    fireEvent.click(await screen.findByTestId(`workflow-node-c-skip`))
    const confirm = await screen.findByTestId(`workflow-node-skip-confirm`)
    expect(mutates.resolveNode).not.toHaveBeenCalled()
    fireEvent.click(confirm)
    await vi.waitFor(() =>
      expect(mutates.resolveNode).toHaveBeenCalledWith(
        { nodeId: `c`, action: `skip` },
        expect.anything()
      )
    )
  })

  it(`draws a compound node as a stacked chip titled with its members`, () => {
    state.nodes = [node(`a`), node(`n`, { memberIssueIds: [`i-x`, `i-y`] })]
    state.issues = [issue(`i-a`, `EXP-1`), issue(`i-n`, `EXP-9`)]
    renderPage()
    const chip = screen.getByTestId(`workflow-node-n`)
    expect(chip.querySelector(`[data-slot="issue-chip-stack"]`)).toBeTruthy()
    expect(chip.textContent).toContain(`EXP-9 +2`)
    expect(
      screen.getByTestId(`workflow-node-a`).querySelector(`[data-slot="issue-chip-stack"]`)
    ).toBeNull()
  })

  it(`keeps a node whose issue has not synced`, () => {
    state.nodes = [
      node(`n`, { state: `ready`, issueId: `abcd1234-5e6f-4a7b-8c9d-0e1f2a3b4c5d` }),
    ]
    state.issues = []
    renderPage()
    const chip = screen.getByTestId(`workflow-node-n`)
    expect(chip.textContent).toContain(`abcd1234`)
    expect(chip.textContent).toContain(NODE_UNSYNCED_TITLE)
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
      `${RUNS_ON_LABEL} · MacBook`,
      DELETE_WORKFLOW_LABEL,
    ])
    fireEvent.click(screen.getByText(PLAN_WORKFLOW_LABEL))
    expect(openComposer).toHaveBeenCalledWith({
      actionId: `builtin:plan-workflow`,
      workflowId: `wf`,
    })
  })

  it(`Runs on opens the device picker, anchored under the overflow`, async () => {
    renderPage({ status: `draft` })
    await openOverflow()
    fireEvent.click(screen.getByText(`${RUNS_ON_LABEL} · MacBook`))
    expect(await screen.findByText(`MacBook`)).toBeTruthy()
    const anchor = screen.getByTestId(`workflow-runs-on-anchor`)
    expect(anchor.closest(`span.relative`)?.contains(screen.getByTestId(`workflow-more`))).toBe(true)
  })

  // EXP-1102: a live workflow whose runner is gone says so and offers to
  // re-bind; one whose runner is up keeps the pick frozen.
  it(`a running workflow with an offline runner offers to pick another device`, async () => {
    state.devices = [
      { deviceId: `mac`, deviceLabel: `MacBook`, caps: [`workflows`], online: false, lastSeenAt: `2026-09-25T14:49:00.000Z` },
      { deviceId: `srv`, deviceLabel: `Server`, caps: [`workflows`], online: true },
    ]
    renderPage({ status: `running` })
    const caption = screen.getByTestId(`workflow-runner-offline`)
    expect(caption.textContent).toMatch(/^Runner offline since \d\d:\d\d\. Pick another device$/)
    fireEvent.click(screen.getByTestId(`workflow-runner-rebind`))
    fireEvent.click(await screen.findByText(`Server`))
    expect(mutates.update).toHaveBeenCalledWith(
      expect.objectContaining({ id: `wf`, deviceId: `srv` }),
      expect.anything()
    )
  })

  it(`a running workflow whose runner is up shows no offline caption`, () => {
    renderPage({ status: `running` })
    expect(screen.queryByTestId(`workflow-runner-offline`)).toBeNull()
  })

  it(`a running workflow whose runner id no row names any more offers a re-bind too`, () => {
    state.devices = [{ deviceId: `srv`, deviceLabel: `Server`, caps: [`workflows`], online: true }]
    renderPage({ status: `running` })
    expect(screen.getByTestId(`workflow-runner-offline`).textContent).toBe(`Runner offline. Pick another device`)
  })

  it(`Runs on reads bare while no device is bound`, async () => {
    renderPage({ status: `draft`, deviceId: null })
    expect(await openOverflow()).toContain(RUNS_ON_LABEL)
  })

  it.each([`running`, `paused`])(`a %s overflow: Stop only, cancelling after the confirm`, async (status) => {
    renderPage({ status })
    expect(await openOverflow()).toEqual([STOP_WORKFLOW_LABEL])
    fireEvent.click(screen.getByText(STOP_WORKFLOW_LABEL))
    const confirm = await screen.findByTestId(`workflow-cancel-confirm`)
    expect(mutates.cancel).not.toHaveBeenCalled()
    fireEvent.click(confirm)
    expect(mutates.cancel).toHaveBeenCalledWith({ id: `wf` }, expect.anything())
  })

  it.each([`done`, `failed`, `cancelled`])(`a %s overflow: Delete only`, async (status) => {
    renderPage({ status })
    expect(await openOverflow()).toEqual([DELETE_WORKFLOW_LABEL])
  })

  it(`Delete asks first, then deletes and returns to the list`, async () => {
    renderPage({ status: `done` })
    await openOverflow()
    fireEvent.click(screen.getByText(DELETE_WORKFLOW_LABEL))
    const confirm = await screen.findByTestId(`workflow-delete-confirm`)
    expect(mutates.delete).not.toHaveBeenCalled()
    fireEvent.click(confirm)
    await vi.waitFor(() =>
      expect(mutates.delete).toHaveBeenCalledWith({ id: `wf` }, expect.anything())
    )
    await vi.waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(
        expect.objectContaining({ to: `/t/$teamSlug/workflows` })
      )
    )
  })

  it(`disables Start and says why while a draft is not ready`, () => {
    renderPage({ status: `draft`, repositoryId: null })
    expect((screen.getByTestId(`workflow-start`) as HTMLButtonElement).disabled).toBe(true)
    expect(screen.getByTestId(`workflow-notice`).textContent).toBe(
      `The workflow's repository is gone.`
    )
  })

  it(`disables Start and spells out a blocking cycle`, () => {
    renderPage({
      status: `draft`,
      metrics: { nodes: 2, edges: 2, depth: 1, width: 2, cycles: [[`EXP-1`, `EXP-2`]] },
    } as Partial<SyncedWorkflow>)
    expect((screen.getByTestId(`workflow-start`) as HTMLButtonElement).disabled).toBe(true)
    expect(screen.getByTestId(`workflow-notice`).textContent).toContain(`EXP-1, EXP-2`)
  })

  it(`starts a ready draft, with no notice`, () => {
    renderPage({ status: `draft` })
    expect((screen.getByTestId(`workflow-start`) as HTMLButtonElement).disabled).toBe(false)
    expect(screen.queryByTestId(`workflow-notice`)).toBeNull()
  })

  it(`freezes the runner device once the workflow started`, async () => {
    const view = render(
      <WorkflowDetail workflow={workflow({ status: `draft` })} teamSlug="acme" />
    )
    await openOverflow()
    fireEvent.click(screen.getByText(`${RUNS_ON_LABEL} · MacBook`))
    expect(await screen.findByText(`MacBook`)).toBeTruthy()
    // The workflow starts while the picker is open: it closes, and nothing
    // on the page can write the runner any more.
    view.rerender(
      <WorkflowDetail workflow={workflow({ status: `running` })} teamSlug="acme" />
    )
    expect(screen.queryByText(`MacBook`)).toBeNull()
    expect(screen.queryByTestId(`workflow-device`)).toBeNull()
    expect(await openOverflow()).toEqual([STOP_WORKFLOW_LABEL])
    expect(mutates.update).not.toHaveBeenCalled()
  })

  it(`names no runner it cannot resolve (never a raw device id)`, () => {
    state.devices = []
    renderPage({ status: `running`, deviceId: `3f2a9c1e-uuid` })
    expect(screen.getByTestId(`workflow-caption`).textContent).toBe(
      `0 of 3 done · 1 running`
    )
  })

  it(`saves the name on blur, and nothing but the name`, async () => {
    renderPage({ status: `draft` })
    const name = screen.getByTestId(`workflow-name`) as HTMLInputElement
    fireEvent.focus(name)
    fireEvent.change(name, { target: { value: `Renamed` } })
    expect(mutates.update).not.toHaveBeenCalled()
    fireEvent.blur(name)
    await vi.waitFor(() =>
      expect(mutates.update).toHaveBeenCalledWith(
        { id: `wf`, name: `Renamed` },
        expect.anything()
      )
    )
    expect(mutates.update).toHaveBeenCalledTimes(1)
  })

  it(`Escape discards the edited name without saving`, async () => {
    renderPage({ status: `draft` })
    const name = screen.getByTestId(`workflow-name`) as HTMLInputElement
    name.focus()
    fireEvent.focus(name)
    fireEvent.change(name, { target: { value: `Renamed` } })
    act(() => {
      fireEvent.keyDown(name, { key: `Escape` })
    })
    expect(document.activeElement).not.toBe(name)
    await act(async () => {})
    expect(mutates.update).not.toHaveBeenCalled()
    expect(name.value).toBe(`Checkout`)
    // The next edit saves again.
    name.focus()
    fireEvent.change(name, { target: { value: `Renamed` } })
    act(() => {
      name.blur()
    })
    await vi.waitFor(() =>
      expect(mutates.update).toHaveBeenCalledWith(
        { id: `wf`, name: `Renamed` },
        expect.anything()
      )
    )
  })

  it(`a key inside a chip's menu or a popover never steps the strip`, () => {
    renderPage()
    const strip = screen.getByTestId(`workflow-strip`)
    const menu = document.createElement(`div`)
    menu.setAttribute(`role`, `menu`)
    const item = document.createElement(`div`)
    menu.appendChild(item)
    strip.appendChild(menu)
    act(() => {
      fireEvent.keyDown(item, { key: `ArrowRight` })
      fireEvent.keyDown(item, { key: `j` })
    })
    expect(pressed(`all`)).toBe(`true`)
    const popper = document.createElement(`div`)
    popper.setAttribute(`data-radix-popper-content-wrapper`, ``)
    const inner = document.createElement(`button`)
    popper.appendChild(inner)
    strip.appendChild(popper)
    act(() => {
      fireEvent.keyDown(inner, { key: `ArrowRight` })
    })
    expect(pressed(`all`)).toBe(`true`)
    strip.removeChild(menu)
    strip.removeChild(popper)
  })

  it(`a chip's hover opens no mini-graph while the name is being edited`, () => {
    renderPage()
    const popover = screen.getByTestId(`blocks-popover-i-a`)
    expect(popover.getAttribute(`data-open-on-hover`)).toBe(`true`)
    const name = screen.getByTestId(`workflow-name`)
    act(() => {
      name.focus()
    })
    expect(popover.getAttribute(`data-open-on-hover`)).toBe(`false`)
    fireEvent.mouseEnter(screen.getByTestId(`workflow-node-a`))
    expect(document.activeElement).toBe(name)
    act(() => {
      name.blur()
    })
    expect(popover.getAttribute(`data-open-on-hover`)).toBe(`true`)
  })

  it(`a chip's hover opens no mini-graph while an embedded editor is focused`, () => {
    renderPage({}, { initialNodeId: `a` })
    const popover = screen.getByTestId(`blocks-popover-i-a`)
    expect(popover.getAttribute(`data-open-on-hover`)).toBe(`true`)
    const editor = screen.getByTestId(`issue-editor-EXP-1`)
    act(() => {
      editor.focus()
    })
    expect(popover.getAttribute(`data-open-on-hover`)).toBe(`false`)
    fireEvent.mouseEnter(screen.getByTestId(`workflow-node-a`))
    expect(document.activeElement).toBe(editor)
    act(() => {
      editor.blur()
    })
    expect(popover.getAttribute(`data-open-on-hover`)).toBe(`true`)
  })

  it(`All × Changes with no nodes and no final PR reads No changes yet`, () => {
    state.nodes = []
    renderPage({ status: `draft` }, { initialFace: `changes` })
    expect(screen.getByTestId(`workflow-changes-empty`).textContent).toBe(
      NO_CHANGES_LABEL
    )
  })

  it(`a draft's Runs face reads No runs yet`, () => {
    state.nodes = []
    state.sessions = []
    renderPage({ status: `draft` }, { initialFace: `runs` })
    expect(screen.getByTestId(`session-tree`).textContent).toBe(NO_RUNS_LABEL)
  })

  it(`Results with nothing published reads No results yet`, () => {
    renderPage({}, { initialFace: `results` })
    expect(screen.getByTestId(`workflow-results-empty`).textContent).toBe(
      NO_RESULTS_LABEL
    )
  })

  it(`re-seeds the pick and the face from an external search change`, () => {
    const view = renderPage({}, { initialNodeId: `b`, initialFace: `runs` })
    expect(pressed(`b`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-runs`)).toBeTruthy()
    // The session tree's workflow row: the bare URL = All × Issue.
    view.rerender(<WorkflowDetail workflow={workflow()} teamSlug="acme" />)
    expect(pressed(`all`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-issue-list`)).toBeTruthy()
    expect(navigate).not.toHaveBeenCalled()
    // A `?node=&face=` link applies.
    view.rerender(
      <WorkflowDetail
        workflow={workflow()}
        teamSlug="acme"
        initialNodeId="c"
        initialFace="changes"
      />
    )
    expect(pressed(`c`)).toBe(`true`)
    expect(screen.getByTestId(`workflow-body-changes`)).toBeTruthy()
    expect(navigate).not.toHaveBeenCalled()
  })

  it(`the page's own write-back does not re-seed or loop`, () => {
    const view = renderPage()
    fireEvent.click(screen.getByTestId(`workflow-node-b`))
    expect(navigate).toHaveBeenCalledTimes(1)
    // The router echoes the written search back as props.
    view.rerender(
      <WorkflowDetail workflow={workflow()} teamSlug="acme" initialNodeId="b" />
    )
    expect(pressed(`b`)).toBe(`true`)
    expect(navigate).toHaveBeenCalledTimes(1)
    // A pick made after the echo still writes.
    fireEvent.click(screen.getByTestId(`workflow-node-c`))
    expect(navigate).toHaveBeenCalledTimes(2)
  })

  it(`All × Changes keeps every node's row; one without a PR reads No changes yet`, () => {
    state.nodes = [node(`a`, { wave: 0, lane: 0 }), node(`b`, { wave: 1, lane: 0 })]
    state.issues = [
      { ...issue(`i-a`, `EXP-1`), prNumber: 7 } as Issue,
      issue(`i-b`, `EXP-2`),
    ]
    state.reviewFiles = (issueId) =>
      issueId === `i-a`
        ? { kind: `files`, files: [{ path: `a.ts` }] }
        : { kind: `none` }
    renderPage({}, { initialFace: `changes` })
    expect(screen.getByTestId(`workflow-changes-a`).textContent).toContain(`EXP-1`)
    expect(screen.getByTestId(`workflow-changes-b`).textContent).toContain(`EXP-2`)
    expect(screen.queryByTestId(`workflow-changes-a-empty`)).toBeNull()
    expect(screen.getByTestId(`workflow-changes-b-empty`).textContent).toBe(
      `No changes yet`
    )
  })

  it(`All × Changes shows a loading node's row with a skeleton`, () => {
    state.reviewFiles = (issueId) =>
      issueId === `i-a` ? { kind: `loading` } : { kind: `files`, files: [] }
    renderPage({}, { initialFace: `changes` })
    expect(screen.getByTestId(`workflow-changes-a-loading`)).toBeTruthy()
    expect(screen.getByTestId(`workflow-changes-a`).textContent).toContain(`EXP-1`)
  })

  it(`merges the final pull request after the confirm`, async () => {
    renderPage({
      status: `running`,
      finalPrUrl: `https://github.com/acme/app/pull/42`,
      finalPrNumber: 42,
      finalPrState: `open`,
    })
    fireEvent.mouseDown(screen.getByText(CHANGES_FACE_LABEL))
    fireEvent.click(screen.getByText(CHANGES_FACE_LABEL))
    fireEvent.click(screen.getByTestId(`workflow-final-pr-merge`))
    expect(await screen.findByText(MERGE_FINAL_PR_CONFIRM)).toBeTruthy()
    expect(mutates.mergeFinalPr).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`workflow-final-pr-merge-confirm`))
    await vi.waitFor(() =>
      expect(mutates.mergeFinalPr).toHaveBeenCalledWith({ id: `wf` }, expect.anything())
    )
  })

  // EXP-1059: closed without merging — the row offers the way back, no
  // confirm (nothing lands), and the synced row swaps it back to Merge.
  it(`offers Open final PR on a final pull request closed without merging`, async () => {
    renderPage(
      {
        status: `running`,
        finalPrUrl: `https://github.com/acme/app/pull/42`,
        finalPrNumber: 42,
        finalPrState: `closed`,
      },
      { initialFace: `changes` }
    )
    expect(screen.queryByTestId(`workflow-final-pr-merge`)).toBeNull()
    fireEvent.click(screen.getByTestId(`workflow-final-pr-open`))
    await vi.waitFor(() =>
      expect(mutates.openFinalPr).toHaveBeenCalledWith({ id: `wf` }, expect.anything())
    )
    expect(mutates.mergeFinalPr).not.toHaveBeenCalled()
  })

  it(`offers no Merge on a merged final pull request`, () => {
    renderPage(
      {
        status: `done`,
        finalPrUrl: `https://github.com/acme/app/pull/42`,
        finalPrNumber: 42,
        finalPrState: `merged`,
      },
      { initialFace: `changes` }
    )
    expect(screen.getByTestId(`workflow-final-pr`)).toBeTruthy()
    expect(screen.queryByTestId(`workflow-final-pr-merge`)).toBeNull()
  })
})
