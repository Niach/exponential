import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Board, Issue } from "@/db/schema"
import type { IssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import { IssuePropertiesTray } from "@/components/issue-properties-tray"
import { REVIEW_MERGES_THROUGH_WORKFLOW } from "@/lib/reviews-merge"

// EXP-1094: the tray's Merge pill on a workflow NODE PR. The node merges
// through its workflow (the server refuses the row merge, PR #864), so the
// pill gives way to the Reviews row's quiet caption while the workflow is
// running or paused, and comes back once it is done.

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/components/agent-session`, () => ({
  useSteerConfig: () => ({ enabled: true }),
}))
vi.mock(`@/hooks/use-team-statuses`, () => ({
  useTeamStatusesContext: () => ({
    resolve: () => ({ id: `s1`, name: `In progress`, category: `started` }),
  }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamById: () => null }))
vi.mock(`@/hooks/use-agents-data`, () => ({
  mergeTargetProps: () => ({}),
}))
vi.mock(`@/components/issue-coding-rows`, () => ({
  useIsTeamMember: () => true,
}))
vi.mock(`@/components/issue-coding-action`, () => ({
  IssueCodingAction: () => <div data-testid="coding-action" />,
}))
vi.mock(`@/components/issue-properties-panel`, () => ({
  IssuePropertiesPanel: () => <div data-testid="properties-panel" />,
}))
vi.mock(`@/components/run-action-pills`, () => ({
  MergePrPill: () => <button type="button">Merge PR</button>,
}))
const workflowState = {
  workflows: [] as { id: string; status: string; teamId: string }[],
  nodes: [] as {
    workflowId: string
    issueId: string
    memberIssueIds: string[]
    teamId: string
  }[],
}
vi.mock(`@/hooks/use-workflows`, () => ({
  useTeamWorkflows: () => workflowState.workflows,
  useTeamWorkflowNodes: () => workflowState.nodes,
}))

const issue = {
  id: `i1`,
  identifier: `MET-12`,
  teamId: `t1`,
  boardId: `b1`,
  title: `Do the thing`,
  prState: `open`,
  prNumber: 7,
  prUrl: `https://github.com/o/r/pull/7`,
  branch: `exp/MET-12`,
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

function renderTray() {
  return render(
    <IssuePropertiesTray
      issue={issue}
      board={board}
      users={[]}
      teamId="t1"
      currentUserId="u1"
      handlers={handlers}
    />
  )
}

describe(`IssuePropertiesTray`, () => {
  it(`offers Merge on a plain open PR`, () => {
    workflowState.workflows = []
    workflowState.nodes = []
    renderTray()
    expect(screen.getByRole(`button`, { name: `Merge PR` })).toBeTruthy()
    expect(screen.queryByTestId(`merge-reason`)).toBeNull()
  })

  it(`swaps Merge for the workflow caption on a node PR of a live workflow`, () => {
    for (const status of [`running`, `paused`]) {
      workflowState.workflows = [{ id: `w1`, status, teamId: `t1` }]
      workflowState.nodes = [
        { workflowId: `w1`, issueId: `i1`, memberIssueIds: [], teamId: `t1` },
      ]
      const { unmount } = renderTray()
      expect(screen.queryByRole(`button`, { name: `Merge PR` })).toBeNull()
      expect(screen.getByTestId(`merge-reason`).textContent).toBe(
        REVIEW_MERGES_THROUGH_WORKFLOW
      )
      unmount()
    }
  })

  it(`offers Merge again once the workflow is done`, () => {
    workflowState.workflows = [{ id: `w1`, status: `done`, teamId: `t1` }]
    workflowState.nodes = [
      { workflowId: `w1`, issueId: `i1`, memberIssueIds: [], teamId: `t1` },
    ]
    renderTray()
    expect(screen.getByRole(`button`, { name: `Merge PR` })).toBeTruthy()
    expect(screen.queryByTestId(`merge-reason`)).toBeNull()
  })
})
