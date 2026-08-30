import { describe, expect, it } from "vitest"
import type { Issue } from "@/db/schema"
import {
  emptyFilters,
  isValidStatusToken,
  issueFilterSearchFromFilters,
  issueFiltersFromSearch,
  matchesFilters,
  parseIssueFilterSearch,
} from "@/lib/filters"

const ROW_ID = `11111111-1111-1111-1111-111111111111`
const OTHER_ROW_ID = `22222222-2222-2222-2222-222222222222`

function makeIssue(overrides: Partial<Issue>): Issue {
  return {
    assigneeId: null,
    completedAt: null,
    createdAt: new Date(`2026-03-06T10:00:00.000Z`),
    creatorId: `user-1`,
    source: `user`,
    description: null,
    dueDate: null,
    id: `issue-1`,
    identifier: `APP-1`,
    number: 1,
    priority: `none`,
    boardId: `board-1`,
    teamId: `team-1`,
    boardDeletedAt: null,
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
  } as Issue
}

describe(`matchesFilters status tokens`, () => {
  it(`matches a row-uuid token against statusId`, () => {
    const issue = makeIssue({ status: `in_progress`, statusId: ROW_ID })
    expect(
      matchesFilters(issue, [], { ...emptyFilters, statusTokens: [ROW_ID] })
    ).toBe(true)
    expect(
      matchesFilters(issue, [], {
        ...emptyFilters,
        statusTokens: [OTHER_ROW_ID],
      })
    ).toBe(false)
  })

  // Legacy shared URLs (?status=in_progress) keep working, and an issue in a
  // CUSTOM started status matches through its dual-written anchor.
  it(`matches a legacy enum token against the anchor status`, () => {
    const issue = makeIssue({ status: `in_progress`, statusId: ROW_ID })
    expect(
      matchesFilters(issue, [], {
        ...emptyFilters,
        statusTokens: [`in_progress`],
      })
    ).toBe(true)
    expect(
      matchesFilters(issue, [], { ...emptyFilters, statusTokens: [`done`] })
    ).toBe(false)
  })

  it(`ignores unknown tokens without dropping the other filters`, () => {
    const issue = makeIssue({ status: `backlog`, statusId: null })
    expect(
      matchesFilters(issue, [], {
        ...emptyFilters,
        statusTokens: [OTHER_ROW_ID, `backlog`],
      })
    ).toBe(true)
  })
})

describe(`filter URL round-trip`, () => {
  it(`accepts uuid and enum status tokens, drops anything else`, () => {
    expect(isValidStatusToken(ROW_ID)).toBe(true)
    expect(isValidStatusToken(`in_review`)).toBe(true)
    expect(isValidStatusToken(`nonsense`)).toBe(false)
    // EXP-685 retired `todo`: an old shared URL carrying it drops the token
    // rather than filtering on a status no team has any more.
    expect(isValidStatusToken(`todo`)).toBe(false)

    expect(
      parseIssueFilterSearch({
        status: `${ROW_ID},backlog,todo,nonsense`,
        priority: `high,bogus`,
        labels: `label-1`,
      })
    ).toEqual({
      status: `${ROW_ID},backlog`,
      priority: `high`,
      labels: `label-1`,
    })
  })

  it(`round-trips filters through the search params`, () => {
    const filters = {
      statusTokens: [ROW_ID, `done`],
      priorities: [`urgent` as const],
      labelIds: [`label-1`],
    }
    const search = issueFilterSearchFromFilters(filters)
    expect(search).toEqual({
      status: `${ROW_ID},done`,
      priority: `urgent`,
      labels: `label-1`,
    })
    expect(issueFiltersFromSearch(search)).toEqual(filters)
  })

  it(`emits undefined for empty filters so they leave the URL`, () => {
    expect(issueFilterSearchFromFilters(emptyFilters)).toEqual({
      status: undefined,
      priority: undefined,
      labels: undefined,
    })
  })
})
