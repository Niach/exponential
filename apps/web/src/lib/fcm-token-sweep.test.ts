import { describe, expect, it, vi } from "vitest"

// Isolate the pure logic — never touch a real DB. The delete chain stub is
// swapped per test via mockDeletedRows.
let mockDeletedRows: { id: string }[] = []
vi.mock(`@/db/connection`, () => ({
  db: {
    delete: () => ({
      where: () => ({
        returning: () => Promise.resolve(mockDeletedRows),
      }),
    }),
  },
}))

import {
  FCM_TOKEN_STALE_MS,
  isFcmTokenStale,
  runFcmTokenSweep,
} from "@/lib/fcm-token-sweep"

describe(`isFcmTokenStale`, () => {
  const now = new Date(`2026-07-12T12:00:00Z`)

  it(`is true once the staleness window has fully elapsed`, () => {
    const updatedAt = new Date(now.getTime() - FCM_TOKEN_STALE_MS - 1000)
    expect(isFcmTokenStale(updatedAt, now)).toBe(true)
  })

  it(`is false while still inside the staleness window`, () => {
    const updatedAt = new Date(now.getTime() - FCM_TOKEN_STALE_MS + 60_000)
    expect(isFcmTokenStale(updatedAt, now)).toBe(false)
  })

  it(`is false for a freshly re-registered token`, () => {
    expect(isFcmTokenStale(now, now)).toBe(false)
  })
})

describe(`runFcmTokenSweep`, () => {
  it(`reports how many stale rows were deleted`, async () => {
    mockDeletedRows = [{ id: `t1` }, { id: `t2` }]
    const result = await runFcmTokenSweep()
    expect(result.tokensDeleted).toBe(2)
  })

  it(`reports zero when nothing is stale`, async () => {
    mockDeletedRows = []
    const result = await runFcmTokenSweep()
    expect(result.tokensDeleted).toBe(0)
  })
})
