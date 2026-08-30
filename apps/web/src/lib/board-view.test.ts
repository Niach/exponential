import { describe, expect, it } from "vitest"
import type { Issue, IssueLabel, Label } from "@/db/schema"
import { formatDateForMutation } from "@/lib/domain"
import { emptyFilters } from "@/lib/filters"
import {
  buildFilteredIssues,
  buildIssueLabelIdsMap,
  buildIssueLabelMap,
  buildVisibleIssueGroups as buildGroups,
  compareIssuesForGroup,
  findIssuePosition,
} from "@/lib/board-view"
import {
  buildStatusOptions,
  defaultStatusOptions,
  resolveIssueStatus,
  type StatusRowOption,
} from "@/lib/team-statuses"

// The default (unsynced / freshly-seeded) team: the 7 constructed builtin
// rows, so these legacy expectations still describe a real team's grouping.
const DEFAULT_OPTIONS = defaultStatusOptions()
const DEFAULT_BY_ID = new Map(DEFAULT_OPTIONS.map((o) => [o.id, o]))
const optionFor = (key: string): StatusRowOption =>
  DEFAULT_OPTIONS.find((option) => option.builtinKey === key)!

function buildVisibleIssueGroups(
  issues: Issue[],
  statusTokens: string[] = [],
  options: StatusRowOption[] = DEFAULT_OPTIONS
) {
  const byId = new Map(options.map((o) => [o.id, o]))
  return buildGroups(
    issues,
    options,
    (issue) => resolveIssueStatus(issue, options, byId),
    statusTokens
  ).map((group) => ({
    status: group.status.builtinKey ?? group.status.id,
    issues: group.issues,
  }))
}

// The un-mapped form, for findIssuePosition (which takes real IssueGroups).
function rawGroups(issues: Issue[], statusTokens: string[] = []) {
  return buildGroups(
    issues,
    DEFAULT_OPTIONS,
    (issue) => resolveIssueStatus(issue, DEFAULT_OPTIONS, DEFAULT_BY_ID),
    statusTokens
  )
}

function makeIssue(overrides: Partial<Issue>): Issue {
  return {
    assigneeId: null,
    completedAt: null,
    createdAt: new Date(`2026-03-06T10:00:00.000Z`),
    creatorId: `user-1`,
    source: `user`,
    description: `Description`,
    dueDate: null,
    id: `issue-1`,
    identifier: `APP-1`,
    number: 1,
    priority: `none`,
    boardId: `board-1`,
    teamId: `team-1`,
    boardDeletedAt: null,
    boardArchivedAt: null,
    duplicateOfId: null,
    prUrl: null,
    prNumber: null,
    prState: null,
    branch: null,
    prMergedAt: null,
    sortOrder: 0,
    status: `backlog`,
    statusId: null,
    title: `Issue`,
    updatedAt: new Date(`2026-03-06T10:00:00.000Z`),
    ...overrides,
  }
}

function makeLabel(overrides: Partial<Label>): Label {
  return {
    color: `#6366f1`,
    createdAt: new Date(`2026-03-06T10:00:00.000Z`),
    id: `label-1`,
    name: `Bug`,
    sortOrder: 0,
    updatedAt: new Date(`2026-03-06T10:00:00.000Z`),
    teamId: `team-1`,
    ...overrides,
  }
}

function makeIssueLabel(overrides: Partial<IssueLabel>): IssueLabel {
  return {
    issueId: `issue-1`,
    labelId: `label-1`,
    teamId: `team-1`,
    boardId: `board-1`,
    boardDeletedAt: null,
    boardArchivedAt: null,
    ...overrides,
  }
}

