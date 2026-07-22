import { describe, expect, it } from "vitest"
import {
  REPORTER_PRESENCE_WINDOW_MS,
  isReporterActivelyViewing,
  parsePollSince,
} from "./presence"

const now = new Date(`2026-07-22T12:00:00.000Z`)

function secondsAgo(seconds: number): Date {
  return new Date(now.getTime() - seconds * 1000)
}

describe(`isReporterActivelyViewing`, () => {
  it(`is false for null and undefined`, () => {
    expect(isReporterActivelyViewing(null, now)).toBe(false)
    expect(isReporterActivelyViewing(undefined, now)).toBe(false)
  })

  it(`is true within the presence window`, () => {
    expect(isReporterActivelyViewing(secondsAgo(10), now)).toBe(true)
    expect(
      isReporterActivelyViewing(
        new Date(now.getTime() - REPORTER_PRESENCE_WINDOW_MS + 1),
        now
      )
    ).toBe(true)
  })

  it(`is false at and beyond the window boundary`, () => {
    expect(
      isReporterActivelyViewing(
        new Date(now.getTime() - REPORTER_PRESENCE_WINDOW_MS),
        now
      )
    ).toBe(false)
    expect(isReporterActivelyViewing(secondsAgo(120), now)).toBe(false)
  })

  it(`accepts ISO string input`, () => {
    expect(isReporterActivelyViewing(secondsAgo(5).toISOString(), now)).toBe(
      true
    )
    expect(isReporterActivelyViewing(secondsAgo(60).toISOString(), now)).toBe(
      false
    )
  })

  it(`treats garbage strings as not viewing`, () => {
    expect(isReporterActivelyViewing(`not-a-date`, now)).toBe(false)
    expect(isReporterActivelyViewing(``, now)).toBe(false)
  })

  it(`treats a future stamp (clock skew) as viewing`, () => {
    expect(
      isReporterActivelyViewing(new Date(now.getTime() + 5000), now)
    ).toBe(true)
  })
})

describe(`parsePollSince`, () => {
  it(`parses a valid ISO timestamp`, () => {
    const since = parsePollSince(`2026-07-22T11:59:55.123Z`)
    expect(since).toBeInstanceOf(Date)
    expect(since?.toISOString()).toBe(`2026-07-22T11:59:55.123Z`)
  })

  it(`returns null for non-strings and garbage`, () => {
    expect(parsePollSince(undefined)).toBeNull()
    expect(parsePollSince(null)).toBeNull()
    expect(parsePollSince(1753185595000)).toBeNull()
    expect(parsePollSince({})).toBeNull()
    expect(parsePollSince(``)).toBeNull()
    expect(parsePollSince(`yesterday-ish`)).toBeNull()
  })
})
