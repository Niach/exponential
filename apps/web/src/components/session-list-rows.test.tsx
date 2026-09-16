import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"
import type { SessionListRow } from "@/hooks/use-agents-data"

// EXP-897: the two session rows nest and FOLD the same way — the leading
// chevron is the only control that owns an accessible name, the trailing one
// on a past row is decoration, and a nested row indents 12 + 14·depth px.

vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})
vi.mock(`@/lib/collections`, () => ({
  codingSessionCollection: {},
  deviceCollection: {},
  issueCollection: {},
  teamMemberCollection: {},
  userCollection: {},
}))

import {
  PastSessionRow,
  RunningSessionRow,
} from "@/components/session-list-rows"

const session = (id: string): CodingSession =>
  ({
    id,
    status: `running`,
    agent: `claude`,
    startedAt: new Date(`2026-09-10T10:00:00Z`),
    updatedAt: new Date(),
    endedAt: null,
    needsInput: false,
    agentBusy: false,
    blocked: null,
    deviceLabel: `buildbox`,
    branch: null,
    prState: null,
  }) as unknown as CodingSession

const runningRow = (): SessionListRow => ({
  session: session(`s1`),
  issue: { identifier: `APP-1`, title: `Ship it` } as Issue,
  batchIssues: [],
  board: undefined,
  device: { label: `buildbox`, online: true },
  paused: false,
  mergeTarget: undefined,
})

describe(`RunningSessionRow`, () => {
  it(`names the fold chevron and indents by depth`, () => {
    const onToggle = vi.fn()
    render(
      <RunningSessionRow
        row={runningRow()}
        depth={2}
        expandable
        expanded
        onToggle={onToggle}
        onOpen={vi.fn()}
      />
    )
    const chevron = screen.getByLabelText(`Collapse child runs`)
    expect(chevron).toBeTruthy()
    fireEvent.click(chevron)
    expect(onToggle).toHaveBeenCalledTimes(1)
    const row = screen.getByTestId(`session-row-APP-1`) as HTMLElement
    expect(row.style.paddingLeft).toBe(`40px`)
  })

  it(`says Expand while it is collapsed, and folding never opens the run`, () => {
    const onOpen = vi.fn()
    render(
      <RunningSessionRow
        row={runningRow()}
        expandable
        expanded={false}
        onToggle={vi.fn()}
        onOpen={onOpen}
      />
    )
    fireEvent.click(screen.getByLabelText(`Expand child runs`))
    expect(onOpen).not.toHaveBeenCalled()
  })
})

describe(`PastSessionRow`, () => {
  it(`names the fold chevron and indents by depth`, () => {
    const onToggle = vi.fn()
    render(
      <PastSessionRow
        sessionId="s2"
        title="Ship it"
        identifier="APP-2"
        byline="buildbox · 2h ago"
        depth={1}
        expandable
        expanded={false}
        onToggle={onToggle}
        onOpen={vi.fn()}
      />
    )
    fireEvent.click(screen.getByLabelText(`Expand child runs`))
    expect(onToggle).toHaveBeenCalledTimes(1)
    const row = screen.getByTestId(`session-row-s2`) as HTMLElement
    expect(row.style.paddingLeft).toBe(`26px`)
    // The TRAILING chevron is decoration: exactly one named control.
    expect(screen.queryByLabelText(`Collapse child runs`)).toBeNull()
  })

  it(`carries no fold control at all without children`, () => {
    render(
      <PastSessionRow
        sessionId="s3"
        title="Ship it"
        identifier={null}
        byline=""
        onOpen={vi.fn()}
      />
    )
    expect(screen.queryByLabelText(`Expand child runs`)).toBeNull()
    expect(screen.queryByLabelText(`Collapse child runs`)).toBeNull()
  })
})
