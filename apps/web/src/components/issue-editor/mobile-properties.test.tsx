// EXP-1170: Labels is a PROPERTY ROW on the phone sheet (×3 phones) — the
// picks joined in the team's sort order, "None" when empty, and a tap opens
// the shared picker. The old cloud of toggle chips must not come back.
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Label } from "@/db/schema"
import type { StatusRowOption } from "@/lib/team-statuses"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

// Already in the team's sort order, as the live query returns them.
const LABELS = [
  { id: `label-1`, teamId: `team-1`, name: `bug`, color: `#ef4444`, sortOrder: 1 },
  { id: `label-2`, teamId: `team-1`, name: `chore`, color: `#22c55e`, sortOrder: 2 },
  { id: `label-3`, teamId: `team-1`, name: `mobile`, color: `#3b82f6`, sortOrder: 3 },
] as unknown as Label[]

const STATUS: StatusRowOption = {
  id: `status-1`,
  name: `Backlog`,
  colorHex: `#71717a`,
  category: `backlog`,
  builtinKey: `backlog`,
  sortOrder: 0,
  icon: `circle-dashed`,
}

vi.mock(`@/lib/collections`, () => ({ labelCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: LABELS }) }
})
vi.mock(`@/hooks/use-team-statuses`, () => ({
  useTeamStatusesContext: () => ({ options: [], byId: new Map() }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamBoards: () => [] }))
vi.mock(`@/components/issue-relations-card`, () => ({
  MobileRelationBands: () => null,
}))

const { IssueEditorMobileProperties } = await import(
  `@/components/issue-editor/mobile-properties`
)

const renderSheet = (selectedLabelIds: string[]) =>
  render(
    <IssueEditorMobileProperties
      status={STATUS}
      priority="none"
      assigneeId={null}
      selectedLabelIds={selectedLabelIds}
      teamId="team-1"
      users={[]}
      dueDate={null}
      onStatusChange={vi.fn()}
      onPriorityChange={vi.fn()}
      onAssigneeChange={vi.fn()}
      onToggleLabel={vi.fn()}
      onDueDateSelect={vi.fn()}
    />
  )

const labelsRow = () => screen.getByText(`Labels`).closest(`button`)!

describe(`IssueEditorMobileProperties`, () => {
  it(`Labels is a row between Assignee and Due date`, () => {
    renderSheet([])
    const order = Array.from(document.querySelectorAll(`button`)).map(
      (button) => button.querySelector(`span`)?.textContent
    )
    expect(order.slice(0, 5)).toEqual([
      `Status`,
      `Priority`,
      `Assignee`,
      `Labels`,
      `Due date`,
    ])
  })

  it(`shows the picks joined in sort order, or None`, () => {
    const { unmount } = renderSheet([`label-3`, `label-1`])
    expect(labelsRow().textContent).toContain(`bug, mobile`)
    unmount()
    renderSheet([])
    expect(labelsRow().textContent).toContain(`None`)
  })

  it(`renders no label chips and opens the shared picker on tap`, () => {
    renderSheet([`label-1`])
    // No cloud of toggle chips: `chore` only exists inside the picker.
    expect(screen.queryByText(`chore`)).toBeNull()
    fireEvent.click(labelsRow())
    expect(screen.getByText(`chore`)).toBeTruthy()
  })
})
