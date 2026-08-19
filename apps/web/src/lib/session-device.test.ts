import { describe, expect, it } from "vitest"
import { resolveSessionDevice, sessionIsPaused } from "./session-device"

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
  it(`resolves the renamed registry label by device_id (snapshot ignored)`, () => {
    expect(
      resolveSessionDevice(
        { deviceId: `dev-1`, deviceLabel: `MacBook-Pro.local`, userId: `me` },
        [device()],
        NOW
      )
    ).toEqual({ label: `macbook`, online: true })
  })

  it(`prefers the session owner's row when two users share a device id`, () => {
    expect(
      resolveSessionDevice(
        { deviceId: `dev-1`, deviceLabel: null, userId: `me` },
        [
          device({ userId: `other`, label: `theirs` }),
          device({ userId: `me`, label: `mine` }),
        ],
        NOW
      ).label
    ).toBe(`mine`)
  })

  it(`reads offline once last_seen_at is past the online window`, () => {
    expect(
      resolveSessionDevice(
        { deviceId: `dev-1`, deviceLabel: null, userId: `me` },
        [device({ lastSeenAt: stale })],
        NOW
      )
    ).toEqual({ label: `macbook`, online: false })
  })

  it(`falls back to the snapshot with unknown online-ness when the id matches nothing`, () => {
    expect(
      resolveSessionDevice(
        { deviceId: `dev-9`, deviceLabel: `box`, userId: `me` },
        [device({ label: `box` })],
        NOW
      )
    ).toEqual({ label: `box`, online: null })
  })

  it(`legacy rows (no id) match a UNIQUE row by label`, () => {
    expect(
      resolveSessionDevice(
        { deviceId: null, deviceLabel: `macbook`, userId: `me` },
        [device({ lastSeenAt: stale }), device({ deviceId: `dev-2`, label: `server` })],
        NOW
      )
    ).toEqual({ label: `macbook`, online: false })
  })

  it(`legacy rows with an ambiguous label resolve to the snapshot only`, () => {
    expect(
      resolveSessionDevice(
        { deviceId: null, deviceLabel: `macbook`, userId: `me` },
        [device(), device({ deviceId: `dev-2` })],
        NOW
      )
    ).toEqual({ label: `macbook`, online: null })
  })

  it(`a session that never named a device stays label-less`, () => {
    expect(
      resolveSessionDevice(
        { deviceId: null, deviceLabel: null, userId: `me` },
        [device()],
        NOW
      )
    ).toEqual({ label: null, online: null })
  })
})

describe(`sessionIsPaused`, () => {
  const offline = { label: `macbook`, online: false }
  it(`pauses running and needs_input on an offline device`, () => {
    expect(sessionIsPaused(`running`, offline)).toBe(true)
    expect(sessionIsPaused(`needs_input`, offline)).toBe(true)
  })
  it(`never overrides review/merged/done`, () => {
    expect(sessionIsPaused(`review`, offline)).toBe(false)
    expect(sessionIsPaused(`merged`, offline)).toBe(false)
    expect(sessionIsPaused(`done`, offline)).toBe(false)
  })
  it(`unknown or online devices never pause`, () => {
    expect(sessionIsPaused(`running`, { label: `x`, online: null })).toBe(false)
    expect(sessionIsPaused(`running`, { label: `x`, online: true })).toBe(false)
  })
})
