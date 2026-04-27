import type { ReactNode } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { IssueRowContextMenu } from "@/components/issue-row-context-menu"
import type { Issue, Label, User } from "@/db/schema"

const mockState = vi.hoisted(() => ({
  addLabelMutate: vi.fn(),
  clipboardWriteText: vi.fn(),
  deleteMutate: vi.fn(),
  removeLabelMutate: vi.fn(),
  updateMutate: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issueLabels: {
      add: {
        mutate: mockState.addLabelMutate,
      },
      remove: {
        mutate: mockState.removeLabelMutate,
      },
    },
    issues: {
      delete: {
        mutate: mockState.deleteMutate,
      },
      update: {
        mutate: mockState.updateMutate,
      },
    },
  },
}))

vi.mock(`@/components/ui/context-menu`, () => ({
  ContextMenu: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ContextMenuTrigger: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  ContextMenuContent: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  ContextMenuLabel: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  ContextMenuSeparator: () => <hr />,
  ContextMenuSub: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  ContextMenuSubContent: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  ContextMenuSubTrigger: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  ContextMenuShortcut: ({ children }: { children: ReactNode }) => (
    <span>{children}</span>
  ),
  ContextMenuGroup: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  ContextMenuRadioGroup: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  ContextMenuItem: ({
    children,
    disabled,
    onSelect,
  }: {
    children: ReactNode
    disabled?: boolean
    onSelect?: (event: Event) => void
  }) => (
    <button
      type="button"
      disabled={disabled}
      onClick={() => onSelect?.({ preventDefault() {} } as Event)}
    >
      {children}
    </button>
  ),
  ContextMenuRadioItem: ({
    children,
    onSelect,
  }: {
    children: ReactNode
    onSelect?: (event: Event) => void
  }) => (
    <button
      type="button"
      onClick={() => onSelect?.({ preventDefault() {} } as Event)}
    >
      {children}
    </button>
  ),
  ContextMenuCheckboxItem: ({
    children,
    onSelect,
  }: {
    children: ReactNode
    onSelect?: (event: Event) => void
  }) => (
    <button
      type="button"
      onClick={() => onSelect?.({ preventDefault() {} } as Event)}
    >
      {children}
    </button>
  ),
}))

function buildIssue(overrides: Partial<Issue> = {}): Issue {
  return {
    id: `issue-1`,
    projectId: `project-1`,
    number: 1,
    identifier: `APP-1`,
    title: `Ship custom context menu`,
    description: null,
    status: `backlog`,
    priority: `none`,
    assigneeId: null,
    creatorId: `user-1`,
    dueDate: null,
    sortOrder: 0,
    completedAt: null,
    archivedAt: null,
    recurrenceInterval: null,
    recurrenceUnit: null,
    googleCalendarEventId: null,
    googleCalendarLastSyncedAt: null,
    googleCalendarLastSyncError: null,
    createdAt: new Date(`2026-03-07T09:00:00Z`),
    updatedAt: new Date(`2026-03-07T09:00:00Z`),
    ...overrides,
  }
}

const users: User[] = [
  {
    id: `user-1`,
    name: `Alice Doe`,
    email: `alice@example.com`,
    emailVerified: true,
    image: null,
    createdAt: new Date(`2026-03-01T00:00:00Z`),
    updatedAt: new Date(`2026-03-01T00:00:00Z`),
  },
  {
    id: `user-2`,
    name: `Bob Smith`,
    email: `bob@example.com`,
    emailVerified: true,
    image: null,
    createdAt: new Date(`2026-03-01T00:00:00Z`),
    updatedAt: new Date(`2026-03-01T00:00:00Z`),
  },
]

const labels: Label[] = [
  {
    id: `label-1`,
    workspaceId: `workspace-1`,
    name: `Bug`,
    color: `#ef4444`,
    sortOrder: 0,
    createdAt: new Date(`2026-03-01T00:00:00Z`),
    updatedAt: new Date(`2026-03-01T00:00:00Z`),
  },
  {
    id: `label-2`,
    workspaceId: `workspace-1`,
    name: `Ops`,
    color: `#3b82f6`,
    sortOrder: 1,
    createdAt: new Date(`2026-03-01T00:00:00Z`),
    updatedAt: new Date(`2026-03-01T00:00:00Z`),
  },
]

