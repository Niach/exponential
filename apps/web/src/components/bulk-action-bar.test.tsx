import { act, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { BulkActionBar } from "@/components/bulk-action-bar"
import type { Issue, Label, User } from "@/db/schema"
import type { StatusResolvable, StatusRowInput } from "@/lib/team-statuses"

// EXP-957 — the bar's four property menus render through the shared
// `ComboboxMenuItems`, so what these tests pin is the thing the raw
// `DropdownMenuItem` lists could not do: MARK what the whole selection
// already has, and mark nothing when the selected issues disagree.

const mockState = vi.hoisted(() => ({
  bulkUpdate: vi.fn(),
  bulkDelete: vi.fn(),
  bulkAdd: vi.fn(),
  bulkRemove: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: {
      bulkUpdate: { mutate: mockState.bulkUpdate },
      bulkDelete: { mutate: mockState.bulkDelete },
    },
    issueLabels: {
      bulkAdd: { mutate: mockState.bulkAdd },
      bulkRemove: { mutate: mockState.bulkRemove },
    },
  },
}))

vi.mock(`@/lib/collections`, () => ({
  issueCollection: { utils: { awaitTxId: vi.fn().mockResolvedValue(true) } },
  issueLabelCollection: {
    utils: { awaitTxId: vi.fn().mockResolvedValue(true) },
  },
}))

vi.mock(`@/hooks/use-mobile-chrome`, () => ({
  useMobileChrome: () => ({ setBulkBarPresent: vi.fn() }),
}))

vi.mock(`@/hooks/use-chrome-height-var`, () => ({
  useChromeHeightVar: () => () => {},
}))

// A fixed three-row team so a pick reports a REAL status id (the constructed
// fallback rows write the anchor enum instead, which would not exercise the
// `byId` lookup the menu's `onChange` does). The rows live inside the factory
// because `vi.mock` is hoisted above every top-level binding.
vi.mock(`@/hooks/use-team-statuses`, async () => {
  const { buildStatusOptions, resolveIssueStatus } = await import(
    `@/lib/team-statuses`
  )
  const rows: StatusRowInput[] = [
    {
      id: `status-backlog`,
      name: `Backlog`,
      color: `#a1a1aa`,
      category: `backlog`,
      builtinKey: `backlog`,
      sortOrder: 0,
    },
    {
      id: `status-progress`,
      name: `In Progress`,
      color: `#f59e0b`,
      category: `started`,
      builtinKey: `in_progress`,
      sortOrder: 1,
    },
    {
      id: `status-done`,
      name: `Done`,
      color: `#22c55e`,
      category: `completed`,
      builtinKey: `done`,
      sortOrder: 2,
    },
  ]
  const options = buildStatusOptions(rows)
  const byId = new Map(options.map((option) => [option.id, option]))
  return {
    useTeamStatusesContext: () => ({
      options,
      byId,
      resolve: (issue: StatusResolvable) =>
        resolveIssueStatus(issue, options, byId),
      ready: true,
    }),
  }
})

// No signed-in user ⇒ the bulk "Start coding" pill renders nothing, so none of
// its device/board/relay wiring has to exist in a unit test.
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: null }),
}))
vi.mock(`@/components/agent-session`, () => ({ useSteerConfig: () => null }))
vi.mock(`@/components/issue-coding-rows`, () => ({
  useIsTeamMember: () => false,
}))
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamBoards: () => [] }))
vi.mock(`@/hooks/use-remote-start`, () => ({
  useRemoteStart: () => ({ devices: null }),
}))
vi.mock(`@/hooks/use-open-composer`, () => ({
  useOpenComposer: () => () => {},
}))

// Radix positions menus with ResizeObserver, which jsdom lacks.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

function buildIssue(overrides: Partial<Issue> = {}): Issue {
  return {
    id: `issue-1`,
    boardId: `board-1`,
    teamId: `team-1`,
    boardDeletedAt: null,
    boardArchivedAt: null,
    number: 1,
    identifier: `APP-1`,
    title: `Ship the bulk bar`,
    description: null,
    status: `in_progress`,
    statusId: `status-progress`,
    priority: `none`,
    assigneeId: null,
    creatorId: `user-1`,
    source: `user`,
    dueDate: null,
    sortOrder: 0,
    completedAt: null,
    duplicateOfId: null,
    prUrl: null,
    prNumber: null,
    prState: null,
    branch: null,
    prMergedAt: null,
    prBaseBranch: null,
    prStackNumber: null,
    createdAt: new Date(`2026-03-07T09:00:00Z`),
    updatedAt: new Date(`2026-03-07T09:00:00Z`),
    ...overrides,
  }
}

