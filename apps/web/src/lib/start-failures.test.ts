import { afterEach, describe, expect, it, vi } from "vitest"
import {
  _clearStartFailures,
  forwardStartOnce,
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

  it(`registers a client-minted id as is (Review #3)`, () => {
    const id = crypto.randomUUID()
    expect(mintStartId(`u1`, `u1`, id)).toBe(id)
    expect(recordStartFailure({ startId: id, userId: `u1`, reason: `x` })).toBe(
      true
    )
    expect(takeStartFailure(id, `u1`)?.reason).toBe(`x`)
  })

  it(`a Retry with the same id refreshes its expiry and clears the failure`, () => {
    vi.useFakeTimers()
    const id = crypto.randomUUID()
    mintStartId(`owner`, `teammate`, id)
    recordStartFailure({ startId: id, userId: `owner`, reason: `old` })
    vi.advanceTimersByTime(9 * 60 * 1000)
    expect(mintStartId(`owner`, `teammate`, id)).toBe(id)
    expect(takeStartFailure(id, `teammate`)).toBeNull()
    vi.advanceTimersByTime(9 * 60 * 1000)
    expect(
      recordStartFailure({ startId: id, userId: `owner`, reason: `new` })
    ).toBe(true)
    expect(takeStartFailure(id, `teammate`)?.reason).toBe(`new`)
  })

  it(`an id held by another owner or requester gets a fresh one`, () => {
    const id = crypto.randomUUID()
    mintStartId(`u1`, `u1`, id)
    const otherOwner = mintStartId(`u2`, `u2`, id)
    expect(otherOwner).not.toBe(id)
    const otherRequester = mintStartId(`u1`, `u3`, id)
    expect(otherRequester).not.toBe(id)
    expect(recordStartFailure({ startId: id, userId: `u1`, reason: `x` })).toBe(
      true
    )
    expect(takeStartFailure(id, `u1`)?.reason).toBe(`x`)
    expect(recordStartFailure({ startId: otherOwner, userId: `u2`, reason: `y` })).toBe(
      true
    )
  })

  it(`a consumed id registers again on Retry`, () => {
    const id = crypto.randomUUID()
    mintStartId(`u1`, `u1`, id)
    recordStartFailure({ startId: id, userId: `u1`, reason: `x` })
    takeStartFailure(id, `u1`)
    expect(mintStartId(`u1`, `u1`, id)).toBe(id)
    expect(recordStartFailure({ startId: id, userId: `u1`, reason: `y` })).toBe(
      true
    )
  })
})

// Recheck R1: a forward the device then reported as failed is no answer any
// more: the next Start/Retry of the same draft forwards again.
describe(`forwardStartOnce after a device failure`, () => {
  const forwardFor = (owner: string, requester: string, startId: string) =>
    vi.fn(async () => ({ ok: true as const, startId: mintStartId(owner, requester, startId) }))

  it(`accept, then the device fails it: Retry forwards again`, async () => {
    const startId = crypto.randomUUID()
    const key = { requesterId: `u1`, deviceId: `dev-1`, startId }
    const forward = forwardFor(`u1`, `u1`, startId)
    await forwardStartOnce(key, forward)
    // Before the report the repeat is deduped.
    await forwardStartOnce(key, forward)
    expect(forward).toHaveBeenCalledTimes(1)
    expect(
      recordStartFailure({ startId, userId: `u1`, reason: `dirty tree` })
    ).toBe(true)
    await expect(forwardStartOnce(key, forward)).resolves.toEqual({ ok: true, startId })
    expect(forward).toHaveBeenCalledTimes(2)
    // The fresh forward is remembered again.
    await forwardStartOnce(key, forward)
    expect(forward).toHaveBeenCalledTimes(2)
  })

  it(`a shared-device start: the owner's report frees the requester's Retry`, async () => {
    const startId = crypto.randomUUID()
    const key = { requesterId: `teammate`, deviceId: `dev-1`, startId }
    const forward = forwardFor(`owner`, `teammate`, startId)
    await forwardStartOnce(key, forward)
    expect(recordStartFailure({ startId, userId: `owner`, reason: `signed out` })).toBe(true)
    await forwardStartOnce(key, forward)
    expect(forward).toHaveBeenCalledTimes(2)
  })

  it(`an unaccepted report keeps the dedupe`, async () => {
    const startId = crypto.randomUUID()
    const key = { requesterId: `u1`, deviceId: `dev-1`, startId }
    const forward = forwardFor(`u1`, `u1`, startId)
    await forwardStartOnce(key, forward)
    expect(recordStartFailure({ startId, userId: `u2`, reason: `x` })).toBe(false)
    await forwardStartOnce(key, forward)
    expect(forward).toHaveBeenCalledTimes(1)
  })

  it(`a failure under the id the frame carried frees the client's id`, async () => {
    const startId = crypto.randomUUID()
    // Another owner holds the client's id: the frame carries a fresh one.
    mintStartId(`someone`, `someone`, startId)
    const key = { requesterId: `u1`, deviceId: `dev-1`, startId }
    const forward = forwardFor(`u1`, `u1`, startId)
    const first = await forwardStartOnce(key, forward)
    expect(first.startId).not.toBe(startId)
    expect(
      recordStartFailure({ startId: first.startId, userId: `u1`, reason: `x` })
    ).toBe(true)
    await forwardStartOnce(key, forward)
    expect(forward).toHaveBeenCalledTimes(2)
  })
})
