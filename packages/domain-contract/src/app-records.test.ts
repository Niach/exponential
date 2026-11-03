import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-records.json" with { type: "json" }
import { eventLabel, eventState, historyRows, inverseWrite, outboxFlush, outboxOutcome, outboxPendingLabel, outboxPrune, outboxRequest, outboxRows, outboxWrites, APP_RECORD_ROW_KEYS, appRecordPurgeDue, authorOptions, canChangeRecord, formAuthor, importAuthor, recordAuthor, recordByline, writeAsVerdict, cellText, dataColumns, dataKinds, dataRows, formData, formFields, formInput, recordsCountLabel } from "./app-records"

// VAPP-189: the query and its value are eUI's (packages/exponential-ui
// fixtures/records-query.json); what stays here is Exponential's own.

describe(`app records (VAPP-122)`, () => {
  test(`the reserved row keys`, () => {
    expect([...APP_RECORD_ROW_KEYS]).toEqual([`id`, `createdAt`, `updatedAt`, `createdBy`, `authorId`, `author`])
  })
  test(`a record's author is authorId, else createdBy`, () => {
    expect(recordAuthor({ authorId: `u2`, createdBy: `u1` })).toBe(`u2`)
    expect(recordAuthor({ authorId: null, createdBy: `u1` })).toBe(`u1`)
    expect(recordAuthor({ createdBy: null })).toBeNull()
  })
  test.each(fixture.purge.cases)(`appRecordPurgeDue: $name`, (c) => {
    const now = new Date(`2026-10-30T12:00:00.000Z`)
    const ago = (days: number | null) => (days === null ? null : new Date(now.getTime() - days * 24 * 60 * 60 * 1000))
    expect(appRecordPurgeDue(ago(c.deletedAt), ago(c.appDeletedAt), now)).toBe(c.purged)
  })
  test.each(fixture.ownership.canChangeCases)(`canChangeRecord: $name`, (c) => {
    const row = { authorId: c.author, createdBy: c.createdBy }
    expect({ update: canChangeRecord(row, c.viewer, c.role, `update`), delete: canChangeRecord(row, c.viewer, c.role, `delete`) }).toEqual(c.expected)
  })
  test.each(fixture.onBehalf.writeAsCases)(`writeAsVerdict: $name`, (c) => {
    expect(writeAsVerdict(c.viewer, c.role, c.author, c.members)).toBe(c.expected as never)
  })
  test.each(fixture.onBehalf.importCases)(`importAuthor: $name`, (c) => {
    expect(importAuthor(c.importer, c.role, c.author, c.members)).toBe(c.expected)
  })
  test.each(fixture.dataTab.bylineCases)(`recordByline: $name`, (c) => {
    expect(recordByline({ authorId: c.authorId, createdBy: c.createdBy }, c.names as Record<string, string>, c.fallback)).toBe(c.text)
  })
  test.each(fixture.dataTab.authorOptionCases)(`authorOptions %#`, (c) => {
    expect(authorOptions(c.users).map((u) => u.id)).toEqual(c.order)
  })
  test.each(fixture.dataTab.formAuthorCases)(`formAuthor: $name`, (c) => {
    expect(formAuthor(c.viewer, c.record, c.picked)).toEqual({ default: c.default, send: c.send })
  })
  test.each(fixture.countLabelCases)(`recordsCountLabel %j`, (c) => {
    expect(recordsCountLabel(c.count, c.max)).toBe(c.label)
  })
})

describe(`the Data tab (VAPP-178)`, () => {
  const tab = fixture.dataTab
  test.each(tab.kindCases)(`dataKinds %j`, (c) => {
    expect(dataKinds(c.manifest as never, c.rows)).toEqual(c.kinds)
  })
  test.each(tab.columnCases)(`dataColumns %j`, (c) => {
    expect(dataColumns(c.schema as never)).toEqual(c.columns)
  })
  test.each(tab.rowsCases)(`dataRows %j`, (c) => {
    expect(dataRows(c.rows, c.kind).map((row) => row.id)).toEqual(c.order)
  })
  test.each(tab.cellCases)(`cellText %j`, (c) => {
    expect(cellText((c as { value?: unknown }).value)).toBe(c.text)
  })
  test.each(tab.formCases)(`formFields %j`, (c) => {
    expect(formFields(c.schema as never)).toEqual(c.fields as never)
  })
  test.each(tab.inputCases)(`formInput %j`, (c) => {
    expect(formInput(c.control as never, c.value)).toBe(c.input)
  })
  test.each(tab.parseCases)(`formData %j`, (c) => {
    expect(formData(c.schema as never, c.inputs as never, c.editing)).toEqual(c.data)
  })
})

describe(`record history (VAPP-215)`, () => {
  const h = fixture.history
  test.each(h.stateCases)(`eventState: $name`, (c) => {
    expect(eventState(c.event as never)).toEqual(c.state)
  })
  test.each(h.inverseCases)(`inverseWrite: $name`, (c) => {
    expect(inverseWrite(c.event as never)).toEqual(c.inverse)
  })
  test.each(h.labelCases)(`eventLabel: $name`, (c) => {
    expect(eventLabel(c.event as never)).toBe(c.label)
  })
  test.each(h.rowsCases)(`historyRows: $name`, (c) => {
    expect(historyRows(c.events as never, c.names as Record<string, string>, c.fallback)).toEqual(c.rows)
  })
})

describe(`record outbox (VAPP-235)`, () => {
  const o = fixture.outbox
  test.each(o.writesCases)(`outboxWrites: $name`, (c) => {
    expect(outboxWrites(c.call as never, c.args as never, c.newIds)).toEqual(c.writes as never)
  })
  test.each(o.requestCases)(`outboxRequest: $name`, (c) => {
    expect(outboxRequest(c.entry as never)).toEqual(c.request as never)
  })
  test.each(o.rowsCases)(`outboxRows: $name`, (c) => {
    expect(outboxRows(c.rows as never, c.entries as never, c.me)).toEqual(c.expected as never)
  })
  test.each(o.outcomeCases)(`outboxOutcome: $name`, (c) => {
    expect(outboxOutcome(c.entry as never, c.result as never)).toBe(c.outcome as never)
  })
  test.each(o.flushCases)(`outboxFlush: $name`, (c) => {
    expect(outboxFlush(c.entries as never, c.results as never, c.now)).toEqual({ sent: c.sent, refused: c.refused, after: c.after } as never)
  })
  test.each(o.pruneCases)(`outboxPrune: $name`, (c) => {
    expect(outboxPrune(c.entries as never, c.rows as never, c.now).map((e) => e.writeId)).toEqual(c.kept)
  })
  test.each(o.pendingCases)(`outboxPendingLabel: $name`, (c) => {
    expect(outboxPendingLabel(c.entries as never, c.appId)).toBe(c.label)
  })
})
