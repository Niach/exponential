import { describe, expect, it } from "vitest"
import {
  CODING_SESSION_STALE_MS,
  isCodingSessionStale,
} from "@exp/db-schema/domain"

describe(`isCodingSessionStale`, () => {
  const now = new Date(`2026-07-12T12:00:00Z`)

  it(`is false for a session seen just now`, () => {
    expect(isCodingSessionStale(now, now)).toBe(false)
  })

  it(`is false while still inside the staleness window`, () => {
    const lastSeenAt = new Date(
      now.getTime() - CODING_SESSION_STALE_MS + 60_000
    )
    expect(isCodingSessionStale(lastSeenAt, now)).toBe(false)
  })

  it(`is true once the staleness window has fully elapsed`, () => {
    const lastSeenAt = new Date(
      now.getTime() - CODING_SESSION_STALE_MS - 1000
    )
    expect(isCodingSessionStale(lastSeenAt, now)).toBe(true)
  })

  it(`a heartbeat (advanced last-seen stamp) resets the window`, () => {
    const startedAt = new Date(
      now.getTime() - CODING_SESSION_STALE_MS - 60 * 60 * 1000
    )
    const heartbeatAt = new Date(now.getTime() - 30 * 60 * 1000)
    // Stale by start time alone, but the sweep keys off the last liveness
    // signal — a session that keeps heartbeating never goes stale.
    expect(isCodingSessionStale(startedAt, now)).toBe(true)
    expect(isCodingSessionStale(heartbeatAt, now)).toBe(false)
  })
})
