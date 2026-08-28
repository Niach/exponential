import { describe, expect, it } from "vitest"
import { DEMO_PENDING_INVITE_EXPIRY } from "./screenshot-demo"

/** How much runway the pinned expiry must keep before this test starts failing. */
const MIN_RUNWAY_DAYS = 120

describe(`pinned demo invite expiry (EXP-668)`, () => {
  it(`is far enough out that the seed cannot start producing expired invites`, () => {
    const runwayDays = Object.values(DEMO_PENDING_INVITE_EXPIRY).map(
      (date) => (date.getTime() - Date.now()) / 86_400_000
    )
    for (const days of runwayDays) {
      // Push the dates out (and re-capture `settings-members`) when this
      // fails — it is meant to fail with months to spare, not on the day the
      // demo team's pending invites start rendering as expired.
      expect(days).toBeGreaterThan(MIN_RUNWAY_DAYS)
    }
  })

  it(`keeps the two rows on distinct dates, so the list has a stable order`, () => {
    const times = Object.values(DEMO_PENDING_INVITE_EXPIRY).map((date) =>
      date.getTime()
    )
    expect(new Set(times).size).toBe(times.length)
  })
})
