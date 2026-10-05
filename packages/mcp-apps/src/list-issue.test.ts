import {
  assigneeBoardIds,
  dueDateTone,
  formatDueDate,
  issueListColumns,
  localDay,
  type IssueListRow,
} from "./list-issue"

const row = (id: string, extra: Partial<IssueListRow> = {}): IssueListRow => ({
  id,
  identifier: id,
  title: id,
  status: `backlog`,
  priority: `none`,
  ...extra,
})

describe(`due dates`, () => {
  it(`tones today over overdue over upcoming`, () => {
    expect(dueDateTone(`2026-10-05`, `2026-10-05`)).toBe(`today`)
    expect(dueDateTone(`2026-10-04`, `2026-10-05`)).toBe(`overdue`)
    expect(dueDateTone(`2026-10-06T00:00:00Z`, `2026-10-05`)).toBe(`upcoming`)
  })

  it(`formats a local date without a UTC shift`, () => {
    expect(formatDueDate(`2026-10-05`)).toBe(`Oct 5`)
    expect(localDay(new Date(2026, 0, 9))).toBe(`2026-01-09`)
  })
})

describe(`columns and assignees`, () => {
  it(`draws only the columns somebody fills`, () => {
    expect(issueListColumns([row(`a`)])).toEqual({ pr: false, assignee: false, due: false })
    expect(
      issueListColumns([row(`a`, { prNumber: 3 }), row(`b`, { assigneeId: `u`, dueDate: `2026-01-01` })])
    ).toEqual({ pr: true, assignee: true, due: true })
  })

  it(`looks up only boards holding an assigned row, capped`, () => {
    const rows = [
      row(`a`, { boardId: `b1`, assigneeId: `u` }),
      row(`b`, { boardId: `b2` }),
      row(`c`, { boardId: `b1`, assigneeId: `v` }),
      row(`d`, { boardId: `b3`, assigneeId: `u` }),
    ]
    expect(assigneeBoardIds(rows)).toEqual([`b1`, `b3`])
    expect(assigneeBoardIds(rows, 1)).toEqual([`b1`])
  })
})
