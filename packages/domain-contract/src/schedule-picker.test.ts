import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/schedule-picker.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import {
  cronFromSchedule,
  defaultSchedule,
  scheduleFromCron,
  scheduleSummary,
  weekdaysLabel,
} from "./schedule-picker"

describe(`schedule picker fixtures (NEW-2)`, () => {
  test(`the repeat copy covers every repeat value and custom`, () => {
    for (const repeat of fixture.repeatValues) {
      expect((fixture.copy.repeat as Record<string, string>)[repeat]).toBeTruthy()
    }
    expect(fixture.copy.repeat.custom).toBeTruthy()
    expect(fixture.copy.weekdays).toHaveLength(7)
    expect(fixture.copy.weekdaysShort).toHaveLength(7)
  })

  test(`the defaults are a daily schedule at 09:00`, () => {
    expect(cronFromSchedule(defaultSchedule())).toBe(`0 9 * * *`)
  })

  // EXP-1664: independent of the runtime's tz data. The list is generated
  // from the server's ICU; a runtime with older data (mint's Bun lacks
  // tzdata 2025b's America/Coyhaique) skips the zones it does not know, so
  // the test never fails on WHERE it runs. Clients only draw the names.
  test(`the time-zone list starts with UTC, then unique sorted IANA names`, () => {
    const [first, ...rest] = contract.timeZones.names
    expect(first).toBe(`UTC`)
    expect(new Set(contract.timeZones.names).size).toBe(contract.timeZones.names.length)
    expect(rest).toEqual([...rest].sort())
    for (const zone of rest) expect(zone).toMatch(/^[A-Za-z][A-Za-z0-9_+-]*(\/[A-Za-z0-9_+-]+)+$/)
  })

  test(`every zone this runtime's ICU knows formats (the rest are newer tz data)`, () => {
    let known = 0
    for (const zone of contract.timeZones.names) {
      try {
        new Intl.DateTimeFormat(`en-US`, { timeZone: zone })
        known++
      } catch (err) {
        expect(err).toBeInstanceOf(RangeError)
      }
    }
    // A handful of new zones per tz release, never the bulk of the list.
    expect(known).toBeGreaterThan(contract.timeZones.names.length * 0.95)
  })

  for (const entry of fixture.cronCases) {
    test(`cron both ways: ${entry.name}`, () => {
      expect(cronFromSchedule(entry.schedule as never)).toBe(entry.cron)
      expect(scheduleFromCron(entry.cron)).toEqual(entry.schedule as never)
    })
  }
  for (const entry of fixture.parseCases) {
    test(`parse: ${JSON.stringify(entry.cron)}`, () => {
      expect(scheduleFromCron(entry.cron)).toEqual(entry.schedule as never)
    })
  }
  for (const entry of fixture.weekdaysLabelCases) {
    test(`weekdays label: ${entry.expected || `(none)`}`, () => {
      expect(weekdaysLabel(entry.weekdays)).toBe(entry.expected)
    })
  }
  for (const entry of fixture.summaryCases) {
    test(`summary: ${entry.expected}`, () => {
      expect(scheduleSummary(entry.cron, (entry as { tz?: string }).tz)).toBe(entry.expected)
    })
  }
})
