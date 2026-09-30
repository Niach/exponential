import { afterEach, describe, expect, it, vi } from "vitest"
import {
  _clearStartFailures,
  mintStartId,
  recordStartFailure,
  takeStartFailure,
} from "./start-failures"

afterEach(() => {
  _clearStartFailures()
  vi.useRealTimers()
})

describe(`start failures (FEED-63)`, () => {
  it(`records and takes a failure once, for the minter`, () => {
    const id = mintStartId(`u1`)
    expect(id).toMatch(/^[0-9a-f-]{36}$/)
    expect(takeStartFailure(id, `u1`)).toBeNull()
    expect(
      recordStartFailure({ startId: id, userId: `u1`, reason: `dirty tree` })
    ).toBe(true)
    const failure = takeStartFailure(id, `u1`)
    expect(failure?.reason).toBe(`dirty tree`)
    expect(failure?.at).toBeInstanceOf(Date)
    expect(takeStartFailure(id, `u1`)).toBeNull()
  })

  it(`drops reports for unknown ids and from other users`, () => {
    expect(
      recordStartFailure({
        startId: crypto.randomUUID(),
        userId: `u1`,
        reason: `x`,
      })
    ).toBe(false)
    const id = mintStartId(`u1`)
    expect(recordStartFailure({ startId: id, userId: `u2`, reason: `x` })).toBe(
      false
    )
    expect(takeStartFailure(id, `u1`)).toBeNull()
  })

  it(`never hands a failure to another user`, () => {
    const id = mintStartId(`u1`)
    recordStartFailure({ startId: id, userId: `u1`, reason: `x` })
    expect(takeStartFailure(id, `u2`)).toBeNull()
    expect(takeStartFailure(id, `u1`)?.reason).toBe(`x`)
  })

  it(`a shared-device start takes the owner's report for the requester`, () => {
    const id = mintStartId(`owner`, `teammate`)
    expect(
      recordStartFailure({ startId: id, userId: `teammate`, reason: `x` })
    ).toBe(false)
    expect(
      recordStartFailure({ startId: id, userId: `owner`, reason: `y` })
    ).toBe(true)
    expect(takeStartFailure(id, `owner`)).toBeNull()
    expect(takeStartFailure(id, `teammate`)?.reason).toBe(`y`)
  })

  it(`expires after 10 minutes`, () => {
    vi.useFakeTimers()
    const id = mintStartId(`u1`)
    recordStartFailure({ startId: id, userId: `u1`, reason: `x` })
    vi.advanceTimersByTime(10 * 60 * 1000 + 1)
    expect(takeStartFailure(id, `u1`)).toBeNull()
    const late = mintStartId(`u1`)
    vi.advanceTimersByTime(10 * 60 * 1000 + 1)
    expect(recordStartFailure({ startId: late, userId: `u1`, reason: `x` })).toBe(
      false
    )
  })

  it(`caps the live population, evicting the oldest`, () => {
    const first = mintStartId(`u1`)
    for (let i = 0; i < 1000; i++) mintStartId(`u1`)
    expect(recordStartFailure({ startId: first, userId: `u1`, reason: `x` })).toBe(
      false
    )
  })
})
