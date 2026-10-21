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

  test(`the time-zone list starts with UTC and every zone is one ICU knows`, () => {
    expect(contract.timeZones.names[0]).toBe(`UTC`)
    expect(new Set(contract.timeZones.names).size).toBe(contract.timeZones.names.length)
    for (const zone of contract.timeZones.names) {
      expect(() => new Intl.DateTimeFormat(`en-US`, { timeZone: zone })).not.toThrow()
    }
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
