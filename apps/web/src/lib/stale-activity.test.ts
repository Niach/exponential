import { describe, expect, it } from "vitest"
import {
  STALE_ACTIVITY_AFTER_MS,
  staleActivityLabel,
  staleActivityMinutes,
} from "./stale-activity"

describe(`staleActivityMinutes (FEED-26)`, () => {
  const t0 = 1_700_000_000_000

  it(`is null inside the threshold and whole minutes past it`, () => {
    expect(staleActivityMinutes(t0, t0)).toBeNull()
    expect(staleActivityMinutes(t0 + STALE_ACTIVITY_AFTER_MS - 1, t0)).toBeNull()
    expect(staleActivityMinutes(t0 + STALE_ACTIVITY_AFTER_MS, t0)).toBe(10)
    expect(staleActivityMinutes(t0 + 27 * 60_000 + 59_000, t0)).toBe(27)
  })

  it(`labels like the other phase captions`, () => {
    expect(staleActivityLabel(27, `macbook`)).toBe(`No activity for 27 min · macbook`)
    expect(staleActivityLabel(27, null)).toBe(`No activity for 27 min`)
    expect(staleActivityLabel(27, ``)).toBe(`No activity for 27 min`)
  })
})