describe(`board-view helpers`, () => {
  it(`builds label maps and filters issues`, () => {
    const issues = [
      makeIssue({ id: `issue-1`, status: `backlog`, title: `Buggy` }),
      makeIssue({ id: `issue-2`, status: `done`, title: `Fixed` }),
    ]
    const labels = [makeLabel({ id: `label-1`, name: `Bug` })]
    const issueLabels = [
      makeIssueLabel({ issueId: `issue-1`, labelId: `label-1` }),
    ]
    const issueLabelIdsMap = buildIssueLabelIdsMap(issueLabels)
    const filters = {
      ...emptyFilters,
      labelIds: [`label-1`],
    }

    expect(buildIssueLabelMap(issueLabels, labels).get(`issue-1`)).toEqual(
      labels
    )
    expect(buildFilteredIssues(issues, issueLabelIdsMap, filters)).toEqual([
      issues[0],
    ])
  })

  it(`builds visible groups in status order`, () => {
    const issues = [
      makeIssue({ id: `issue-1`, status: `in_progress` }),
      makeIssue({ id: `issue-2`, status: `done` }),
      makeIssue({ id: `issue-3`, status: `backlog` }),
    ]

    expect(buildVisibleIssueGroups(issues, [])).toEqual([
      { status: `backlog`, issues: [issues[2]] },
      { status: `in_progress`, issues: [issues[0]] },
      { status: `done`, issues: [issues[1]] },
    ])

    expect(buildVisibleIssueGroups(issues, [`done`])).toEqual([
      { status: `done`, issues: [issues[1]] },
    ])
  })

  it(`sorts issues by priority within a status, overdues first`, () => {
    const today = formatDateForMutation(new Date())!
    const yesterday = formatDateForMutation(new Date(Date.now() - 86_400_000))!

    const overdueLow = makeIssue({
      id: `overdue-low`,
      status: `in_progress`,
      priority: `low`,
      dueDate: yesterday,
    })
    const urgentNoDue = makeIssue({
      id: `urgent-nodue`,
      status: `in_progress`,
      priority: `urgent`,
    })
    const mediumToday = makeIssue({
      id: `medium-today`,
      status: `in_progress`,
      priority: `medium`,
      dueDate: today,
    })
    const noPriority = makeIssue({
      id: `none`,
      status: `in_progress`,
      priority: `none`,
    })

    const groups = buildVisibleIssueGroups(
      [noPriority, mediumToday, urgentNoDue, overdueLow],
      []
    )

    expect(groups).toEqual([
      {
        status: `in_progress`,
        issues: [overdueLow, urgentNoDue, mediumToday, noPriority],
      },
    ])
  })

  // EXP-38: the canonical comparator's final tiebreak is the issue `number`,
  // compared numerically — an identifier-string sort would put APP-10 before
  // APP-9.
  it(`breaks non-terminal ties by issue number numerically`, () => {
    const nine = makeIssue({
      id: `issue-9`,
      identifier: `APP-9`,
      number: 9,
      status: `backlog`,
    })
    const ten = makeIssue({
      id: `issue-10`,
      identifier: `APP-10`,
      number: 10,
      status: `backlog`,
    })

    expect(buildVisibleIssueGroups([ten, nine], [])).toEqual([
      { status: `backlog`, issues: [nine, ten] },
    ])
  })

  it(`sorts null due dates after dated issues at equal priority`, () => {
    const tomorrow = formatDateForMutation(new Date(Date.now() + 86_400_000))!

    const noDue = makeIssue({
      id: `no-due`,
      number: 1,
      status: `in_progress`,
      priority: `high`,
    })
    const dated = makeIssue({
      id: `dated`,
      number: 2,
      status: `in_progress`,
      priority: `high`,
      dueDate: tomorrow,
    })

    expect(buildVisibleIssueGroups([noDue, dated], [])).toEqual([
      { status: `in_progress`, issues: [dated, noDue] },
    ])
  })

  // EXP-38: done sorts by (completedAt ?? updatedAt) DESC — latest completed
  // first, with updatedAt as the fallback key for rows that never got a
  // completedAt stamp.
  it(`sorts the done group by completion recency, falling back to updatedAt`, () => {
    const completedOld = makeIssue({
      id: `done-old`,
      status: `done`,
      priority: `urgent`,
      completedAt: new Date(`2026-03-01T10:00:00.000Z`),
      updatedAt: new Date(`2026-03-09T10:00:00.000Z`),
    })
    const completedNew = makeIssue({
      id: `done-new`,
      status: `done`,
      priority: `none`,
      completedAt: new Date(`2026-03-05T10:00:00.000Z`),
      updatedAt: new Date(`2026-03-05T10:00:00.000Z`),
    })
    const noStamp = makeIssue({
      id: `done-nostamp`,
      status: `done`,
      completedAt: null,
      updatedAt: new Date(`2026-03-03T10:00:00.000Z`),
    })

    expect(buildVisibleIssueGroups([completedOld, noStamp, completedNew], []))
      .toEqual([
        { status: `done`, issues: [completedNew, noStamp, completedOld] },
      ])
  })

  it(`sorts cancelled and duplicate groups by updatedAt descending`, () => {
    const cancelledOld = makeIssue({
      id: `cancelled-old`,
      status: `cancelled`,
      priority: `urgent`,
      dueDate: `2026-01-01`,
      updatedAt: new Date(`2026-03-01T10:00:00.000Z`),
    })
    const cancelledNew = makeIssue({
      id: `cancelled-new`,
      status: `cancelled`,
      updatedAt: new Date(`2026-03-08T10:00:00.000Z`),
    })
    const duplicateOld = makeIssue({
      id: `duplicate-old`,
      status: `duplicate`,
      updatedAt: new Date(`2026-03-02T10:00:00.000Z`),
    })
    const duplicateNew = makeIssue({
      id: `duplicate-new`,
      status: `duplicate`,
      updatedAt: new Date(`2026-03-07T10:00:00.000Z`),
    })

    expect(
      buildVisibleIssueGroups(
        [cancelledOld, duplicateOld, cancelledNew, duplicateNew],
        []
      )
    ).toEqual([
      { status: `cancelled`, issues: [cancelledNew, cancelledOld] },
      { status: `duplicate`, issues: [duplicateNew, duplicateOld] },
    ])
  })

  // The comparator also serves tRPC rows (public board) whose timestamps are
  // strings — Electric's `YYYY-MM-DD hh:mm:ss+00` and ISO `…T…Z` must compare
  // as the same instants.
  it(`compares mixed string/Date timestamp formats as instants`, () => {
    const compare = compareIssuesForGroup(`completed`, `2026-03-06`)
    const electricFormat = {
      priority: `none` as const,
      dueDate: null,
      number: 1,
      completedAt: `2026-03-05 10:00:00+00`,
      updatedAt: `2026-03-05 10:00:00+00`,
    }
    const isoFormat = {
      priority: `none` as const,
      dueDate: null,
      number: 2,
      completedAt: `2026-03-04T10:00:00.000Z`,
      updatedAt: `2026-03-04T10:00:00.000Z`,
    }
    const dateFormat = {
      priority: `none` as const,
      dueDate: null,
      number: 3,
      completedAt: null,
      updatedAt: new Date(`2026-03-03T10:00:00.000Z`),
    }

    expect(compare(electricFormat, isoFormat)).toBeLessThan(0)
    expect(compare(isoFormat, dateFormat)).toBeLessThan(0)
    expect(compare(dateFormat, electricFormat)).toBeGreaterThan(0)
  })

  // EXP-48: the detail header's prev/next switcher walks the flattened
  // visible-group sequence — group order first, then the in-group sort.
  it(`locates an issue across the flattened group sequence`, () => {
    const startedUrgent = makeIssue({
      id: `started-urgent`,
      identifier: `APP-2`,
      number: 2,
      status: `in_progress`,
      priority: `urgent`,
    })
    const startedLow = makeIssue({
      id: `started-low`,
      identifier: `APP-3`,
      number: 3,
      status: `in_progress`,
      priority: `low`,
    })
    const backlog = makeIssue({
      id: `backlog-1`,
      identifier: `APP-1`,
      number: 1,
      status: `backlog`,
    })

    // Flattened sequence: [backlog-1, started-urgent, started-low] (the
    // backlog group precedes started in the category display order).
    const groups = rawGroups([backlog, startedLow, startedUrgent])

    expect(findIssuePosition(groups, `backlog-1`)).toEqual({
      index: 1,
      total: 3,
      prev: null,
      next: startedUrgent,
    })
    expect(findIssuePosition(groups, `started-urgent`)).toEqual({
      index: 2,
      total: 3,
      prev: backlog,
      next: startedLow,
    })
    expect(findIssuePosition(groups, `started-low`)).toEqual({
      index: 3,
      total: 3,
      prev: startedUrgent,
      next: null,
    })
  })

  it(`returns null when the issue is filtered out of the visible groups`, () => {
    const done = makeIssue({ id: `done-1`, status: `done` })
    const started = makeIssue({ id: `started-1`, status: `in_progress` })

    // Status filter hides the done issue from the sequence entirely.
    const groups = rawGroups([started], [`in_progress`])

    expect(findIssuePosition(groups, done.id)).toBeNull()
    expect(findIssuePosition(groups, started.id)).toEqual({
      index: 1,
      total: 1,
      prev: null,
      next: null,
    })
  })

  it(`handles a single-issue and empty sequence`, () => {
    expect(findIssuePosition([], `missing`)).toBeNull()

    const only = makeIssue({ id: `only`, status: `backlog` })
    const groups = rawGroups([only])
    expect(findIssuePosition(groups, `only`)).toEqual({
      index: 1,
      total: 1,
      prev: null,
      next: null,
    })
  })
})

