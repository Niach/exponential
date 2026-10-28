import { describe, expect, it } from "vitest"
import { resolveSessionDevice } from "./session-device"

// Parity suite — iOS SessionDevicePresentationTests.swift, Android
// SessionDevicePresentationTest.kt and desktop queries.rs assert the same
// cases; move all of them in lockstep.

const NOW = new Date(`2026-08-19T12:00:00Z`)
const fresh = new Date(NOW.getTime() - 10_000)
const stale = new Date(NOW.getTime() - 10 * 60_000)

function device(
  overrides: Partial<{
    deviceId: string
    label: string
    userId: string
    lastSeenAt: Date
  }> = {}
) {
  return {
    deviceId: `dev-1`,
    label: `macbook`,
    userId: `me`,
    lastSeenAt: fresh,
    ...overrides,
  }
}

describe(`resolveSessionDevice`, () => {
  it(`resolves the registry label by device_id`, () => {
    expect(resolveSessionDevice({ deviceId: `dev-1`, userId: `me` }, [device()], NOW)).toEqual({
      label: `macbook`,
      online: true,
    })
  })

  it(`prefers the session starter's row when two users share a device id`, () => {
    expect(
      resolveSessionDevice(
        { deviceId: `dev-1`, userId: `me` },
        [device({ userId: `other`, label: `theirs` }), device({ userId: `me`, label: `mine` })],
        NOW
      ).label
    ).toBe(`mine`)
  })

  it(`reads offline once last_seen_at is past the online window`, () => {
    expect(
      resolveSessionDevice({ deviceId: `dev-1`, userId: `me` }, [device({ lastSeenAt: stale })], NOW)
    ).toEqual({ label: `macbook`, online: false })
  })

  it(`an id matching no row is unknown`, () => {
    expect(resolveSessionDevice({ deviceId: `dev-9`, userId: `me` }, [device()], NOW)).toEqual({
      label: null,
      online: null,
    })
  })
})
