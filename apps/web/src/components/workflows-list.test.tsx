import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { SyncedWorkflow } from "@/db/schema"
import {
  WORKFLOWS_EMPTY_BODY,
  WORKFLOWS_EMPTY_TITLE,
} from "@/lib/workflow-view"

// EXP-981: the Workflows list — three bands in contract order, empty bands
// hidden, the shape line as the secondary text and a warning glyph on a
// cyclic graph. The banding rule itself is `lib/workflow-view.ts`; this only
// proves the page reads it.

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({ children, ...rest }: { children: React.ReactNode }) => (
    <a {...rest}>{children}</a>
  ),
}))

import { WorkflowsList } from "@/components/workflows-list"

const workflow = (
  id: string,
  over: Partial<SyncedWorkflow> = {}
): SyncedWorkflow =>
  ({
    id,
    teamId: `t1`,
    name: `Workflow ${id}`,
    status: `draft`,
    metrics: { nodes: 3, edges: 2, depth: 2, width: 2, cycles: [] },
    ...over,
  }) as unknown as SyncedWorkflow

describe(`WorkflowsList`, () => {
  it(`says what a workflow is when the team has none`, () => {
    render(<WorkflowsList workflows={[]} teamSlug="acme" />)
    expect(screen.getByText(WORKFLOWS_EMPTY_TITLE)).toBeTruthy()
    expect(screen.getByText(WORKFLOWS_EMPTY_BODY)).toBeTruthy()
    expect(screen.queryByTestId(`workflows-list`)).toBeNull()
  })

  it(`bands Running, Draft and Done in order and hides the empty ones`, () => {
    render(
      <WorkflowsList
        workflows={[
          workflow(`w1`, { status: `paused` }),
          workflow(`w2`, { status: `draft` }),
          workflow(`w3`, { status: `running` }),
        ]}
        teamSlug="acme"
      />
    )
    const bands = screen
      .getAllByText(/^(Running|Draft|Done)$/)
      .map((node) => node.textContent)
    expect(bands).toEqual([`Running`, `Draft`])
    // A paused workflow is still a running one (`workflowBand`).
    expect(screen.getByTestId(`workflow-row-w1`)).toBeTruthy()
  })

  it(`draws the shape line and warns about a cycle`, () => {
    render(
      <WorkflowsList
        workflows={[
          workflow(`w1`),
          workflow(`w2`, {
            metrics: {
              nodes: 1,
              edges: 1,
              depth: 1,
              width: 1,
              cycles: [[`APP-1`, `APP-2`]],
            },
          }),
        ]}
        teamSlug="acme"
      />
    )
    expect(screen.getByText(`3 nodes · depth 2 · width 2`)).toBeTruthy()
    expect(screen.getByText(`1 node · depth 1 · width 1`)).toBeTruthy()
    expect(screen.queryByTestId(`workflow-row-w1-cycle`)).toBeNull()
    expect(screen.getByTestId(`workflow-row-w2-cycle`)).toBeTruthy()
  })

  // EXP-982: the shape line alone cannot tell a paused workflow from a running
  // one (both band under Running), nor a cancelled one from a finished one.
  it(`leads the subtitle with the status a band does not tell apart`, () => {
    render(
      <WorkflowsList
        workflows={[
          workflow(`w1`, { status: `running` }),
          workflow(`w2`, { status: `paused` }),
          workflow(`w3`, { status: `done` }),
          workflow(`w4`, { status: `cancelled` }),
        ]}
        teamSlug="acme"
      />
    )
    const subtitle = (id: string) =>
      screen.getByTestId(`workflow-row-${id}`).textContent
    expect(subtitle(`w1`)).toContain(`3 nodes · depth 2 · width 2`)
    expect(subtitle(`w1`)).not.toContain(`Running ·`)
    expect(subtitle(`w2`)).toContain(`Paused · 3 nodes · depth 2 · width 2`)
    expect(subtitle(`w3`)).toContain(`3 nodes · depth 2 · width 2`)
    expect(subtitle(`w4`)).toContain(`Cancelled · 3 nodes · depth 2 · width 2`)
  })

  it(`rows link to the workflow's detail`, () => {
    render(<WorkflowsList workflows={[workflow(`w1`)]} teamSlug="acme" />)
    const row = screen.getByTestId(`workflow-row-w1`)
    expect(row.textContent).toContain(`Workflow w1`)
  })
})