// EXP-314: grouping is per TEAM STATUS ROW, keyed by row id, with the
// comparator switching on the row's CATEGORY.
describe(`board-view custom statuses`, () => {
  const CUSTOM_ID = `aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa`
  const DONE_ID = `bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb`
  const customOptions = buildStatusOptions([
    {
      id: CUSTOM_ID,
      name: `Designing`,
      color: `#FF00AA`,
      category: `started`,
      builtinKey: null,
      sortOrder: 1,
      createdAt: new Date(`2026-01-01T00:00:00.000Z`),
    },
    {
      id: DONE_ID,
      name: `Shipped`,
      color: `#3B82F6`,
      category: `completed`,
      builtinKey: `done`,
      sortOrder: 1,
      createdAt: new Date(`2026-01-01T00:00:00.000Z`),
    },
  ])

  it(`groups by row id and keeps the team's row order`, () => {
    const custom = makeIssue({
      id: `custom-1`,
      status: `in_progress`,
      statusId: CUSTOM_ID,
    })
    const shipped = makeIssue({
      id: `shipped-1`,
      status: `done`,
      statusId: DONE_ID,
    })

    const groups = buildGroups(
      [shipped, custom],
      customOptions,
      (issue) => resolveIssueStatus(issue, customOptions),
      []
    )

    expect(groups.map((group) => group.status.id)).toEqual([
      CUSTOM_ID,
      DONE_ID,
    ])
    expect(groups[0].status.name).toBe(`Designing`)
    expect(groups[0].issues).toEqual([custom])
    expect(groups[1].issues).toEqual([shipped])
  })

  it(`sorts a custom completed group by completion recency`, () => {
    const older = makeIssue({
      id: `older`,
      status: `done`,
      statusId: DONE_ID,
      priority: `urgent`,
      completedAt: new Date(`2026-03-01T10:00:00.000Z`),
    })
    const newer = makeIssue({
      id: `newer`,
      status: `done`,
      statusId: DONE_ID,
      completedAt: new Date(`2026-03-05T10:00:00.000Z`),
    })

    const groups = buildGroups(
      [older, newer],
      customOptions,
      (issue) => resolveIssueStatus(issue, customOptions),
      []
    )
    expect(groups[0].issues).toEqual([newer, older])
  })

  it(`filters groups by row-id AND legacy enum tokens`, () => {
    const custom = makeIssue({
      id: `custom-1`,
      status: `in_progress`,
      statusId: CUSTOM_ID,
    })
    const shipped = makeIssue({
      id: `shipped-1`,
      status: `done`,
      statusId: DONE_ID,
    })
    const resolve = (issue: { status: string; statusId: string | null }) =>
      resolveIssueStatus(issue, customOptions)

    expect(
      buildGroups([custom, shipped], customOptions, resolve, [CUSTOM_ID]).map(
        (g) => g.status.id
      )
    ).toEqual([CUSTOM_ID])
    // `done` is the Shipped row's anchor — an old ?status=done URL still works.
    expect(
      buildGroups([custom, shipped], customOptions, resolve, [`done`]).map(
        (g) => g.status.id
      )
    ).toEqual([DONE_ID])
  })

  it(`falls back to a constructed group for an unknown status`, () => {
    const orphan = makeIssue({
      id: `orphan`,
      status: `cancelled`,
      statusId: null,
    })
    const groups = buildGroups(
      [orphan],
      customOptions,
      (issue) => resolveIssueStatus(issue, customOptions),
      []
    )
    expect(groups).toHaveLength(1)
    expect(groups[0].status.id).toBe(`builtin:cancelled`)
    expect(groups[0].issues).toEqual([orphan])
  })

  it(`exposes the resolved option (name/icon/color) on each group`, () => {
    const groups = buildVisibleIssueGroupsRaw()
    expect(groups[0].status.name).toBe(`In Progress`)
    expect(groups[0].status.icon).toBe(`progress-2-4`)
  })

  function buildVisibleIssueGroupsRaw() {
    return rawGroups([makeIssue({ id: `t`, status: `in_progress` })])
  }

  it(`keeps optionFor in step with the constructed defaults`, () => {
    expect(optionFor(`in_progress`).icon).toBe(`progress-2-4`)
    expect(optionFor(`backlog`).name).toBe(`Backlog`)
  })
})
