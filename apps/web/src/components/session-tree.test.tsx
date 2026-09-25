import * as React from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"
import { SessionTree, type TreeListRow } from "@/components/session-tree"

// EXP-996: the DRAWN tree. The RULE is `lib/sessions/session-tree.ts` and its
// own table; this is about what a reader sees — a group row per workflow and
// per stack, the workflow's name linking onward, and folding a group taking
// its runs with it.

// Importing the real modules opens Electric shapes / a tRPC client.
vi.mock(`@/lib/collections`, () => ({
  deviceCollection: {},
  issueCollection: {},
  workflowCollection: {},
  workflowNodeCollection: {},
  teamCollection: {},
  boardCollection: {},
  codingSessionCollection: {},
}))
vi.mock(`@tanstack/react-db`, () => ({
  useLiveQuery: () => ({ data: [] }),
  eq: () => undefined,
  inArray: () => undefined,
}))
vi.mock(`@tanstack/react-router`, () => ({
  useParams: () => ({ teamSlug: `acme` }),
  Link: ({
    children,
    params,
  }: {
    children: React.ReactNode
    params?: { workflowId?: string }
  }) => <a href={`/workflows/${params?.workflowId ?? ``}`}>{children}</a>,
}))
// The workflow/stack context is the point, so it is handed in rather than
// synced: these two hooks are the only door to the collections.
const workflows = [{ id: `w1`, name: `EXP-996 +5`, status: `running` }]
const nodes = [
  { workflowId: `w1`, issueId: `i1`, sessionId: null },
  { workflowId: `w1`, issueId: `i2`, sessionId: null },
]
vi.mock(`@/hooks/use-workflows`, () => ({
  useTeamWorkflows: () => workflows,
  useTeamWorkflowNodes: () => nodes,
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
}))
vi.mock(`@/hooks/use-now`, () => ({ useNow: () => Date.now() }))

const issue = (id: string, over: Partial<Issue> = {}): Issue =>
  ({
    id,
    identifier: id.toUpperCase(),
    title: `Issue ${id}`,
    boardId: `b1`,
    branch: null,
    prBaseBranch: null,
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
  it(`draws one group row per workflow, its name linking to the workflow`, () => {
    const i1 = issue(`i1`)
    const i2 = issue(`i2`)
    draw([
      row(`n1`, { issueId: `i1` }, { issue: i1 }),
      row(`n2`, { issueId: `i2` }, { issue: i2 }),
    ])
    expect(screen.getByTestId(`session-group-workflow:w1`)).toBeTruthy()
    expect(
      screen.getByRole(`link`, { name: `EXP-996 +5` }).getAttribute(`href`)
    ).toBe(`/workflows/w1`)
    // Both node runs sit under it, and the group says how many.
    expect(screen.getByTestId(`session-row-I1`)).toBeTruthy()
    expect(screen.getByTestId(`session-row-I2`)).toBeTruthy()
  })

  it(`draws a stack under one group row, and never links it`, () => {
    const low = issue(`s1`, { branch: `exp/S1` })
    const top = issue(`s2`, { branch: `exp/S2`, prBaseBranch: `exp/S1` })
    draw([
      row(`r1`, { issueId: `s1` }, { issue: low }),
      row(`r2`, { issueId: `s2` }, { issue: top }),
    ])
    expect(screen.getByTestId(`session-group-stack:s1`)).toBeTruthy()
    expect(screen.queryAllByRole(`link`)).toHaveLength(0)
  })

  it(`folds a group away with its runs`, () => {
    draw([
      row(`n1`, { issueId: `i1` }, { issue: issue(`i1`) }),
      row(`n2`, { issueId: `i2` }, { issue: issue(`i2`) }),
    ])
    fireEvent.click(screen.getByLabelText(`Collapse these runs`))
    expect(screen.getByTestId(`session-group-workflow:w1`)).toBeTruthy()
    expect(screen.queryByTestId(`session-row-I1`)).toBeNull()
    expect(screen.getByLabelText(`Expand these runs`)).toBeTruthy()
  })

  it(`leaves an ungrouped run at top level, with no group row`, () => {
    draw([row(`lone`)])
    expect(screen.queryByTestId(/^session-group-/)).toBeNull()
    expect(screen.getByTestId(`session-row-lone`)).toBeTruthy()
  })

  it(`renders the empty note when nothing is listed`, () => {
    render(<SessionTree rows={[]} onOpen={() => {}} emptyNote="Nothing yet." />)
    expect(screen.getByText(`Nothing yet.`)).toBeTruthy()
  })
})
