import { createContext, useContext, useRef, type ReactNode } from "react"
import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { IssueContextMenuProvider } from "@/components/issue-context-menu/provider"
import { issueMenuProps } from "@/components/issue-context-menu/attr"
import { useIssueMenuSelection } from "@/components/issue-context-menu/selection"
import type { Team } from "@/db/schema"
import { issueMenuLabels } from "@exp/ui"

// EXP-1074 — THE issue context menu: one host, opened by a right-click (or a
// touch long-press) on any element carrying `data-issue-menu`, resolving the
// issue live by id. The cases below are the old per-row menu's (quick
// actions, quick edits, the duplicate interception, the board-move confirm,
// delete, relations) plus the host's own: the attribute gate, the estimate
// submenu, the phone's Select through a registered list, re-targeting.

// Radix positions the menu (and its submenus) with ResizeObserver, and a
// focused row scrolls into view; jsdom has neither.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const mockState = vi.hoisted(() => ({
  addLabelMutate: vi.fn(),
  createRelationMutate: vi.fn(),
  clipboardWriteText: vi.fn(),
  deleteMutate: vi.fn(),
  moveMutate: vi.fn(),
  removeLabelMutate: vi.fn(),
  updateMutate: vi.fn(),
  openIssue: vi.fn(),
  openIssueIn: vi.fn(),
  agent: null as Record<string, unknown> | null,
  auth: null as { user: { id: string } } | null,
}))

vi.mock(`@/hooks/use-billing`, () => ({ useBillingPlan: () => null }))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: mockState.auth }),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issueLabels: {
      add: { mutate: mockState.addLabelMutate },
      remove: { mutate: mockState.removeLabelMutate },
    },
    issues: {
      delete: { mutate: mockState.deleteMutate },
      move: { mutate: mockState.moveMutate },
      update: { mutate: mockState.updateMutate },
    },
    relations: {
      create: { mutate: mockState.createRelationMutate },
    },
  },
}))

// #EXP-2027: another team's row gets ITS team's refs around the menu; the
// stand-in records which team an Open went through.
const RefTeam = vi.hoisted(() => ({ context: null as unknown }))
vi.mock(`@/components/issue-ref-provider`, () => {
  RefTeam.context = createContext<string | null>(null)
  const Context = RefTeam.context as React.Context<string | null>
  return {
    IssueRefProvider: ({ teamSlug, children }: { teamSlug: string; children: ReactNode }) => (
      <Context.Provider value={teamSlug}>{children}</Context.Provider>
    ),
    useIssueRefs: () => {
      const teamSlug = useContext(Context)
      return {
        open: teamSlug
          ? (identifier: string, options?: unknown) =>
              mockState.openIssueIn(teamSlug, identifier, options)
          : mockState.openIssue,
      }
    },
  }
})

// EXP-760: the picker is the shared IssuePickerDialog, whose real results come
// from the IssueRefProvider (not mounted here). Stand in for it with the same
// two observable facts the tests need: the empty-state line the duplicate flow
// asserts on, and one row to click so a relation pick can be completed.
vi.mock(`@/components/issue-picker-dialog`, () => ({
  IssuePickerDialog: ({
    open,
    title,
    onPick,
  }: {
    open: boolean
    title?: string
    onPick: (issue: { id: string; identifier: string }) => void
  }) =>
    open ? (
      <div>
        <span>{title}</span>
        <span>No issues to pick from</span>
        <button
          type="button"
          onClick={() => onPick({ id: `issue-9`, identifier: `APP-9` })}
        >
          Pick APP-9
        </button>
      </div>
    ) : null,
}))

