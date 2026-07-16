import { describe, expect, it } from "vitest"
import type { Issue, IssueLabel, Label } from "@/db/schema"
import { formatDateForMutation } from "@/lib/domain"
import { emptyFilters } from "@/lib/filters"
import {
  buildFilteredIssues,
  buildIssueLabelIdsMap,
  buildIssueLabelMap,
  buildVisibleIssueGroups,
  compareIssuesForGroup,
  findIssuePosition,
} from "@/lib/project-board"

function makeIssue(overrides: Partial<Issue>): Issue {
  return {
    archivedAt: null,
    assigneeId: null,
    completedAt: null,
    createdAt: new Date(`2026-03-06T10:00:00.000Z`),
    creatorId: `user-1`,
    description: `Description`,
    dueDate: null,
    dueTime: null,
    endTime: null,
    id: `issue-1`,
    identifier: `APP-1`,
    number: 1,
    priority: `none`,
    projectId: `project-1`,
    duplicateOfId: null,
    prUrl: null,
    prNumber: null,
    prState: null,
    branch: null,
    prMergedAt: null,
    sortOrder: 0,
    status: `backlog`,
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
    workspaceId: `workspace-1`,
    ...overrides,
  }
}

function makeIssueLabel(overrides: Partial<IssueLabel>): IssueLabel {
  return {
    issueId: `issue-1`,
    labelId: `label-1`,
    workspaceId: `workspace-1`,
    projectId: `project-1`,
    ...overrides,
  }
}

describe(`project-board helpers`, () => {
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
      makeIssue({ id: `issue-1`, status: `todo` }),
      makeIssue({ id: `issue-2`, status: `done` }),
      makeIssue({ id: `issue-3`, status: `backlog` }),
    ]

    expect(buildVisibleIssueGroups(issues, [])).toEqual([
      { status: `todo`, issues: [issues[0]] },
      { status: `backlog`, issues: [issues[2]] },
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
      status: `todo`,
      priority: `low`,
      dueDate: yesterday,
    })
    const urgentNoDue = makeIssue({
      id: `urgent-nodue`,
      status: `todo`,
      priority: `urgent`,
    })
    const mediumToday = makeIssue({
      id: `medium-today`,
      status: `todo`,
      priority: `medium`,
      dueDate: today,
    })
    const noPriority = makeIssue({
      id: `none`,
      status: `todo`,
      priority: `none`,
    })

    const groups = buildVisibleIssueGroups(
      [noPriority, mediumToday, urgentNoDue, overdueLow],
      []
    )

    expect(groups).toEqual([
      {
        status: `todo`,
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
      status: `todo`,
      priority: `high`,
    })
    const dated = makeIssue({
      id: `dated`,
      number: 2,
      status: `todo`,
      priority: `high`,
      dueDate: tomorrow,
    })

    expect(buildVisibleIssueGroups([noDue, dated], [])).toEqual([
      { status: `todo`, issues: [dated, noDue] },
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
    const compare = compareIssuesForGroup(`done`, `2026-03-06`)
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
    const todoUrgent = makeIssue({
      id: `todo-urgent`,
      identifier: `APP-2`,
      number: 2,
      status: `todo`,
      priority: `urgent`,
    })
    const todoLow = makeIssue({
      id: `todo-low`,
      identifier: `APP-3`,
      number: 3,
      status: `todo`,
      priority: `low`,
    })
    const backlog = makeIssue({
      id: `backlog-1`,
      identifier: `APP-1`,
      number: 1,
      status: `backlog`,
    })

    // Flattened sequence: [todo-urgent, todo-low, backlog-1] (todo group
    // precedes backlog in issueStatusOrder).
    const groups = buildVisibleIssueGroups([backlog, todoLow, todoUrgent], [])

    expect(findIssuePosition(groups, `todo-urgent`)).toEqual({
      index: 1,
      total: 3,
      prev: null,
      next: todoLow,
    })
    expect(findIssuePosition(groups, `todo-low`)).toEqual({
      index: 2,
      total: 3,
      prev: todoUrgent,
      next: backlog,
    })
    expect(findIssuePosition(groups, `backlog-1`)).toEqual({
      index: 3,
      total: 3,
      prev: todoLow,
      next: null,
    })
  })

  it(`returns null when the issue is filtered out of the visible groups`, () => {
    const done = makeIssue({ id: `done-1`, status: `done` })
    const todo = makeIssue({ id: `todo-1`, status: `todo` })

    // Status filter hides the done issue from the sequence entirely.
    const groups = buildVisibleIssueGroups([todo], [`todo`])

    expect(findIssuePosition(groups, done.id)).toBeNull()
    expect(findIssuePosition(groups, todo.id)).toEqual({
      index: 1,
      total: 1,
      prev: null,
      next: null,
    })
  })

  it(`handles a single-issue and empty sequence`, () => {
    expect(findIssuePosition([], `missing`)).toBeNull()

    const only = makeIssue({ id: `only`, status: `backlog` })
    const groups = buildVisibleIssueGroups([only], [])
    expect(findIssuePosition(groups, `only`)).toEqual({
      index: 1,
      total: 1,
      prev: null,
      next: null,
    })
  })
})
