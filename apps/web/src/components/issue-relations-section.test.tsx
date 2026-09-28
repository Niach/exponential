import { fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import type { Issue, User } from "@/db/schema"

// EXP-1097 direction A — the grouped bands, drawn off the fixture-locked
// model (`lib/issue-relations-view.ts`). What this pins is the WIRING: the
// synced rows reach the model in their canonical direction, the answer draws
// as the Sub-issues band + foldable bands, the fold / "Show N more" controls
// feed the model back, the `+` opens the inline composer, and the phone sheet
// wears the same bands under "Relations".

const mocks = vi.hoisted(() => ({
  relations: [] as unknown[],
  refs: new Map<string, unknown>(),
  open: vi.fn(),
  deleteMutate: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    relations: {
      delete: { mutate: mocks.deleteMutate },
      create: { mutate: vi.fn() },
    },
    issues: { update: { mutate: vi.fn() } },
  },
}))
vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  issueRelationCollection: {},
}))
// Every query aliases its source: `relations` answers the test's rows,
// `issues` (the far issues' assignees) answers nothing.
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let alias = ``
      const chain = { where: () => chain }
      const probe = {
        from: (source: Record<string, unknown>) => {
          alias = Object.keys(source)[0] ?? ``
          return chain
        },
      }
      const result = build(probe)
      return {
        data: result === undefined || alias !== `relations` ? [] : mocks.relations,
      }
    },
  }
})
vi.mock(`@/components/issue-ref-provider`, () => ({
  useIssueRefs: () => ({
    teamId: `t1`,
    resolveById: (id: string) => mocks.refs.get(id) ?? null,
    open: mocks.open,
  }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamUsers: () => ({ userMap: new Map(), users: [], members: [] }),
}))
vi.mock(`@/hooks/use-team-statuses`, () => ({
  useTeamStatusesContext: () => ({
    resolve: () => ({
      id: `builtin:done`,
      builtinKey: `done`,
      icon: `circle-check`,
      colorHex: `#000`,
    }),
  }),
}))
vi.mock(`@/components/issue-properties/status-dropdown`, () => ({
  IssueStatusIcon: () => <span data-testid="status-glyph" />,
  statusColorClass: () => `text-done`,
}))
vi.mock(`@/components/issue-preview-card`, () => ({
  IssuePreviewHoverCard: ({ children }: { children: React.ReactNode }) => children,
}))
vi.mock(`@/components/issue-chip`, () => ({
  IssueChip: ({ issue }: { issue: { identifier: string } }) => (
    <span data-testid={`chip-${issue.identifier}`} />
  ),
}))
vi.mock(`@/components/issue-picker-dialog`, () => ({
  IssuePickerDialog: () => null,
}))
vi.mock(`@/components/sub-issue-composer`, () => ({
  SubIssueComposer: ({ open }: { open?: boolean }) =>
    open ? <div data-testid="sub-issue-composer" /> : null,
}))

import {
  IssueParentLine,
  IssueRelationsSection,
  MobileRelationBands,
  relationSlot,
  relationsViewInput,
} from "@/components/issue-relations-card"

const ref = (id: string, status = `in_progress`) => ({
  id,
  identifier: id.toUpperCase(),
  title: `Title ${id}`,
  status,
  statusId: null,
  boardSlug: `web`,
})

const subject = {
  id: `me`,
  identifier: `ME`,
  title: `Subject`,
  boardId: `b1`,
} as unknown as Issue
const users: User[] = []

function seed(
  rows: Array<{ type: string; issueId: string; relatedIssueId: string }>,
  refs: Array<ReturnType<typeof ref>>
) {
  mocks.relations = rows.map((row, index) => ({
    id: `rel-${index}`,
    source: `user`,
    ...row,
  }))
  mocks.refs = new Map(refs.map((row) => [row.id, row]))
}

afterEach(() => {
  mocks.relations = []
  mocks.refs = new Map()
  vi.clearAllMocks()
})

describe(`relationSlot + relationsViewInput`, () => {
  it(`maps each point-of-view side onto its band`, () => {
    expect(relationSlot({ type: `parent`, direction: `forward` })).toBe(`child`)
    expect(relationSlot({ type: `parent`, direction: `inverse` })).toBe(`parent`)
    expect(relationSlot({ type: `blocks`, direction: `forward` })).toBe(`blocking`)
    expect(relationSlot({ type: `blocks`, direction: `inverse` })).toBe(`blocked_by`)
    expect(relationSlot({ type: `duplicate`, direction: `forward` })).toBe(
      `duplicate_of`
    )
    expect(relationSlot({ type: `duplicate`, direction: `inverse` })).toBe(
      `duplicated_by`
    )
    expect(relationSlot({ type: `related`, direction: `inverse` })).toBe(`related`)
  })

  it(`folds rows back to their canonical direction`, () => {
    const other = { ...ref(`x`), description: null } as never
    const input = relationsViewInput(
      `me`,
      [
        { id: `r1`, type: `blocks`, source: `user`, direction: `inverse`, other },
        { id: `r2`, type: `parent`, source: `user`, direction: `forward`, other },
      ],
      { toggled: [`related`], showAll: [] }
    )
    expect(input.relations).toEqual([
      { type: `blocks`, issueId: `x`, relatedIssueId: `me` },
      { type: `parent`, issueId: `me`, relatedIssueId: `x` },
    ])
    expect(input.issues).toEqual([
      { id: `x`, identifier: `X`, title: `Title x`, status: `in_progress` },
    ])
    expect(input.toggled).toEqual([`related`])
  })
})

describe(`IssueRelationsSection (md+)`, () => {
  it(`draws the Sub-issues band and the bands, blockers open`, () => {
    seed(
      [
        { type: `parent`, issueId: `me`, relatedIssueId: `c1` },
        { type: `parent`, issueId: `me`, relatedIssueId: `c2` },
        { type: `blocks`, issueId: `b1`, relatedIssueId: `me` },
        { type: `related`, issueId: `me`, relatedIssueId: `r1` },
      ],
      [ref(`c1`, `done`), ref(`c2`), ref(`b1`), ref(`r1`)]
    )
    render(<IssueRelationsSection issue={subject} teamId="t1" users={users} />)
    const sub = screen.getByTestId(`sub-issues`)
    expect(within(sub).getByText(`Sub-issues`)).toBeTruthy()
    expect(within(sub).getByText(`1/2`)).toBeTruthy()
    expect(sub.querySelector(`[data-slot=progress-ring]`)).toBeTruthy()
    expect(screen.getByTestId(`relation-row-C1`)).toBeTruthy()
    // Blocked by opens by default; Related stays folded.
    const blocked = screen.getByTestId(`relation-band-blocked_by`)
    expect(within(blocked).getByText(`Blocked by`)).toBeTruthy()
    expect(within(blocked).getByTestId(`relation-row-B1`)).toBeTruthy()
    const related = screen.getByTestId(`relation-band-related`)
    expect(within(related).queryByTestId(`relation-row-R1`)).toBeNull()
    // Unfolding feeds the model back.
    fireEvent.click(within(related).getByRole(`button`, { name: /Related/ }))
    expect(
      within(screen.getByTestId(`relation-band-related`)).getByTestId(
        `relation-row-R1`
      )
    ).toBeTruthy()
  })

  it(`opens the issue from its row, removes the relation from its ✕`, () => {
    seed(
      [{ type: `blocks`, issueId: `b1`, relatedIssueId: `me` }],
      [ref(`b1`)]
    )
    render(<IssueRelationsSection issue={subject} teamId="t1" users={users} />)
    const row = screen.getByTestId(`relation-row-B1`)
    fireEvent.click(within(row).getByText(`Title b1`))
    expect(mocks.open).toHaveBeenCalledWith(`B1`)
    fireEvent.click(within(row).getByLabelText(`Remove relation to B1`))
    expect(mocks.deleteMutate).toHaveBeenCalledWith({ id: `rel-0` })
  })

  it(`caps a band at three rows behind "Show N more"`, () => {
    const ids = [`b1`, `b2`, `b3`, `b4`, `b5`]
    seed(
      ids.map((id) => ({ type: `blocks`, issueId: id, relatedIssueId: `me` })),
      ids.map((id) => ref(id))
    )
    render(<IssueRelationsSection issue={subject} teamId="t1" users={users} />)
    const band = () => screen.getByTestId(`relation-band-blocked_by`)
    expect(within(band()).getAllByTestId(/relation-row-/)).toHaveLength(3)
    fireEvent.click(within(band()).getByText(`Show 2 more`))
    expect(within(band()).getAllByTestId(/relation-row-/)).toHaveLength(5)
    fireEvent.click(within(band()).getByText(`Show less`))
    expect(within(band()).getAllByTestId(/relation-row-/)).toHaveLength(3)
  })

  it(`offers "Add sub-issues" with none, and the + opens the composer`, () => {
    seed([], [])
    const { unmount } = render(
      <IssueRelationsSection issue={subject} teamId="t1" users={users} />
    )
    expect(screen.queryByTestId(`sub-issue-composer`)).toBeNull()
    fireEvent.click(screen.getByTestId(`add-sub-issues`))
    expect(screen.getByTestId(`sub-issue-composer`)).toBeTruthy()
    unmount()
    seed([{ type: `parent`, issueId: `me`, relatedIssueId: `c1` }], [ref(`c1`)])
    render(<IssueRelationsSection issue={subject} teamId="t1" users={users} />)
    fireEvent.click(screen.getByLabelText(`Add sub-issue`))
    expect(screen.getByTestId(`sub-issue-composer`)).toBeTruthy()
  })

  it(`draws nothing for a read-only viewer of an unrelated issue`, () => {
    seed([], [])
    const { container } = render(
      <IssueRelationsSection issue={subject} teamId="t1" users={users} readOnly />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`leaves the bands to the sheet on phones`, () => {
    seed(
      [{ type: `blocks`, issueId: `b1`, relatedIssueId: `me` }],
      [ref(`b1`)]
    )
    render(
      <IssueRelationsSection issue={subject} teamId="t1" users={users} phone />
    )
    expect(screen.queryByTestId(`relation-band-blocked_by`)).toBeNull()
    expect(screen.getByTestId(`add-sub-issues`)).toBeTruthy()
  })
})

describe(`IssueParentLine`, () => {
  it(`names the parent above the title`, () => {
    seed([{ type: `parent`, issueId: `p1`, relatedIssueId: `me` }], [ref(`p1`)])
    render(<IssueParentLine issueId="me" />)
    const line = screen.getByTestId(`sub-issue-of`)
    expect(within(line).getByText(`Sub-issue of`)).toBeTruthy()
    expect(within(line).getByTestId(`chip-P1`)).toBeTruthy()
  })

  it(`is absent without a parent`, () => {
    seed([], [])
    const { container } = render(<IssueParentLine issueId="me" />)
    expect(container.innerHTML).toBe(``)
  })
})

describe(`MobileRelationBands (the phone sheet)`, () => {
  it(`groups the relations under "Relations" with Add`, () => {
    seed(
      [
        { type: `blocks`, issueId: `b1`, relatedIssueId: `me` },
        { type: `blocks`, issueId: `me`, relatedIssueId: `k1` },
        // Sub-issues stay on the detail, never in the sheet.
        { type: `parent`, issueId: `me`, relatedIssueId: `c1` },
      ],
      [ref(`b1`), ref(`k1`), ref(`c1`)]
    )
    render(<MobileRelationBands issueId="me" />)
    const sheet = screen.getByTestId(`mobile-relations`)
    expect(within(sheet).getByText(`Relations`)).toBeTruthy()
    expect(within(sheet).getByText(`Add`)).toBeTruthy()
    expect(within(sheet).getByTestId(`relation-band-blocked_by`)).toBeTruthy()
    expect(within(sheet).getByTestId(`relation-band-blocking`)).toBeTruthy()
    expect(within(sheet).queryByTestId(`relation-row-C1`)).toBeNull()
  })

  it(`hides for a read-only viewer with nothing to show`, () => {
    seed([], [])
    const { container } = render(<MobileRelationBands issueId="me" readOnly />)
    expect(container.innerHTML).toBe(``)
  })
})
