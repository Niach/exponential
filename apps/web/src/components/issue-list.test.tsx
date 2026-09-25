import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue } from "@/db/schema"
import type { IssueGroup } from "@/lib/board-view"
import type { StatusRowOption } from "@/lib/team-statuses"
import { IssueList } from "@/components/issue-list"

// Importing the real modules opens a tRPC client.
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { issues: { update: { mutate: vi.fn(async () => ({})) } } },
}))
// The row's three dropdowns are scenery here: this test is about the row
// GRID, not the cells' own behaviour (the context menu is the layout's host,
// EXP-1074 — a row only carries its attribute).
vi.mock(`@/components/issue-properties/status-dropdown`, () => ({
  StatusDropdown: () => null,
  // The group header above the rows reaches for this one.
  statusColorClass: () => ``,
}))
vi.mock(`@/components/issue-properties/priority-dropdown`, () => ({
  PriorityDropdown: () => null,
}))
vi.mock(`@/components/issue-properties/assignee-picker`, () => ({
  AssigneePicker: () => null,
}))

const status: StatusRowOption = {
  id: `builtin:backlog`,
  name: `Backlog`,
  colorHex: `#888888`,
  category: `backlog`,
  builtinKey: `backlog`,
  sortOrder: 0,
  icon: `circle-dashed`,
}

const makeIssue = (id: string, dueDate: string | null): Issue =>
  ({
    id,
    identifier: id.toUpperCase(),
    teamId: `t1`,
    boardId: `b1`,
    title: `Do the thing`,
    status: `backlog`,
    statusId: null,
    priority: `none`,
    assigneeId: null,
    dueDate,
  }) as unknown as Issue

const groupOf = (...issues: Issue[]): IssueGroup[] => [{ issues, status }]

function renderList(groups: IssueGroup[]) {
  const { container } = render(
    <IssueList
      groups={groups}
      issueLabelMap={new Map()}
      users={[]}
      userMap={new Map()}
      onNewIssue={() => {}}
      onIssueClick={() => {}}
    />
  )
  return container.querySelector(`[data-issue-rail-root]`) as HTMLElement
}

describe(`IssueList due-date column`, () => {
  // EXP-1023: a column nobody fills was a wide empty band to the right of the
  // label chips. The rule is per LIST, not per row.
  it(`collapses when no issue in the list has a due date`, () => {
    const list = renderList(groupOf(makeIssue(`a`, null), makeIssue(`b`, null)))
    expect(list.style.getPropertyValue(`--issue-due`)).toBe(`0px`)
  })

  it(`keeps the column as soon as one issue has a due date`, () => {
    const list = renderList(
      groupOf(makeIssue(`a`, null), makeIssue(`b`, `2026-01-01`))
    )
    // Unset — the grid template's own `4.5rem` fallback applies, so dated and
    // dateless rows keep their dates in one column.
    expect(list.style.getPropertyValue(`--issue-due`)).toBe(``)
  })

  it(`counts issues "Show more" hides, so revealing them never reflows the list`, () => {
    // 101 rows: the last one is behind the group cap and is the only one with
    // a date.
    const issues = Array.from({ length: 101 }, (_, i) =>
      makeIssue(`i${i}`, i === 100 ? `2026-01-01` : null)
    )
    const list = renderList(groupOf(...issues))
    expect(list.style.getPropertyValue(`--issue-due`)).toBe(``)
  })
})