// The synced shapes, answered by table and by the ONE `eq` the query names —
// the session's own issue lookup, the issue's labels, the team's labels /
// boards / members / users.
const liveRows = vi.hoisted(() => ({
  byTable: {} as Record<string, Record<string, unknown>[]>,
}))
vi.mock(`@tanstack/react-db`, () => ({
  eq: (column: string, value: unknown) => ({ column, value }),
  useLiveQuery: (build: (query: unknown) => unknown) => {
    let table: string | null = null
    let filter: { column: string; value: unknown } | null = null
    const columns = new Proxy({}, { get: (_target, column) => String(column) })
    const chain = {
      where(predicate: (tables: Record<string, unknown>) => unknown) {
        filter = predicate({ [table ?? ``]: columns }) as typeof filter
        return chain
      },
      orderBy() {
        return chain
      },
    }
    const query = {
      from(source: Record<string, unknown>) {
        table = Object.keys(source)[0] ?? null
        return chain
      },
    }
    // `undefined` is the skipped query.
    if (build(query) === undefined) return { data: [], isReady: false }
    const rows = table ? (liveRows.byTable[table] ?? []) : []
    // Assigned inside the `where` callback, which the narrowing cannot see.
    const active = filter as { column: string; value: unknown } | null
    return {
      data: active ? rows.filter((row) => row[active.column] === active.value) : rows,
      isReady: true,
    }
  },
}))
// #EXP-1383: no team agents in these cases (an agent ROW only for the
// agent menu's case).
vi.mock(`@/hooks/use-agent-row`, () => {
  const none: never[] = []
  return {
    useAgentRow: () => ({ agent: mockState.agent, isReady: true }),
    useMyAgentConversation: () => null,
    useAgentRows: () => none,
    useAllAgentRows: () => none,
  }
})
vi.mock(`@/lib/collections`, () => ({
  boardCollection: {},
  issueCollection: {},
  issueLabelCollection: {},
  issueStatusCollection: {},
  labelCollection: {},
  messageCollection: {},
  sessionCollection: {},
  teamCollection: {},
  teamMemberCollection: {},
  teamInviteCollection: {},
  userCollection: {},
}))

const now = new Date(`2026-03-07T09:00:00Z`)

function issueRow(overrides: Record<string, unknown> = {}) {
  return {
    id: `issue-1`,
    boardId: `board-1`,
    teamId: `team-1`,
    number: 1,
    identifier: `APP-1`,
    title: `Ship custom context menu`,
    status: `backlog`,
    statusId: null,
    priority: `none`,
    assigneeId: null,
    dueDate: null,
    duplicateOfId: null,
    estimate: null,
    createdAt: now,
    updatedAt: now,
    ...overrides,
  }
}

const user = (id: string, name: string, email: string) => ({
  id,
  name,
  email,
  image: null,
  createdAt: now,
  updatedAt: now,
})

function seed(overrides: Partial<Record<string, Record<string, unknown>[]>> = {}) {
  liveRows.byTable = {
    issues: [issueRow()],
    issueLabels: [],
    labels: [
      { id: `label-1`, teamId: `team-1`, name: `Bug`, color: `#ef4444`, sortOrder: 0 },
      { id: `label-2`, teamId: `team-1`, name: `Ops`, color: `#3b82f6`, sortOrder: 1 },
    ],
    boards: [
      { id: `board-1`, teamId: `team-1`, name: `App`, slug: `app`, prefix: `APP`, color: `#6366f1`, icon: null, sortOrder: 0, createdAt: now },
      { id: `board-2`, teamId: `team-1`, name: `Platform`, slug: `platform`, prefix: `PLT`, color: `#22c55e`, icon: null, sortOrder: 1, createdAt: now },
    ],
    members: [
      { id: `m-1`, teamId: `team-1`, userId: `user-1`, createdAt: now },
      { id: `m-2`, teamId: `team-1`, userId: `user-2`, createdAt: now },
    ],
    users: [user(`user-1`, `Alice Doe`, `alice@example.com`), user(`user-2`, `Bob Smith`, `bob@example.com`)],
    ...overrides,
  }
}

const team = (estimationType = `fibonacci`) =>
  ({ id: `team-1`, slug: `acme`, name: `Acme`, estimationType }) as unknown as Team

function Host({
  children,
  estimation,
}: {
  children: ReactNode
  estimation?: string
}) {
  return (
    <IssueContextMenuProvider team={team(estimation)}>
      {children}
    </IssueContextMenuProvider>
  )
}

