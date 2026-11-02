import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-records.json" with { type: "json" }
import { appRecordPurgeDue, authorOptions, canChangeRecord, formAuthor, importAuthor, recordByline, writeAsVerdict, cellText, dataColumns, dataKinds, dataRows, formData, formFields, formInput, monthDays, parseRecordsQuery, recordsSourceParams, recordsCountLabel, recordsValue, shiftMonth } from "./app-records"
import type { AppRecordRow } from "./app-records"

// The SDK's parseSource, inlined (domain-contract does not depend on the SDK).
const parseSource = (uri: string) => {
  const [, scheme, name, query] = /^([a-zA-Z][a-zA-Z0-9+.-]*):([^?]+)(?:\?(.*))?$/.exec(uri)!
  return { scheme: scheme!.toLowerCase(), name: name!, params: Object.fromEntries(new URLSearchParams(query ?? ``)) }
}

describe(`app records (VAPP-122)`, () => {
  test.each(fixture.queryCases)(`parseRecordsQuery: $name`, (c) => {
    expect(parseRecordsQuery(c.params)).toEqual(c.expected as never)
  })
  test.each(fixture.sourceCases)(`recordsSourceParams: $name`, (c) => {
    const params = recordsSourceParams(parseSource(c.source))
    expect(params ? parseRecordsQuery(params) : null).toEqual(c.expected as never)
  })
  test.each(fixture.purge.cases)(`appRecordPurgeDue: $name`, (c) => {
    const now = new Date(`2026-10-30T12:00:00.000Z`)
    const ago = (days: number | null) => (days === null ? null : new Date(now.getTime() - days * 24 * 60 * 60 * 1000))
    expect(appRecordPurgeDue(ago(c.deletedAt), ago(c.appDeletedAt), now)).toBe(c.purged)
  })
  test.each(fixture.valueCases)(`recordsValue: $name`, (c) => {
    const value = recordsValue(c.rows as AppRecordRow[], parseRecordsQuery(c.params), { me: c.me, now: c.now, names: c.names })
    expect(value).toEqual(c.expected as never)
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
  test(`months wrap and leap years count`, () => {
    expect(shiftMonth(`2026-01`, -1)).toBe(`2025-12`)
    expect(shiftMonth(`2026-12`, 1)).toBe(`2027-01`)
    expect(monthDays(`2028-02`)).toHaveLength(29)
    expect(monthDays(`2026-02`)).toHaveLength(28)
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
