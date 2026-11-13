import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/local-parity.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import { LOCAL_COMPOSER_DRAFT_EMPTY, anchorForCategory, applyIssuePatch, assistantRow, composerDraftRow, createCommentRow, createIssueRow, draftConsumedRefusal, draftRow, isLocalProcedure, issueIdentifier, localEmail, localRefusal, nextIssueNumber, seedRows, sendRows, type Row } from "./local-parity"

const ids = (list: readonly string[]) => {
  let i = 0
  return () => list[i++]!
}
const team = fixture.issues.team

describe(`local parity (EDE-55)`, () => {
  test(`the seed statuses are the contract's builtins`, () => {
    expect(fixture.seed.statuses).toEqual(contract.issueStatusDefaults)
  })
  test.each(fixture.procedures.cases)(`procedure $procedure`, (c) => {
    expect(isLocalProcedure(c.procedure)).toBe(c.local)
    if (c.error) expect(localRefusal(c.procedure)).toBe(c.error)
  })
  test.each(fixture.identity.cases)(`localEmail %j`, (c) => {
    expect(localEmail(c.login)).toBe(c.email)
  })
  test.each(fixture.seed.cases)(`seedRows: $name`, (c) => {
    expect(seedRows({ id: c.desktopId, name: c.desktopName }, c.identity, c.now, ids(c.ids))).toEqual(c.expected)
  })
  test.each(fixture.issues.numberCases)(`nextIssueNumber %j`, (c) => {
    expect(nextIssueNumber(c.numbers)).toBe(c.next)
  })
  test.each(fixture.issues.identifierCases)(`issueIdentifier %j`, (c) => {
    expect(issueIdentifier(c.prefix, c.number)).toBe(c.identifier)
  })
  test(`anchorForCategory`, () => {
    for (const [category, anchor] of Object.entries(fixture.issues.anchorByCategory)) expect(anchorForCategory(category)).toBe(anchor)
  })
  test.each(fixture.issues.create.cases)(`createIssueRow: $name`, (c) => {
    const row = createIssueRow(c.input as Row, { teamId: team.teamId, boardPrefix: team.boards[0]!.prefix, numbers: c.numbers, statuses: team.statuses, identity: team.identity, id: c.id, now: c.now })
    expect(row).toEqual(c.expected)
  })
  test.each(fixture.issues.update.cases)(`applyIssuePatch: $name`, (c) => {
    expect(applyIssuePatch(c.row as Row, c.patch as Row, team.statuses, c.now)).toEqual(c.expected)
  })
  test.each(fixture.drafts.upsert.cases)(`draftRow: $name`, (c) => {
    expect(draftRow(c.input as Row, { identity: team.identity, existing: c.existing as Row | null, now: c.now })).toEqual(c.expected)
  })
  test.each(fixture.composerDrafts.upsert.cases)(`composerDraftRow: $name`, (c) => {
    expect(composerDraftRow(c.input as Row, { identity: team.identity, existing: c.existing as Row | null, id: c.id, now: c.now })).toEqual(c.expected)
  })
  test(`composer draft empty refusal`, () => {
    expect(LOCAL_COMPOSER_DRAFT_EMPTY).toBe(`An empty draft is a delete`)
  })
  test(`draftConsumedRefusal`, () => {
    expect(draftConsumedRefusal(`GEN-7`)).toBe(`This draft was already created as GEN-7`)
  })
  test.each(fixture.comments.cases)(`createCommentRow: $name`, (c) => {
    expect(createCommentRow(c.input as Row, { teamId: team.teamId, identity: team.identity, existing: c.existing, id: c.id, now: c.now })).toEqual(c.expected)
  })
  test.each(fixture.sessions.send.cases)(`sendRows: $name`, (c) => {
    const existing = (c as { existing?: { session: Row; conversation: Row } }).existing ?? null
    const attachments = (c as { attachments?: Row[] }).attachments ?? []
    const got = sendRows(c.input as Row, { teamId: team.teamId, identity: team.identity, deviceId: fixture.sessions.deviceId, existing, attachments, nextId: ids(c.ids), now: c.now })
    if (`expected` in c) expect(got).toEqual({ rows: c.expected, output: c.output })
    if (`expectedTitle` in c) expect(got.rows[0]!.row.title).toBe(c.expectedTitle)
  })
  test(`assistantRow`, () => {
    const a = fixture.agents.assistant
    expect(assistantRow({ teamId: team.teamId, identity: team.identity, deviceId: fixture.sessions.deviceId, id: a.id, now: a.now })).toEqual(a.expected)
  })
})