const row = (issueId = `issue-1`, from?: string) => (
  <div data-testid={`row-${issueId}`} {...issueMenuProps(issueId, from)}>
    Issue row {issueId}
  </div>
)

function openOn(testId = `row-issue-1`) {
  return fireEvent.contextMenu(screen.getByTestId(testId), {
    clientX: 10,
    clientY: 10,
  })
}

/** Opens the menu on the row and walks `path`: every label but the last
 *  opens a submenu (a Radix sub-trigger opens on click), the last is picked
 *  inside the innermost surface. */
function choose(path: string[], testId = `row-issue-1`) {
  openOn(testId)
  let scope: HTMLElement = screen.getByRole(`menu`)
  for (const label of path.slice(0, -1)) {
    fireEvent.click(within(scope).getByText(label))
    const subs = document.querySelectorAll<HTMLElement>(
      `[data-slot="dropdown-menu-sub-content"]`
    )
    scope = subs[subs.length - 1]!
  }
  fireEvent.click(within(scope).getByText(path[path.length - 1]!))
}

describe(`IssueContextMenuProvider`, () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(now)
    for (const fn of Object.values(mockState)) if (typeof fn === `function`) fn.mockReset()
    mockState.agent = null
    mockState.auth = null
    mockState.createRelationMutate.mockResolvedValue({})
    mockState.addLabelMutate.mockResolvedValue({})
    mockState.clipboardWriteText.mockResolvedValue(undefined)
    mockState.deleteMutate.mockResolvedValue({})
    mockState.moveMutate.mockResolvedValue({})
    mockState.removeLabelMutate.mockResolvedValue({})
    mockState.updateMutate.mockResolvedValue({})
    seed()

    Object.defineProperty(window.navigator, `clipboard`, {
      configurable: true,
      value: { writeText: mockState.clipboardWriteText },
    })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it(`opens only from an element carrying the attribute, and takes the event`, () => {
    render(
      <Host>
        <div data-testid="plain">Not an issue</div>
        {row()}
      </Host>
    )

    // A right-click elsewhere is the browser's.
    const plainDefault = fireEvent.contextMenu(screen.getByTestId(`plain`), {
      clientX: 10,
      clientY: 10,
    })
    expect(plainDefault).toBe(true)
    expect(screen.queryByRole(`menu`)).toBeNull()

    // On the row it is ours: prevented, and the menu names the issue.
    expect(openOn()).toBe(false)
    expect(screen.getByRole(`menu`)).not.toBeNull()
    expect(screen.getByText(`APP-1`)).not.toBeNull()
    expect(screen.getByText(`Ship custom context menu`)).not.toBeNull()
  })

  it(`runs quick actions for opening the issue and copying the issue id only`, async () => {
    render(<Host>{row(`issue-1`, `board:app`)}</Host>)
    choose([`Open issue`])
    choose([`Copy issue ID`])
    await Promise.resolve()
    await Promise.resolve()

    // "Open issue" keeps the list it came from beside the detail (EXP-851).
    expect(mockState.openIssue).toHaveBeenCalledWith(`APP-1`, { from: `board:app` })
    expect(mockState.clipboardWriteText).toHaveBeenCalledTimes(1)
    expect(mockState.clipboardWriteText).toHaveBeenCalledWith(`APP-1`)
    expect(screen.queryByText(`Copy title`)).toBeNull()
  })

  it(`runs quick-edit mutations for menu actions`, async () => {
    seed({
      issues: [issueRow({ assigneeId: `user-1` })],
      issueLabels: [{ issueId: `issue-1`, labelId: `label-1`, teamId: `team-1` }],
    })
    render(<Host>{row()}</Host>)

    choose([`Mark as done`])
    choose([`Status`, `In Progress`])
    choose([`Priority`, `High`])
    choose([`Assignee`, `Unassigned`])
    choose([`Assignee`, `Bob Smith`])
    choose([`Labels`, `Bug`])
    // A multi pick keeps the submenu open: the next toggle lands in it.
    const labels = document.querySelectorAll<HTMLElement>(
      `[data-slot="dropdown-menu-sub-content"]`
    )
    fireEvent.click(within(labels[labels.length - 1]!).getByText(`Ops`))
    choose([`Set due date`, `Tomorrow`])
    await Promise.resolve()
    await Promise.resolve()

    expect(mockState.updateMutate).toHaveBeenCalledWith({ id: `issue-1`, status: `done` })
    expect(mockState.updateMutate).toHaveBeenCalledWith({ id: `issue-1`, status: `in_progress` })
    expect(mockState.updateMutate).toHaveBeenCalledWith({ id: `issue-1`, priority: `high` })
    expect(mockState.updateMutate).toHaveBeenCalledWith({ id: `issue-1`, assigneeId: null, assigneeAgentId: null })
    expect(mockState.updateMutate).toHaveBeenCalledWith({ id: `issue-1`, assigneeId: `user-2`, assigneeAgentId: null })
    expect(mockState.updateMutate).toHaveBeenCalledWith({ id: `issue-1`, dueDate: `2026-03-08` })
    expect(mockState.removeLabelMutate).toHaveBeenCalledWith({ issueId: `issue-1`, labelId: `label-1` })
    expect(mockState.addLabelMutate).toHaveBeenCalledWith({ issueId: `issue-1`, labelId: `label-2` })
  })

  // EXP-1074: story points on the team's scale (EXP-630).
  it(`offers the estimate on the team's scale and hides it for a team without one`, async () => {
    seed({ issues: [issueRow({ estimate: 3 })] })
    const { unmount } = render(<Host>{row()}</Host>)

    choose([`Estimate`, `5 points`])
    choose([`Estimate`, `No estimate`])
    await Promise.resolve()

    expect(mockState.updateMutate).toHaveBeenCalledWith({ id: `issue-1`, estimate: 5 })
    expect(mockState.updateMutate).toHaveBeenCalledWith({ id: `issue-1`, estimate: null })
    unmount()

    render(<Host estimation="none">{row()}</Host>)
    openOn()
    expect(screen.queryByText(`Estimate`)).toBeNull()
  })

  it(`intercepts the duplicate status: opens the picker instead of writing status`, async () => {
    render(<Host>{row()}</Host>)

    // The picker is closed, so its search results are not mounted yet.
    expect(screen.queryByText(`No issues to pick from`)).toBeNull()

    choose([`Status`, `Duplicate`])
    // Picker opens on a deferred tick (past the menu's focus restore).
    await act(async () => {
      vi.runAllTimers()
    })

    expect(mockState.updateMutate).not.toHaveBeenCalledWith(
      expect.objectContaining({ status: `duplicate` })
    )
    expect(screen.getByText(`No issues to pick from`)).not.toBeNull()
  })

  it(`shows Unmark duplicate (and no Mark as duplicate entry) for a duplicate issue`, () => {
    const { unmount } = render(<Host>{row()}</Host>)
    openOn()
    expect(screen.queryByText(`Mark as duplicate…`)).toBeNull()
    expect(screen.queryByText(`Unmark duplicate`)).toBeNull()
    unmount()

    seed({ issues: [issueRow({ duplicateOfId: `issue-99` })] })
    render(<Host>{row()}</Host>)
    openOn()
    expect(screen.getByText(`Unmark duplicate`)).not.toBeNull()
    expect(screen.queryByText(`Mark as duplicate…`)).toBeNull()
  })

  it(`confirms before moving the issue to another board (EXP-428)`, async () => {
    render(<Host>{row()}</Host>)
    expect(screen.queryByTestId(`issue-move-board-confirm`)).toBeNull()

    choose([`Move to board`, `Platform`])
    await act(async () => {
      vi.runAllTimers()
    })

    expect(mockState.moveMutate).not.toHaveBeenCalled()
    expect(screen.getByTestId(`issue-move-board-confirm`)).not.toBeNull()
    expect(screen.getByText(`Move APP-1 to "Platform"?`)).not.toBeNull()
    expect(
      screen.getByText(`It gets a new identifier in that board.`)
    ).not.toBeNull()

    fireEvent.click(screen.getByRole(`button`, { name: `Move` }))
    await Promise.resolve()
    await Promise.resolve()

    expect(mockState.moveMutate).toHaveBeenCalledTimes(1)
    expect(mockState.moveMutate).toHaveBeenCalledWith({ id: `issue-1`, boardId: `board-2` })
  })

  // EXP-1215: Delete confirms in the shared `delete-issue` prompt (deferred
  // past the menu close like the move confirm), never on a submenu step.
  it(`deletes the issue once the delete prompt is answered`, async () => {
    render(<Host>{row()}</Host>)
    expect(screen.queryByTestId(`issue-delete-confirm`)).toBeNull()

    choose([`Delete issue`])
    await act(async () => {
      vi.runAllTimers()
    })

    expect(mockState.deleteMutate).not.toHaveBeenCalled()
    const prompt = screen.getByTestId(`issue-delete-confirm`)
    expect(prompt.textContent).toContain(`Move APP-1 to trash?`)
    expect(prompt.textContent).toContain(
      `After 30 days it is deleted with its comments and files.`
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Move to trash` }))
    await Promise.resolve()

    expect(mockState.deleteMutate).toHaveBeenCalledTimes(1)
    expect(mockState.deleteMutate).toHaveBeenCalledWith({ id: `issue-1` })
  })

  // EXP-760: the same six sides the issue header's `…` offers, from the one
  // hook that owns the picker — so a pick made from a list row lands on the
  // SAME canonical row a pick made from the detail page would.
  it(`adds a relation from the picked side (inverse sides pass inverse)`, async () => {
    render(<Host>{row()}</Host>)

    choose([`Add relation`, `Blocked by`])
    await act(async () => {
      vi.runAllTimers()
    })
    fireEvent.click(screen.getByText(`Pick APP-9`))

    expect(mockState.createRelationMutate).toHaveBeenCalledWith({
      issueId: `issue-1`,
      relatedIssueId: `issue-9`,
      type: `blocks`,
      inverse: true,
    })
  })

  it(`routes "Duplicate of" through issues.update, not relations.create`, async () => {
    render(<Host>{row()}</Host>)

    choose([`Add relation`, `Duplicate of`])
    await act(async () => {
      vi.runAllTimers()
    })
    fireEvent.click(screen.getByText(`Pick APP-9`))

    expect(mockState.createRelationMutate).not.toHaveBeenCalled()
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      id: `issue-1`,
      duplicateOfId: `issue-9`,
    })
  })

  it(`re-targets on a right-click on another row`, () => {
    seed({ issues: [issueRow(), issueRow({ id: `issue-2`, identifier: `APP-2`, title: `Second` })] })
    render(
      <Host>
        {row(`issue-1`)}
        {row(`issue-2`)}
      </Host>
    )
    openOn(`row-issue-1`)
    expect(screen.getByText(`APP-1`)).not.toBeNull()

    openOn(`row-issue-2`)
    expect(screen.getAllByRole(`menu`)).toHaveLength(1)
    expect(screen.queryByText(`APP-1`)).toBeNull()
    expect(screen.getByText(`APP-2`)).not.toBeNull()
  })

  // FEED-12: the phone's Select item toggles the LIST the row belongs to,
  // through the selection it registered.
  it(`toggles the registered list's selection from the Select item`, () => {
    const toggle = vi.fn()
    function List({ children }: { children: ReactNode }) {
      const root = useRef<HTMLDivElement>(null)
      useIssueMenuSelection({
        root,
        isSelected: (issueId) => issueId === `issue-2`,
        toggle,
      })
      return <div ref={root}>{children}</div>
    }
    seed({ issues: [issueRow(), issueRow({ id: `issue-2`, identifier: `APP-2` })] })
    render(
      <Host>
        <List>
          {row(`issue-1`)}
          {row(`issue-2`)}
        </List>
        {row(`issue-3`)}
      </Host>
    )

    choose([`Select`], `row-issue-1`)
    expect(toggle).toHaveBeenCalledWith(`issue-1`)

    openOn(`row-issue-2`)
    expect(screen.getByText(`Deselect`)).not.toBeNull()
  })

  // #EXP-1388 (owner 2026-10-27): on touch the long-press itself enters the
  // selection, as on the Chats list and the natives: no menu, no Select step.
  it(`selects on a touch long-press instead of opening the menu`, () => {
    const matchMedia = window.matchMedia
    window.matchMedia = ((query: string) =>
      ({ matches: query === `(pointer: coarse)`, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList) as typeof window.matchMedia
    try {
      const toggle = vi.fn()
      function List({ children }: { children: ReactNode }) {
        const root = useRef<HTMLDivElement>(null)
        useIssueMenuSelection({ root, isSelected: () => false, toggle })
        return <div ref={root}>{children}</div>
      }
      seed({ issues: [issueRow()] })
      render(
        <Host>
          <List>{row(`issue-1`)}</List>
        </Host>
      )
      openOn(`row-issue-1`)
      expect(toggle).toHaveBeenCalledWith(`issue-1`)
      expect(screen.queryByRole(`menu`)).toBeNull()
    } finally {
      window.matchMedia = matchMedia
    }
  })

  // The ONE layout (`@exp/ui` ISSUE_MENU_LAYOUT): what the styleguide draws
  // at rest and the IDE mirrors is what this host draws, top to bottom.
  it(`draws the items in the shared layout's order`, () => {
    function List({ children }: { children: ReactNode }) {
      const root = useRef<HTMLDivElement>(null)
      useIssueMenuSelection({ root, isSelected: () => false, toggle: () => {} })
      return <div ref={root}>{children}</div>
    }
    render(
      <Host>
        <List>{row()}</List>
      </Host>
    )
    openOn()

    const labels = issueMenuLabels(new Set([`phone`, `estimation`, `boards`]))
    expect(labels).toEqual([
      `Open issue`,
      `Mark as done`,
      `Copy issue ID`,
      `Select`,
      `Status`,
      `Assignee`,
      `Priority`,
      `Labels`,
      `Estimate`,
      `Set due date`,
      `Move to board`,
      `Add relation`,
      `Delete issue`,
    ])
    const text = screen.getByRole(`menu`).textContent ?? ``
    let at = -1
    for (const label of labels) {
      const next = text.indexOf(label, at + 1)
      expect(next, `${label} after position ${at}`).toBeGreaterThan(at)
      at = next
    }
  })

  it(`opens on a touch long-press, not on a tap`, () => {
    render(<Host>{row()}</Host>)
    const target = screen.getByTestId(`row-issue-1`)

    fireEvent.pointerDown(target, { pointerType: `touch`, clientX: 10, clientY: 10 })
    fireEvent.pointerUp(target, { pointerType: `touch` })
    act(() => {
      vi.advanceTimersByTime(700)
    })
    expect(screen.queryByRole(`menu`)).toBeNull()

    fireEvent.pointerDown(target, { pointerType: `touch`, clientX: 10, clientY: 10 })
    act(() => {
      vi.advanceTimersByTime(700)
    })
    expect(screen.getByRole(`menu`)).not.toBeNull()
  })

  // #EXP-2027: md+ has no active team — the layout's team is a fallback,
  // the menu acts in the team of the row it opened on.
  describe(`a row from another team`, () => {
    const betaTeam = { id: `team-2`, slug: `beta`, name: `Beta`, estimationType: `none` }
    beforeEach(() => {
      seed({
        issues: [
          issueRow(),
          issueRow({ id: `issue-7`, boardId: `board-3`, identifier: `OPS-7`, title: `Rotate keys` }),
        ],
        teams: [{ id: `team-1`, slug: `acme`, name: `Acme`, estimationType: `fibonacci` }, betaTeam],
        boards: [
          ...liveRows.byTable.boards!,
          { id: `board-3`, teamId: `team-2`, name: `Ops`, slug: `ops`, prefix: `OPS`, color: `#f97316`, icon: null, sortOrder: 0, createdAt: now },
          { id: `board-4`, teamId: `team-2`, name: `Infra`, slug: `infra`, prefix: `INF`, color: `#14b8a6`, icon: null, sortOrder: 1, createdAt: now },
        ],
        labels: [
          ...liveRows.byTable.labels!,
          { id: `label-9`, teamId: `team-2`, name: `Security`, color: `#eab308`, sortOrder: 0 },
        ],
        members: [
          ...liveRows.byTable.members!,
          { id: `m-3`, teamId: `team-2`, userId: `user-3`, createdAt: now },
        ],
        users: [...liveRows.byTable.users!, user(`user-3`, `Cleo Ray`, `cleo@example.com`)],
        issueStatuses: [
          { id: `st-b1`, teamId: `team-2`, name: `Inbox`, color: `#888888`, category: `backlog`, builtinKey: `backlog`, sortOrder: 0 },
          { id: `st-b2`, teamId: `team-2`, name: `Shipped`, color: `#22c55e`, category: `completed`, builtinKey: `done`, sortOrder: 1 },
        ],
      })
    })

    it(`offers that team's statuses, labels, members and boards, not the layout's`, () => {
      render(<Host>{row(`issue-7`)}</Host>)
      openOn(`row-issue-7`)
      const menu = screen.getByRole(`menu`)
      expect(within(menu).getByText(`OPS-7`)).not.toBeNull()
      // Its team's status row, and no estimate row (Beta does not estimate).
      expect(within(menu).getByText(`Inbox`)).not.toBeNull()
      expect(within(menu).queryByText(`Estimate`)).toBeNull()

      const sub = (label: string) => {
        fireEvent.click(within(menu).getByText(label))
        const subs = document.querySelectorAll<HTMLElement>(`[data-slot="dropdown-menu-sub-content"]`)
        return subs[subs.length - 1]!
      }
      const statuses = sub(`Status`)
      expect(within(statuses).getByText(`Shipped`)).not.toBeNull()
      const labels = sub(`Labels`)
      expect(within(labels).getByText(`Security`)).not.toBeNull()
      expect(within(labels).queryByText(`Bug`)).toBeNull()
      const assignees = sub(`Assignee`)
      expect(within(assignees).getByText(`Cleo Ray`)).not.toBeNull()
      expect(within(assignees).queryByText(`Alice Doe`)).toBeNull()
      const boards = sub(`Move to board`)
      expect(within(boards).getByText(`Infra`)).not.toBeNull()
      expect(within(boards).queryByText(`Platform`)).toBeNull()
    })

    it(`opens the issue through that team's refs`, () => {
      render(<Host>{row(`issue-7`, `chats`)}</Host>)
      choose([`Open issue`], `row-issue-7`)
      expect(mockState.openIssueIn).toHaveBeenCalledWith(`beta`, `OPS-7`, { from: `chats` })
      expect(mockState.openIssue).not.toHaveBeenCalled()
    })

    it(`keeps the layout's team for its own rows`, () => {
      render(<Host>{row()}</Host>)
      choose([`Open issue`])
      expect(mockState.openIssue).toHaveBeenCalledWith(`APP-1`, { from: undefined })
      expect(mockState.openIssueIn).not.toHaveBeenCalled()
    })

    it(`answers an agent row of that team as that team's member`, () => {
      mockState.auth = { user: { id: `user-3` } }
      mockState.agent = { id: `agent-1`, teamId: `team-2`, name: `Ops bot`, ownerId: null, kind: `agent` }
      render(
        <Host>
          <div data-testid="agent-row" data-menu="agent" data-menu-id="agent-1">
            Ops bot
          </div>
        </Host>
      )
      openOn(`agent-row`)
      expect(screen.getByTestId(`agent-menu`)).not.toBeNull()
    })
  })
})
