import { describe, expect, it } from "vitest"
import { cronMatches, latestCronMatch, parseCron, wallClock } from "./cron"

const at = (iso: string) => new Date(iso)

describe(`parseCron`, () => {
  it(`parses stars, lists, ranges, steps and names`, () => {
    const fields = parseCron(`*/15 9-17 1,15 jan-mar mon-fri`)!
    expect([...fields.minute]).toEqual([0, 15, 30, 45])
    expect([...fields.hour]).toEqual([9, 10, 11, 12, 13, 14, 15, 16, 17])
    expect([...fields.dom]).toEqual([1, 15])
    expect([...fields.month]).toEqual([1, 2, 3])
    expect([...fields.dow]).toEqual([1, 2, 3, 4, 5])
  })
  it(`folds 7 onto Sunday`, () => {
    expect([...parseCron(`0 0 * * 7`)!.dow]).toEqual([0])
  })
  it(`refuses anything that is not five valid fields`, () => {
    for (const bad of [``, `* * * *`, `60 * * * *`, `* 24 * * *`, `* * 0 * *`, `* * * 13 *`, `a * * * *`, `*/0 * * * *`, `5-1 * * * *`]) {
      expect(parseCron(bad), bad).toBeNull()
    }
  })
})

describe(`wallClock`, () => {
  it(`reads the zone's clock`, () => {
    const date = at(`2026-10-21T07:30:00Z`)
    expect(wallClock(date, `UTC`)).toEqual({ minute: 30, hour: 7, dom: 21, month: 10, dow: 3 })
    expect(wallClock(date, `Europe/Vienna`)).toEqual({ minute: 30, hour: 9, dom: 21, month: 10, dow: 3 })
    expect(wallClock(at(`2026-10-21T23:30:00Z`), `Asia/Tokyo`).dom).toBe(22)
  })
  it(`falls back to UTC on an unknown zone`, () => {
    expect(wallClock(at(`2026-10-21T07:30:00Z`), `Mars/Olympus`).hour).toBe(7)
  })
})

describe(`cronMatches`, () => {
  it(`matches minute, hour, weekday in the zone`, () => {
    const nine = parseCron(`0 9 * * 1-5`)!
    expect(cronMatches(nine, at(`2026-10-21T07:00:00Z`), `Europe/Vienna`)).toBe(true)
    expect(cronMatches(nine, at(`2026-10-21T09:00:00Z`), `Europe/Vienna`)).toBe(false)
    expect(cronMatches(nine, at(`2026-10-24T07:00:00Z`), `Europe/Vienna`)).toBe(false)
  })
  it(`uses the vixie OR when both day fields are restricted`, () => {
    const either = parseCron(`0 0 15 * 1`)!
    expect(cronMatches(either, at(`2026-10-15T00:00:00Z`), `UTC`)).toBe(true)
    expect(cronMatches(either, at(`2026-10-19T00:00:00Z`), `UTC`)).toBe(true)
    expect(cronMatches(either, at(`2026-10-20T00:00:00Z`), `UTC`)).toBe(false)
  })
})

describe(`latestCronMatch`, () => {
  const every10 = parseCron(`*/10 * * * *`)!
  it(`finds the newest matching minute inside the window`, () => {
    const found = latestCronMatch(every10, at(`2026-10-21T07:01:00Z`), at(`2026-10-21T07:23:30Z`), `UTC`)
    expect(found?.toISOString()).toBe(`2026-10-21T07:20:00.000Z`)
  })
  it(`excludes the window's start and returns null when nothing matched`, () => {
    expect(latestCronMatch(every10, at(`2026-10-21T07:20:10Z`), at(`2026-10-21T07:29:00Z`), `UTC`)).toBeNull()
  })
  it(`catches up a daily schedule missed inside the window`, () => {
    const daily = parseCron(`0 9 * * *`)!
    const found = latestCronMatch(daily, at(`2026-10-21T06:00:00Z`), at(`2026-10-21T11:00:00Z`), `UTC`)
    expect(found?.toISOString()).toBe(`2026-10-21T09:00:00.000Z`)
  })
})