describe(`IssueRowContextMenu`, () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date(`2026-03-07T09:00:00Z`))
    mockState.addLabelMutate.mockReset()
    mockState.clipboardWriteText.mockReset()
    mockState.deleteMutate.mockReset()
    mockState.removeLabelMutate.mockReset()
    mockState.updateMutate.mockReset()
    mockState.addLabelMutate.mockResolvedValue({})
    mockState.clipboardWriteText.mockResolvedValue(undefined)
    mockState.deleteMutate.mockResolvedValue({})
    mockState.removeLabelMutate.mockResolvedValue({})
    mockState.updateMutate.mockResolvedValue({})

    Object.defineProperty(window.navigator, `clipboard`, {
      configurable: true,
      value: {
        writeText: mockState.clipboardWriteText,
      },
    })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it(`runs quick actions for opening the issue and copying the issue id only`, async () => {
    const onOpenIssue = vi.fn()

    render(
      <IssueRowContextMenu
        issue={buildIssue()}
        issueLabels={[]}
        labels={labels}
        users={users}
        userMap={new Map(users.map((user) => [user.id, user]))}
        onOpenIssue={onOpenIssue}
      >
        <div>Issue row</div>
      </IssueRowContextMenu>
    )

    fireEvent.click(screen.getByText(`Open issue`))
    fireEvent.click(screen.getByText(`Copy issue ID`))

    await Promise.resolve()
    await Promise.resolve()

    expect(onOpenIssue).toHaveBeenCalledTimes(1)
    expect(mockState.clipboardWriteText).toHaveBeenCalledTimes(1)
    expect(mockState.clipboardWriteText).toHaveBeenCalledWith(`APP-1`)
    expect(screen.queryByText(`Copy title`)).toBeNull()
  })

  it(`runs quick-edit mutations for menu actions`, async () => {
    render(
      <IssueRowContextMenu
        issue={buildIssue({ assigneeId: `user-1` })}
        issueLabels={[labels[0]]}
        labels={labels}
        users={users}
        userMap={new Map(users.map((user) => [user.id, user]))}
        onOpenIssue={vi.fn()}
      >
        <div>Issue row</div>
      </IssueRowContextMenu>
    )

    fireEvent.click(screen.getByText(`Mark as done`))
    fireEvent.click(screen.getByText(`In Progress`))
    fireEvent.click(screen.getByText(`High`))
    fireEvent.click(screen.getByText(`Unassigned`))
    fireEvent.click(screen.getByText(`Bob Smith`))
    fireEvent.click(screen.getByText(`Bug`))
    fireEvent.click(screen.getByText(`Ops`))
    fireEvent.click(screen.getByText(`Tomorrow`))

    await Promise.resolve()
    await Promise.resolve()

    expect(mockState.updateMutate).toHaveBeenCalledWith({
      id: `issue-1`,
      status: `done`,
    })
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      id: `issue-1`,
      status: `in_progress`,
    })
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      id: `issue-1`,
      priority: `high`,
    })
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      id: `issue-1`,
      assigneeId: null,
    })
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      id: `issue-1`,
      assigneeId: `user-2`,
    })
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      id: `issue-1`,
      dueDate: `2026-03-08`,
    })
    expect(mockState.removeLabelMutate).toHaveBeenCalledWith({
      issueId: `issue-1`,
      labelId: `label-1`,
    })
    expect(mockState.addLabelMutate).toHaveBeenCalledWith({
      issueId: `issue-1`,
      labelId: `label-2`,
    })
  })

  it(`deletes the issue when confirm delete is selected`, async () => {
    render(
      <IssueRowContextMenu
        issue={buildIssue()}
        issueLabels={[]}
        labels={labels}
        users={users}
        userMap={new Map(users.map((user) => [user.id, user]))}
        onOpenIssue={vi.fn()}
      >
        <div>Issue row</div>
      </IssueRowContextMenu>
    )

    fireEvent.click(screen.getByText(`Confirm delete`))

    await Promise.resolve()
    await Promise.resolve()

    expect(mockState.deleteMutate).toHaveBeenCalledTimes(1)
    expect(mockState.deleteMutate).toHaveBeenCalledWith({ id: `issue-1` })
  })
})
