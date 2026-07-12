import { describe, expect, it, vi } from "vitest"
import { CODING_SESSION_STALE_MS } from "@exp/db-schema/domain"

// Isolate the pure logic — never touch a real DB.
vi.mock(`@/db/connection`, () => ({ db: {} }))

import { isCodingSessionStale } from "@/lib/coding-session-sweep"

describe(`isCodingSessionStale`, () => {
  const now = new Date(`2026-07-12T12:00:00Z`)

  it(`is false for a freshly started session`, () => {
    expect(isCodingSessionStale(now, now)).toBe(false)
  })

  it(`is false while still inside the staleness window`, () => {
    const startedAt = new Date(
      now.getTime() - CODING_SESSION_STALE_MS + 60_000
    )
    expect(isCodingSessionStale(startedAt, now)).toBe(false)
  })

  it(`is true once the staleness window has fully elapsed`, () => {
    const startedAt = new Date(
      now.getTime() - CODING_SESSION_STALE_MS - 1000
    )
    expect(isCodingSessionStale(startedAt, now)).toBe(true)
  })
})