function buildUser(id: string, name: string, email: string): User {
  return {
    id,
    name,
    email,
    emailVerified: true,
    image: null,
    isAdmin: false,
    creemCustomerId: null,
    hadTrial: false,
    onboardingCompletedAt: null,
    desktopAppCardDismissedAt: null,
    signupRef: null,
    signupUtmSource: null,
    signupUtmMedium: null,
    signupUtmCampaign: null,
    signupCreemRef: null,
    signupReferrer: null,
    signupLandingPath: null,
    signupAnonymousId: null,
    timezone: null,
    teamIds: [],
    createdAt: new Date(`2026-03-01T00:00:00Z`),
    updatedAt: new Date(`2026-03-01T00:00:00Z`),
  }
}

function buildLabel(id: string, name: string, color: string): Label {
  return {
    id,
    teamId: `team-1`,
    name,
    color,
    sortOrder: 0,
    createdAt: new Date(`2026-03-01T00:00:00Z`),
    updatedAt: new Date(`2026-03-01T00:00:00Z`),
  }
}

const users = [
  buildUser(`user-1`, `Alice Doe`, `alice@example.com`),
  buildUser(`user-2`, `Bob Smith`, `bob@example.com`),
]

const labels = [
  buildLabel(`label-1`, `Bug`, `#ef4444`),
  buildLabel(`label-2`, `Ops`, `#3b82f6`),
  buildLabel(`label-3`, `Docs`, `#a855f7`),
]

const ISSUE_IDS = [`issue-1`, `issue-2`]

function renderBar({
  issues,
  issueLabelMap = new Map<string, Label[]>(),
}: {
  issues: Issue[]
  issueLabelMap?: Map<string, Label[]>
}) {
  return render(
    <BulkActionBar
      issues={issues}
      issueLabelMap={issueLabelMap}
      labels={labels}
      users={users}
      teamId="team-1"
      onClear={() => {}}
    />
  )
}

