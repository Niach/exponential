import { fireEvent, render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  sortWorkflowEvents,
  WorkflowEventList,
  workflowEventGlyph,
  workflowEventTime,
} from "./workflow-events"

// EXP-1064: the workflow event log, rendered.
const event = (id: string, kind: string, at: string, over: Record<string, unknown> = {}) => ({
  id,
  kind,
  message: `${kind} happened`,
  at,
  ...over,
})

describe(`WorkflowEventList`, () => {
  it(`renders nothing for an empty log`, () => {
    const { container } = render(<WorkflowEventList events={[]} />)
    expect(container.querySelector(`li`)).toBeNull()
  })

  it(`lists the newest first with the node's identifier and the time`, () => {
    const { container } = render(
      <WorkflowEventList
        events={[
          event(`a`, `node_started`, `2026-09-25T10:00:00Z`, { nodeId: `n1` }),
          event(`b`, `landed`, `2026-09-25T11:00:00Z`, { nodeId: `n1` }),
          event(`c`, `final_pr_opened`, `2026-09-25T12:00:00Z`),
        ]}
        nodeLabel={(id) => (id === `n1` ? `EXP-12` : null)}
      />
    )
    const rows = [...container.querySelectorAll(`li`)]
    expect(rows.map((row) => row.querySelector(`[data-kind]`)?.getAttribute(`data-kind`))).toEqual([
      `final_pr_opened`,
      `landed`,
      `node_started`,
    ])
    expect(rows[1]?.textContent).toContain(`EXP-12`)
    expect(rows[1]?.textContent).toContain(`landed happened`)
    expect(rows[0]?.textContent).not.toContain(`EXP-12`)
  })

  it(`filters to the picked nodes`, () => {
    const { container } = render(
      <WorkflowEventList
        events={[
          event(`a`, `node_started`, `2026-09-25T10:00:00Z`, { nodeId: `n1` }),
          event(`b`, `landed`, `2026-09-25T11:00:00Z`, { nodeId: `n2` }),
          event(`c`, `final_pr_opened`, `2026-09-25T12:00:00Z`),
        ]}
        nodeIds={[`n2`]}
      />
    )
    const kinds = [...container.querySelectorAll(`[data-kind]`)].map((row) => row.getAttribute(`data-kind`))
    expect(kinds).toEqual([`landed`])
  })

  it(`opens the run behind a row that names one`, () => {
    const open = vi.fn()
    const { getByTestId } = render(
      <WorkflowEventList
        events={[event(`a`, `review_started`, `2026-09-25T10:00:00Z`, { sessionId: `s1` })]}
        onOpenSession={open}
      />
    )
    fireEvent.click(getByTestId(`workflow-event-a`))
    expect(open).toHaveBeenCalledWith(`s1`)
  })
})

describe(`sortWorkflowEvents`, () => {
  it(`orders by time desc, then id desc`, () => {
    const sorted = sortWorkflowEvents([
      event(`a`, `landed`, `2026-09-25T10:00:00Z`),
      event(`b`, `landed`, `2026-09-25T10:00:00Z`),
      event(`c`, `landed`, `2026-09-25T09:00:00Z`),
    ])
    expect(sorted.map((row) => row.id)).toEqual([`b`, `a`, `c`])
  })
})

describe(`workflowEventGlyph`, () => {
  it(`maps every contract kind to a concept and falls back to info`, () => {
    expect(workflowEventGlyph(`failed`)).toBe(`ui-warning`)
    expect(workflowEventGlyph(`landed`)).toBe(`ui-check`)
    expect(workflowEventGlyph(`something-new`)).toBe(`ui-info`)
  })
})

describe(`workflowEventTime`, () => {
  it(`is HH:mm, empty for garbage`, () => {
    expect(workflowEventTime(new Date(2026, 8, 25, 9, 5))).toBe(`09:05`)
    expect(workflowEventTime(`not a date`)).toBe(``)
  })
})
