import {
  decodeToolResult,
  groupIssuesByStatus,
  parseView,
  runStateLabel,
  type IssueRow,
} from "./model"

const row = (id: string, status: string, priority = `none`): IssueRow =>
  ({ id, identifier: id, title: id, status, priority }) as IssueRow

describe(`parseView`, () => {
  it(`accepts the known views and falls back to issues`, () => {
    expect(parseView(`run`)).toBe(`run`)
    expect(parseView(`issue`)).toBe(`issue`)
    expect(parseView(`nope`)).toBe(`issues`)
    expect(parseView(null)).toBe(`issues`)
  })
})

describe(`decodeToolResult`, () => {
  it(`parses the ok() text block`, () => {
    expect(
      decodeToolResult({ content: [{ type: `text`, text: `[{"id":"a"}]` }] })
    ).toEqual({ kind: `ok`, data: [{ id: `a` }] })
  })
  it(`prefers structuredContent`, () => {
    expect(
      decodeToolResult({ content: [], structuredContent: { id: `b` } })
    ).toEqual({ kind: `ok`, data: { id: `b` } })
  })
  it(`surfaces an error result's text`, () => {
    expect(
      decodeToolResult({
        isError: true,
        content: [{ type: `text`, text: `Issue not found` }],
      })
    ).toEqual({ kind: `error`, message: `Issue not found` })
  })
  it(`treats unparseable text as an error`, () => {
    expect(
      decodeToolResult({ content: [{ type: `text`, text: `oops` }] }).kind
    ).toBe(`error`)
  })
})

describe(`groupIssuesByStatus`, () => {
  it(`groups in the contract display order, keeping row order`, () => {
    const groups = groupIssuesByStatus([
      row(`a`, `done`),
      row(`b`, `backlog`),
      row(`c`, `in_progress`),
      row(`d`, `backlog`),
    ])
    expect(groups.map((g) => g.status)).toEqual([
      `backlog`,
      `in_progress`,
      `done`,
    ])
    expect(groups[0].issues.map((i) => i.id)).toEqual([`b`, `d`])
  })
  it(`folds unknown values to backlog / no priority`, () => {
    const [group] = groupIssuesByStatus([row(`a`, `todo`, `blocker`)])
    expect(group.status).toBe(`backlog`)
    expect(group.issues[0].priority).toBe(`none`)
  })
})

describe(`runStateLabel`, () => {
  it(`names the run's state`, () => {
    expect(runStateLabel({ id: `r`, status: `ended` })).toBe(`Ended`)
    expect(runStateLabel({ id: `r`, status: `running`, needsInput: true })).toBe(
      `Needs input`
    )
    expect(runStateLabel({ id: `r`, status: `in_review` })).toBe(`In review`)
    expect(
      runStateLabel({ id: `r`, status: `running`, agentBusy: true, agentCaption: `Editing` })
    ).toBe(`Editing`)
    expect(runStateLabel({ id: `r`, status: `running` })).toBe(`Idle`)
  })
})
