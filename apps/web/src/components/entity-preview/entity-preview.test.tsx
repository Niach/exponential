import { readFileSync } from "node:fs"
import { join } from "node:path"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { EntityRef } from "@/lib/mcp/preview"
import type { ResolvedIssueRef } from "@/components/issue-ref-provider"

// EXP-920: the entity chips under a settled Exponential tool row. Two halves
// are locked here: `entityRefRoute`, the pure "where does this chip go" rule
// (one case per kind), and the chip row itself — one chip per
// `groupPreviewRefs` group wearing the contract's label (the fixture
// `packages/domain-contract/fixtures/entity-chip.json` supplies the refs,
// so a label here cannot drift from the ×4 lock).
//
// Rows are mocked at the seam every other component test uses: `useLiveQuery`
// answers with whatever the test hands it, the issue resolver is a stub, and
// `useIsMobile` is a switch — a phone chip has no hover card to mount.

const mobile = { value: true }
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => mobile.value,
}))

const liveRows: { value: unknown[] } = { value: [] }
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: () => ({ data: liveRows.value }),
}))

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({
    children,
    to,
    params,
    search,
    ...rest
  }: {
    children: React.ReactNode
    to: string
    params?: Record<string, string>
    search?: Record<string, string>
  }) => (
    <a
      {...rest}
      href={to}
      data-params={JSON.stringify(params ?? {})}
      data-search={JSON.stringify(search ?? {})}
    >
      {children}
    </a>
  ),
  useParams: () => ({ teamSlug: `acme` }),
}))

const issues = new Map<string, ResolvedIssueRef>()
const open = vi.fn()
vi.mock(`@/components/issue-ref-provider`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIssueRefs: () => ({
    teamId: `t-1`,
    rows: [...issues.values()],
    open,
    resolve: (identifier: string) =>
      [...issues.values()].find((row) => row.identifier === identifier) ?? null,
    resolveById: (id: string) => issues.get(id) ?? null,
  }),
}))

import { entityRefRoute, type EntityRefRouteContext } from "./entity-ref-route"
import { EntityRefChips } from "./entity-ref-chips"
import { cardExists } from "./entity-ref-chip"

const FIXTURE = JSON.parse(
  readFileSync(
    join(
      import.meta.dirname,
      `../../../../../packages/domain-contract/fixtures/entity-chip.json`
    ),
    `utf8`
  )
) as {
  chips: { ref: EntityRef; label: string; detail: string | null }[]
  groups: { name: string; refs: EntityRef[]; groups: { ref: number }[] }[]
}

const issue: ResolvedIssueRef = {
  id: `i-1`,
  identifier: `EXP-1`,
  title: `Fix the flicker`,
  description: null,
  createdAt: null,
  updatedAt: null,
  boardId: `b-1`,
  status: `in_progress`,
  statusId: null,
  statusIcon: `circle-dot`,
  statusColor: `var(--color-yellow-500)`,
  boardSlug: `web`,
}

function context(over: Partial<EntityRefRouteContext> = {}): EntityRefRouteContext {
  return {
    teamSlug: `acme`,
    issueById: (id) => (id === `i-1` ? issue : null),
    issueByIdentifier: (identifier) => (identifier === `EXP-1` ? issue : null),
    boardSlugById: (id) => (id === `b-1` ? `web` : null),
    commentIssueId: (id) => (id === `c-1` ? `i-1` : null),
    attachmentIssueId: (id) => (id === `at-1` ? `i-1` : null),
    notificationIssueId: (id) => (id === `n-1` ? `i-1` : null),
    teamSlugById: (id) => (id === `t-2` ? `other` : null),
    ...over,
  }
}

const ISSUE_ROUTE = {
  to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
  params: { teamSlug: `acme`, boardSlug: `web`, issueIdentifier: `EXP-1` },
}

