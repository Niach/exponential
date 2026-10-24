import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-records.json" with { type: "json" }
import { monthDays, parseRecordsQuery, recordsCountLabel, recordsValue, shiftMonth } from "./app-records"
import type { AppRecordRow } from "./app-records"

describe(`app records (VAPP-122)`, () => {
  test.each(fixture.queryCases)(`parseRecordsQuery: $name`, (c) => {
    expect(parseRecordsQuery(c.params)).toEqual(c.expected as never)
  })
  test.each(fixture.valueCases)(`recordsValue: $name`, (c) => {
    const value = recordsValue(c.rows as AppRecordRow[], parseRecordsQuery(c.params), { me: c.me, now: c.now, names: c.names })
    expect(value).toEqual(c.expected as never)
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
