import { describe, expect, it } from "vitest"
import type { Issue, IssueLabel, Label } from "@/db/schema"
import { emptyFilters } from "@/lib/filters"
import {
  buildFilteredIssues,
  buildIssueLabelIdsMap,
  buildIssueLabelMap,
  buildVisibleIssueGroups,
  getEditingIssue,
  getEditingIssueLabelIds,
} from "@/lib/project-board"

function makeIssue(overrides: Partial<Issue>): Issue {
  return {
    archivedAt: null,
    assigneeId: null,
    completedAt: null,
    createdAt: new Date(`2026-03-06T10:00:00.000Z`),
    creatorId: `user-1`,
    description: { text: `Description` },
    dueDate: null,
    id: `issue-1`,
    identifier: `APP-1`,
    number: 1,
    priority: `none`,
    projectId: `project-1`,
    recurrenceInterval: null,
    recurrenceUnit: null,
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

  it(`derives the editing issue and selected labels`, () => {
    const issues = [makeIssue({ id: `issue-1` }), makeIssue({ id: `issue-2` })]
    const issueLabels = [
      makeIssueLabel({ issueId: `issue-2`, labelId: `label-1` }),
      makeIssueLabel({ issueId: `issue-2`, labelId: `label-2` }),
    ]

    expect(getEditingIssue(issues, `issue-2`)).toEqual(issues[1])
    expect(getEditingIssueLabelIds(issueLabels, `issue-2`)).toEqual([
      `label-1`,
      `label-2`,
    ])
    expect(getEditingIssue(issues, null)).toBeNull()
  })
})
