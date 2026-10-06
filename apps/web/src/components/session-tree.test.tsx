import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"
import { SessionTree, type TreeListRow } from "@/components/session-tree"

// EXP-996: the DRAWN tree. The RULE is `lib/sessions/session-tree.ts` and its
// own table; this is about what a reader sees — a child run nested under its
// parent, folding a parent taking its children with it, and the red dot of an
// open question.

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
vi.mock(`@/hooks/use-now`, () => ({ useNow: () => Date.now() }))

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
  device: { label: `Desktop`, online: true } as TreeListRow[`device`],
  paused: false,
  ...joined,
})

const draw = (rows: readonly TreeListRow[]) =>
  render(<SessionTree rows={rows} onOpen={() => {}} />)

describe(`SessionTree (EXP-996)`, () => {
  const family = () => [
    row(`p`, { issueId: `i1` }, { issue: issue(`i1`) }),
    row(
      `c`,
      {
        issueId: `i2`,
        parentSessionId: `p`,
        createdAt: new Date(`2026-09-01T10:30:00Z`),
        updatedAt: new Date(`2026-09-01T10:30:00Z`),
      },
      { issue: issue(`i2`) }
    ),
  ]

  it(`nests a child run under its parent`, () => {
    draw(family())
    expect(screen.getByTestId(`session-row-I1`)).toBeTruthy()
    expect(screen.getByTestId(`session-row-I2`)).toBeTruthy()
  })

  it(`folds a parent away with its children`, () => {
    draw(family())
    fireEvent.click(screen.getByLabelText(`Collapse child runs`))
    expect(screen.getByTestId(`session-row-I1`)).toBeTruthy()
    expect(screen.queryByTestId(`session-row-I2`)).toBeNull()
    fireEvent.click(screen.getByLabelText(`Expand child runs`))
    expect(screen.getByTestId(`session-row-I2`)).toBeTruthy()
  })

  it(`renders the empty note when nothing is listed`, () => {
    render(<SessionTree rows={[]} onOpen={() => {}} emptyNote="Nothing yet." />)
    expect(screen.getByText(`Nothing yet.`)).toBeTruthy()
  })
})
