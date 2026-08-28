import { formatDistanceToNowStrict } from "date-fns"
import { describe, expect, it } from "vitest"
import {
  DEMO_PENDING_INVITE_EXPIRY,
  DEMO_SHOWCASE_COMMENT_HOURS_AGO,
} from "./screenshot-demo"

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

/**
 * How long a capture run may take before a pinned comment offset is allowed to
 * change what it renders.
 *
 * The gap that matters is seed -> shutter, and the last lane to photograph this
 * thread is a native styleguide suite several lanes after the seed. Eight hours
 * is far past any real run and still inside the 12-hour budget the fleet's
 * bucket boundaries allow, so a failure here means an offset was moved to a bad
 * place rather than that runs got slower.
 */
const COMMENT_LABEL_RUNWAY_HOURS = 8

/** What web renders: date-fns `formatDistanceToNowStrict`, which ROUNDS. */
const webLabel = (hoursAgo: number) =>
  formatDistanceToNowStrict(new Date(Date.now() - hoursAgo * 3_600_000), {
    addSuffix: true,
  })

/**
 * What iOS, Android and desktop render: all three FLOOR on calendar
 * boundaries, so they agree with each other and split from web in the back half
 * of every day. Only the unit and count matter here, not the wording.
 */
const nativeLabel = (hoursAgo: number) =>
  hoursAgo < 24 ? `${Math.floor(hoursAgo)}h` : `${Math.floor(hoursAgo / 24)}d`

describe(`pinned showcase comment offsets (EXP-669)`, () => {
  const offsets = Object.entries(DEMO_SHOWCASE_COMMENT_HOURS_AGO)

  it.each(offsets)(
    `%s renders the same label however late in the run the shutter fires`,
    (_author, hours) => {
      // Sampled rather than checked at the endpoints only: the label is
      // monotonic in elapsed time, but sampling says WHICH hour broke it.
      for (let elapsed = 0; elapsed <= COMMENT_LABEL_RUNWAY_HOURS; elapsed++) {
        expect(webLabel(hours + elapsed)).toBe(webLabel(hours))
        expect(nativeLabel(hours + elapsed)).toBe(nativeLabel(hours))
      }
    }
  )

  it.each(offsets)(
    `%s is day-granular, which is what makes it survive that gap at all`,
    (_author, hours) => {
      expect(webLabel(hours)).toMatch(/^\d+ days? ago$/)
      expect(nativeLabel(hours)).toMatch(/^\d+d$/)
    }
  )

  it.each(offsets)(
    `%s reads the same on web as on the natives, despite round vs floor`,
    (_author, hours) => {
      const webDays = webLabel(hours).match(/^(\d+) days? ago$/)?.[1]
      expect(nativeLabel(hours)).toBe(`${webDays}d`)
    }
  )

  it(`keeps the thread in order, newest last`, () => {
    const hours = offsets.map(([, value]) => value)
    expect(hours).toStrictEqual([...hours].sort((a, b) => b - a))
    expect(new Set(hours).size).toBe(hours.length)
  })

  it(`posts every comment after the showcase issue was created`, () => {
    // `seedIssues` gives APP-5 `createdDaysAgo: 3`; a comment seeded before
    // that renders above the "created the issue" row and reads as nonsense.
    for (const [, hours] of offsets) {
      expect(hours).toBeLessThan(3 * 24)
    }
  })
})
