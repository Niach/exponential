import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"
import type { SessionListRow } from "@/hooks/use-agents-data"

// EXP-897: the two session rows nest and FOLD the same way — the leading
// chevron is the only control that owns an accessible name, the trailing one
// on a past row is decoration, and a nested row indents 12 + 14·depth px.
// EXP-1208: every row LEADS with the shared run mark (never a dot), and a
// parent's fold chevron FOLLOWS it — so a parent's mark sits exactly where a
// standalone row's does.

/** The row's direct children, in order, minus the connector layer. */
function leadOf(row: HTMLElement): (string | null)[] {
  return Array.from(row.children)
    .filter((child) => child.tagName.toLowerCase() !== `svg`)
    .map((child) => child.getAttribute(`data-slot`))
}

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
  runningRowMarkState,
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

describe(`the run mark lead`, () => {
  it(`puts the mark FIRST and the chevron after it on a parent row`, () => {
    render(
      <RunningSessionRow
        row={runningRow()}
        expandable
        expanded
        onToggle={vi.fn()}
        onOpen={vi.fn()}
      />
    )
    const row = screen.getByTestId(`session-row-APP-1`) as HTMLElement
    expect(leadOf(row).slice(0, 2)).toEqual([`run-mark`, `session-row-fold`])
  })

  it(`leads a standalone row with the mark at the same inset`, () => {
    render(<RunningSessionRow row={runningRow()} onOpen={vi.fn()} />)
    const row = screen.getByTestId(`session-row-APP-1`) as HTMLElement
    expect(leadOf(row)[0]).toBe(`run-mark`)
    expect(row.style.paddingLeft).toBe(`12px`)
    expect(row.querySelector(`[data-slot="session-row-fold"]`)).toBeNull()
  })

  it(`wears the working spark while the agent works, a badge when parked`, () => {
    const busy = runningRow()
    busy.session = { ...busy.session, agentBusy: true }
    const { unmount } = render(<RunningSessionRow row={busy} onOpen={vi.fn()} />)
    const mark = screen
      .getByTestId(`session-row-APP-1`)
      .querySelector(`[data-slot="run-mark"]`)!
    expect(mark.getAttribute(`data-state`)).toBe(`working`)
    expect(mark.querySelector(`[data-slot="run-mark-badge"]`)).toBeNull()
    unmount()

    const parked = runningRow()
    parked.session = { ...parked.session, needsInput: true }
    render(<RunningSessionRow row={parked} onOpen={vi.fn()} />)
    const parkedMark = screen
      .getByTestId(`session-row-APP-1`)
      .querySelector(`[data-slot="run-mark"]`)!
    expect(parkedMark.getAttribute(`data-state`)).toBe(`needs_input`)
    expect(
      parkedMark.querySelector(`[data-slot="run-mark-badge"]`)
    ).not.toBeNull()
  })

  it(`maps the display state onto the mark`, () => {
    expect(
      runningRowMarkState(`review`, { paused: true, working: false })
    ).toBeUndefined()
    expect(
      runningRowMarkState(`working`, { paused: false, working: true })
    ).toBe(`working`)
    expect(
      runningRowMarkState(`working`, { paused: false, working: false })
    ).toBeUndefined()
    expect(
      runningRowMarkState(`done`, { paused: false, working: false })
    ).toBe(`done`)
  })

  it(`dims an ended row's mark with no badge, ahead of its chevron`, () => {
    render(
      <PastSessionRow
        sessionId="s9"
        agent="claude"
        title="Ship it"
        identifier={null}
        byline=""
        expandable
        onToggle={vi.fn()}
        onOpen={vi.fn()}
      />
    )
    const row = screen.getByTestId(`session-row-s9`) as HTMLElement
    expect(leadOf(row).slice(0, 2)).toEqual([`run-mark`, `session-row-fold`])
    const mark = row.querySelector(`[data-slot="run-mark"]`) as HTMLElement
    expect(mark.getAttribute(`data-state`)).toBe(`ended`)
    expect(mark.style.opacity).toBe(`0.5`)
    expect(mark.querySelector(`[data-slot="run-mark-badge"]`)).toBeNull()
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
        agent="claude"
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
        agent="codex"
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