describe(`entityRefRoute`, () => {
  const ctx = context()
  const route = (ref: Partial<EntityRef> & { kind: EntityRef[`kind`] }) =>
    entityRefRoute({ id: `x`, ...ref } as EntityRef, ctx)

  it(`sends an issue to its page by id, by identifier, or by an id that IS one`, () => {
    expect(route({ kind: `issue`, id: `i-1` })).toEqual(ISSUE_ROUTE)
    expect(route({ kind: `issue`, id: `unknown`, identifier: `EXP-1` })).toEqual(ISSUE_ROUTE)
    expect(route({ kind: `issue`, id: `EXP-1` })).toEqual(ISSUE_ROUTE)
    expect(route({ kind: `issue`, id: `i-9` })).toBeNull()
  })

  it(`sends a board to its list, and an unknown board nowhere`, () => {
    expect(route({ kind: `board`, id: `b-1` })).toEqual({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug: `acme`, boardSlug: `web` },
    })
    expect(route({ kind: `board`, id: `b-9` })).toBeNull()
  })

  it(`opens an action and an automation in their editors`, () => {
    expect(route({ kind: `action`, id: `a-1` })).toEqual({
      to: `/t/$teamSlug/actions`,
      params: { teamSlug: `acme` },
      search: { editAction: `a-1` },
    })
    expect(route({ kind: `automation`, id: `au-1` })).toEqual({
      to: `/t/$teamSlug/automations`,
      params: { teamSlug: `acme` },
      search: { editAutomation: `au-1` },
    })
  })

  it(`sends a comment, an attachment and a notification to their issue`, () => {
    expect(route({ kind: `comment`, id: `c-1` })).toEqual(ISSUE_ROUTE)
    expect(route({ kind: `comment`, id: `c-9` })).toBeNull()
    expect(route({ kind: `attachment`, id: `at-1` })).toEqual(ISSUE_ROUTE)
    expect(route({ kind: `attachment`, id: `at-9` })).toBeNull()
    expect(route({ kind: `notification`, id: `n-1` })).toEqual(ISSUE_ROUTE)
    // An issue-less notification still opens the inbox.
    expect(route({ kind: `notification`, id: `n-9` })).toEqual({
      to: `/t/$teamSlug/inbox`,
      params: { teamSlug: `acme` },
    })
  })

  it(`sends a run, a workflow and a thread to their own pages`, () => {
    expect(route({ kind: `session`, id: `s-1` })).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s-1` },
    })
    expect(route({ kind: `workflow`, id: `w-1` })).toEqual({
      to: `/t/$teamSlug/workflows/$workflowId`,
      params: { teamSlug: `acme`, workflowId: `w-1` },
    })
    expect(route({ kind: `thread`, id: `th-1` })).toEqual({
      to: `/t/$teamSlug/support/$threadId`,
      params: { teamSlug: `acme`, threadId: `th-1` },
    })
  })

  it(`sends the settings-owned kinds to their settings page`, () => {
    const settings = (page: string) => ({
      to: `/t/$teamSlug/settings/${page}`,
      params: { teamSlug: `acme` },
    })
    expect(route({ kind: `label`, id: `l-1` })).toEqual(settings(`labels`))
    expect(route({ kind: `status`, id: `st-1` })).toEqual(settings(`statuses`))
    expect(route({ kind: `member`, id: `u-1` })).toEqual(settings(`members`))
    expect(route({ kind: `invite`, id: `inv-1` })).toEqual(settings(`members`))
    expect(route({ kind: `repository`, id: `r-1` })).toEqual(settings(`repositories`))
    expect(route({ kind: `device`, id: `d-1` })).toEqual({
      to: `/t/$teamSlug/devices`,
      params: { teamSlug: `acme` },
    })
  })

  it(`sends a team to ITS slug, never the current one`, () => {
    expect(route({ kind: `team`, id: `t-2` })).toEqual({
      to: `/t/$teamSlug`,
      params: { teamSlug: `other` },
    })
    expect(route({ kind: `team`, id: `t-9` })).toBeNull()
  })

  it(`a list never navigates, and nothing does outside a team`, () => {
    expect(route({ kind: `list`, id: `issue`, count: 3 })).toBeNull()
    expect(
      entityRefRoute({ kind: `session`, id: `s-1` }, context({ teamSlug: undefined }))
    ).toBeNull()
  })
})

describe(`cardExists`, () => {
  it(`is the list's members, the row-less kinds' title, and everything else`, () => {
    expect(cardExists({ kind: `list`, id: `issue`, count: 0 }, [])).toBe(false)
    expect(cardExists({ kind: `list`, id: `issue`, count: 1 }, [{ kind: `issue`, id: `i-1` }])).toBe(true)
    expect(cardExists({ kind: `repository`, id: `r-1` }, [])).toBe(false)
    expect(cardExists({ kind: `repository`, id: `r-1`, title: `Niach/exponential` }, [])).toBe(true)
    expect(cardExists({ kind: `thread`, id: `th-1`, title: ` ` }, [])).toBe(false)
    expect(cardExists({ kind: `board`, id: `b-1` }, [])).toBe(true)
  })
})

describe(`EntityRefChips`, () => {
  function group(name: string) {
    const found = FIXTURE.groups.find((entry) => entry.name === name)
    if (!found) throw new Error(`fixture group ${name} is missing`)
    return found
  }

  it(`renders one chip per group, labelled by the contract`, () => {
    issues.clear()
    liveRows.value = []
    const { refs, groups } = group(`a ref of another kind after a list is its own chip`)
    render(<EntityRefChips refs={refs} />)
    const row = screen.getByTestId(`entity-ref-chips`)
    const chips = row.querySelectorAll(`[data-slot="entity-chip"]`)
    expect(chips.length).toBe(groups.length)
    expect(chips[0]!.textContent).toBe(`2 comments`)
    expect(chips[1]!.textContent).toBe(`EXP-1`)
    // The shared box, never a capsule.
    for (const chip of chips) {
      expect(chip.className).toContain(`issue-chip`)
      expect(chip.className).not.toContain(`rounded-full`)
    }
  })

  it(`renders nothing for no refs`, () => {
    const { container } = render(<EntityRefChips refs={[]} />)
    expect(container.innerHTML).toBe(``)
  })

  it(`draws every fixture chip with the fixture's label (and an issue's detail)`, () => {
    issues.clear()
    liveRows.value = []
    for (const { ref, label, detail } of FIXTURE.chips) {
      const { unmount } = render(<EntityRefChips refs={[ref]} />)
      const chip = screen.getByTestId(`entity-ref-chips`).querySelector(`[data-slot="entity-chip"]`)
      expect(chip?.textContent, `${ref.kind}/${ref.id}`).toBe(`${label}${detail ?? ``}`)
      unmount()
    }
  })

  it(`an unsynced row is the same chip, muted and inert`, () => {
    issues.clear()
    liveRows.value = []
    render(<EntityRefChips refs={[{ kind: `board`, id: `b-9`, title: `Elsewhere` }]} />)
    const chip = screen.getByTestId(`entity-chip-board`)
    expect(chip.textContent).toBe(`Elsewhere`)
    expect(chip.className).toContain(`text-muted-foreground`)
    expect(screen.queryByRole(`link`)).toBeNull()
    expect(screen.queryByRole(`button`)).toBeNull()
  })

  it(`a synced issue is a real link to its page on a pointer device`, () => {
    mobile.value = false
    issues.set(issue.id, issue)
    liveRows.value = []
    try {
      render(<EntityRefChips refs={[{ kind: `issue`, id: `i-1`, identifier: `EXP-1`, title: `Fix the flicker` }]} />)
      const link = screen.getByRole(`link`, { name: `Open EXP-1` })
      expect(link.getAttribute(`href`)).toBe(ISSUE_ROUTE.to)
      expect(JSON.parse(link.getAttribute(`data-params`) ?? `{}`)).toEqual(ISSUE_ROUTE.params)
      // Identifier and title, in the chip's three-part layout.
      const chip = screen.getByTestId(`entity-chip-issue`)
      expect(chip.textContent).toBe(`EXP-1Fix the flicker`)
      expect(chip.querySelector(`svg`)).toBeTruthy()
    } finally {
      mobile.value = true
      issues.clear()
    }
  })

  it(`on a phone a synced chip opens the sheet with the card and an Open row`, () => {
    mobile.value = true
    issues.set(issue.id, issue)
    liveRows.value = []
    try {
      render(<EntityRefChips refs={[{ kind: `issue`, id: `i-1`, identifier: `EXP-1` }]} />)
      expect(screen.queryByTestId(`entity-preview-sheet`)).toBeNull()
      fireEvent.click(screen.getByRole(`button`, { name: `Open EXP-1` }))
      expect(screen.getByTestId(`entity-preview-sheet`)).toBeTruthy()
      const link = screen.getByRole(`link`, { name: `Open EXP-1` })
      expect(link.getAttribute(`href`)).toBe(ISSUE_ROUTE.to)
    } finally {
      issues.clear()
    }
  })
})
