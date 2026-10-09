import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"
import { SessionTree, type TreeListRow } from "@/components/session-tree"

// EXP-996 / EXP-1248: the DRAWN tree. The RULE is `lib/sessions/session-tree.ts`
// and its own table; this is what a reader sees: a child run nested under its
// parent (always shown, no fold), every row led by the run mark at the base
// inset, and the big row's caption.

// Importing the real modules opens Electric shapes / a tRPC client.
vi.mock(`@/lib/collections`, () => ({
  deviceCollection: {},
  issueCollection: {},
  teamCollection: {},
  boardCollection: {},
  codingSessionCollection: {},
}))
vi.mock(`@tanstack/react-db`, () => ({
  useLiveQuery: () => ({ data: [] }),
  eq: () => undefined,
  inArray: () => undefined,
}))
vi.mock(`@/hooks/use-now`, () => ({
  useNow: () => new Date(`2026-09-01T10:05:00Z`),
}))

const issue = (id: string, over: Partial<Issue> = {}): Issue =>
  ({
    id,
    identifier: id.toUpperCase(),
    title: `Issue ${id}`,
    boardId: `b1`,
    branch: null,
    prState: null,
    ...over,
  }) as Issue

const session = (id: string, over: Partial<CodingSession> = {}): CodingSession =>
  ({
    id,
    agent: `claude`,
    status: `running`,
    issueId: null,
    batchIssueIds: null,
    parentSessionId: null,
    resumedFromId: null,
    startedReason: null,
    blocked: null,
    needsInput: null,
    agentBusy: null,
    agentCaption: null,
    deviceId: `d1`,
    deviceLabel: `Desktop`,
    prUrl: null,
    prNumber: null,
    startedAt: new Date(`2026-09-01T10:00:00Z`),
    createdAt: new Date(`2026-09-01T10:00:00Z`),
    updatedAt: new Date(`2026-09-01T10:00:00Z`),
    endedAt: null,
    ...over,
  }) as CodingSession

const row = (
  id: string,
  over: Partial<CodingSession> = {},
  joined: Partial<TreeListRow> = {}
): TreeListRow => ({
  session: session(id, over),
  issue: undefined,
  board: undefined,
  device: { label: `mint`, online: true } as TreeListRow[`device`],
  paused: false,
  ...joined,
})

const firstSlot = (el: HTMLElement) =>
  Array.from(el.children)
    .find((child) => child.getAttribute(`data-testid`) !== `tree-guides`)
    ?.getAttribute(`data-slot`)

describe(`SessionTree`, () => {
  const family = () => [
    row(`p`, { issueId: `i1`, agentBusy: true }, { issue: issue(`i1`) }),
    row(
      `c`,
      {
        issueId: `i2`,
        parentSessionId: `p`,
        needsInput: true,
        createdAt: new Date(`2026-09-01T10:30:00Z`),
        updatedAt: new Date(`2026-09-01T10:30:00Z`),
      },
      { issue: issue(`i2`) }
    ),
  ]

  it(`nests a child run under its parent, always shown, with no fold control`, () => {
    render(<SessionTree rows={family()} onOpen={() => {}} />)
    const parent = screen.getByTestId(`session-row-I1`)
    const child = screen.getByTestId(`session-row-I2`)
    expect(parent.style.paddingLeft).toBe(`12px`)
    expect(child.style.paddingLeft).toBe(`26px`)
    expect(screen.queryByLabelText(`Collapse child runs`)).toBeNull()
    expect(firstSlot(parent)).toBe(`run-mark`)
    expect(firstSlot(child)).toBe(`run-mark`)
  })

  it(`captions a big row with its state, device and age`, () => {
    render(<SessionTree rows={family()} onOpen={() => {}} />)
    const caption = (id: string) =>
      screen.getByTestId(`session-row-${id}`).querySelector(`[data-slot="session-row-caption"]`)
    expect(caption(`I1`)?.textContent).toBe(`Building · mint · 5 min`)
    expect(caption(`I2`)?.textContent).toBe(`Needs input · mint · 5 min`)
    expect(caption(`I2`)?.className).toContain(`text-amber-400`)
  })

  it(`dims an ended row's mark and says Done since it ended`, () => {
    render(
      <SessionTree
        rows={[
          row(`e`, {
            status: `ended`,
            endedAt: new Date(`2026-09-01T09:05:00Z`),
          }),
        ]}
        onOpen={() => {}}
      />
    )
    const ended = screen.getByTestId(`session-row-e`)
    expect(ended.querySelector(`[data-slot="run-mark"]`)?.getAttribute(`data-state`)).toBe(`ended`)
    expect(ended.textContent).toContain(`Done · mint · 1 h`)
  })

  it(`renders one line in the small size`, () => {
    render(<SessionTree rows={family()} size="small" onOpen={() => {}} />)
    const parent = screen.getByTestId(`session-row-I1`)
    expect(parent.getAttribute(`data-session-row`)).toBe(`small`)
    expect(parent.querySelector(`[data-slot="session-row-caption"]`)).toBeNull()
  })

  it(`opens the run on click`, () => {
    const onOpen = vi.fn()
    render(<SessionTree rows={family()} onOpen={onOpen} />)
    fireEvent.click(screen.getByTestId(`session-row-I2`))
    expect(onOpen.mock.calls[0]![0].id).toBe(`c`)
  })

  it(`renders the empty note when nothing is listed`, () => {
    render(<SessionTree rows={[]} onOpen={() => {}} emptyNote="Nothing yet." />)
    expect(screen.getByText(`Nothing yet.`)).toBeTruthy()
  })
})
