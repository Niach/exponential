import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Board, Issue } from "@/db/schema"
import type { IssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import { IssuePropertiesTray } from "@/components/issue-properties-tray"

// EXP-1293: the tray is the properties alone: no Merge, no coding action.

vi.mock(`@/hooks/use-team-statuses`, () => ({
  useTeamStatusesContext: () => ({
    resolve: () => ({ id: `s1`, name: `In progress`, category: `started` }),
  }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamById: () => null }))
vi.mock(`@/components/issue-properties-panel`, () => ({
  IssuePropertiesPanel: () => <div data-testid="properties-panel" />,
}))
const issue = {
  id: `i1`,
  identifier: `MET-12`,
  boardId: `b1`,
  title: `Do the thing`,
  updatedAt: `2026-09-01T10:00:00.000Z`,
  priority: `none`,
  assigneeId: null,
  dueDate: null,
  estimate: null,
  source: `user`,
} as unknown as Issue
const board = {
  id: `b1`,
  slug: `met`,
  color: `#000`,
  prefix: `MET`,
  icon: `board`,
  repositoryId: `r1`,
} as unknown as Board
const handlers = { issueLabelIds: [] } as unknown as IssuePropertyHandlers

describe(`IssuePropertiesTray`, () => {
  it(`draws the properties and nothing that merges or starts`, () => {
    render(
      <IssuePropertiesTray
        issue={issue}
        board={board}
        users={[]}
        teamId="t1"
        handlers={handlers}
      />
    )
    expect(screen.getByTestId(`properties-panel`)).toBeTruthy()
    expect(screen.queryByRole(`button`)).toBeNull()
  })
})