const openMenu = (label: string) => {
  act(() => {
    fireEvent.pointerDown(screen.getByRole(`button`, { name: label }), {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

const items = () =>
  Array.from(
    document.querySelectorAll<HTMLElement>(`[data-slot=dropdown-menu-item]`)
  )

const glyphs = (kind: string) =>
  Array.from(document.querySelectorAll(`[data-selected-glyph="${kind}"]`))

const rowByText = (text: string) =>
  items().find((row) => row.textContent?.includes(text))!

describe(`BulkActionBar property menus`, () => {
  beforeEach(() => {
    for (const mutate of [
      mockState.bulkUpdate,
      mockState.bulkDelete,
      mockState.bulkAdd,
      mockState.bulkRemove,
    ]) {
      mutate.mockReset()
      mutate.mockResolvedValue({ txId: 1 })
    }
  })

  it(`marks the status the whole selection shares and bulk-writes a pick`, async () => {
    renderBar({
      issues: [
        buildIssue(),
        buildIssue({ id: `issue-2`, identifier: `APP-2`, number: 2 }),
      ],
    })

    openMenu(`Set status`)
    expect(items()).toHaveLength(3)
    expect(glyphs(`check`)).toHaveLength(1)
    expect(
      rowByText(`In Progress`).querySelector(`[data-selected-glyph=check]`)
    ).toBeTruthy()

    await act(async () => {
      fireEvent.click(rowByText(`Done`))
    })

    expect(mockState.bulkUpdate).toHaveBeenCalledTimes(1)
    expect(mockState.bulkUpdate).toHaveBeenCalledWith({
      issueIds: ISSUE_IDS,
      statusId: `status-done`,
    })
  })

  it(`marks nothing when the selected issues disagree on priority`, async () => {
    renderBar({
      issues: [
        buildIssue({ priority: `high` }),
        buildIssue({
          id: `issue-2`,
          identifier: `APP-2`,
          number: 2,
          priority: `low`,
        }),
      ],
    })

    openMenu(`Set priority`)
    expect(items()).toHaveLength(5)
    expect(glyphs(`check`)).toHaveLength(0)

    await act(async () => {
      fireEvent.click(rowByText(`Urgent`))
    })

    expect(mockState.bulkUpdate).toHaveBeenCalledWith({
      issueIds: ISSUE_IDS,
      priority: `urgent`,
    })
  })

  it(`marks the shared priority when both issues carry it`, () => {
    renderBar({
      issues: [
        buildIssue({ priority: `high` }),
        buildIssue({
          id: `issue-2`,
          identifier: `APP-2`,
          number: 2,
          priority: `high`,
        }),
      ],
    })

    openMenu(`Set priority`)
    expect(glyphs(`check`)).toHaveLength(1)
    expect(
      rowByText(`High`).querySelector(`[data-selected-glyph=check]`)
    ).toBeTruthy()
  })

  it(`marks Unassigned only while every selected issue is unassigned`, async () => {
    renderBar({
      issues: [
        buildIssue(),
        buildIssue({ id: `issue-2`, identifier: `APP-2`, number: 2 }),
      ],
    })

    openMenu(`Set assignee`)
    // The none row + the two members.
    expect(items()).toHaveLength(3)
    const none = items()[0]!
    expect(none.getAttribute(`data-combobox-none`)).toBe(`true`)
    expect(none.textContent).toContain(`Unassigned`)
    expect(glyphs(`check`)).toHaveLength(1)
    expect(none.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()

    await act(async () => {
      fireEvent.click(rowByText(`Bob Smith`))
    })
    expect(mockState.bulkUpdate).toHaveBeenCalledWith({
      issueIds: ISSUE_IDS,
      assigneeId: `user-2`,
    })
  })

  it(`marks nothing — not even Unassigned — on a mixed assignee selection`, async () => {
    renderBar({
      issues: [
        buildIssue({ assigneeId: `user-1` }),
        buildIssue({ id: `issue-2`, identifier: `APP-2`, number: 2 }),
      ],
    })

    openMenu(`Set assignee`)
    expect(glyphs(`check`)).toHaveLength(0)

    // The none row is still a real pick: it unassigns the whole selection.
    await act(async () => {
      fireEvent.click(items()[0]!)
    })
    expect(mockState.bulkUpdate).toHaveBeenCalledWith({
      issueIds: ISSUE_IDS,
      assigneeId: null,
    })
  })

  it(`draws the label tri-state and adds or removes from the whole selection`, async () => {
    // Bug on both issues, Ops on one, Docs on neither.
    const issueLabelMap = new Map<string, Label[]>([
      [`issue-1`, [labels[0]!, labels[1]!]],
      [`issue-2`, [labels[0]!]],
    ])
    renderBar({
      issues: [
        buildIssue(),
        buildIssue({ id: `issue-2`, identifier: `APP-2`, number: 2 }),
      ],
      issueLabelMap,
    })

    openMenu(`Set labels`)
    expect(items()).toHaveLength(3)
    expect(
      rowByText(`Bug`).querySelector(`[data-selected-glyph=selected]`)
    ).toBeTruthy()
    expect(
      rowByText(`Ops`).querySelector(`[data-selected-glyph=indeterminate]`)
    ).toBeTruthy()
    expect(
      rowByText(`Docs`).querySelector(`[data-selected-glyph=unselected]`)
    ).toBeTruthy()

    // "On some" joins: add it to every selected issue. The multi arm keeps the
    // menu open, so the sweep continues in the same visit.
    await act(async () => {
      fireEvent.click(rowByText(`Ops`))
    })
    expect(mockState.bulkAdd).toHaveBeenCalledWith({
      labelId: `label-2`,
      issueIds: ISSUE_IDS,
    })
    expect(items()).toHaveLength(3)

    // "On all" leaves: remove it from every selected issue.
    await act(async () => {
      fireEvent.click(rowByText(`Bug`))
    })
    expect(mockState.bulkRemove).toHaveBeenCalledWith({
      labelId: `label-1`,
      issueIds: ISSUE_IDS,
    })
    expect(mockState.bulkAdd).toHaveBeenCalledTimes(1)
  })

  it(`shows one disabled row when the team has no labels yet`, () => {
    render(
      <BulkActionBar
        issues={[buildIssue()]}
        issueLabelMap={new Map()}
        labels={[]}
        users={users}
        teamId="team-1"
        onClear={() => {}}
      />
    )

    openMenu(`Set labels`)
    expect(items()).toHaveLength(1)
    expect(items()[0]!.textContent).toBe(`No labels yet`)
    expect(items()[0]!.getAttribute(`data-disabled`)).toBe(``)
  })
})
