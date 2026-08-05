// EXP-420: the machine-row Update button must render only when a newer CLI
// version actually exists (or an update is already in flight) — not for
// every online server.
import { describe, expect, it } from "vitest"

import {
  deviceUpdateAvailable,
  showDeviceUpdateButton,
  type SteerDevice,
} from "./steer-devices"

function server(overrides: Partial<SteerDevice> = {}): SteerDevice {
  return {
    deviceId: `dev-1`,
    deviceLabel: `homelab`,
    kind: `server`,
    online: true,
    registered: true,
    version: `0.14.1`,
    ...overrides,
  }
}

describe(`deviceUpdateAvailable`, () => {
  it(`compares numerically, not lexicographically`, () => {
    expect(deviceUpdateAvailable(`0.9.9`, `0.10.0`)).toBe(true)
    expect(deviceUpdateAvailable(`0.14.1`, `0.14.2`)).toBe(true)
    expect(deviceUpdateAvailable(`0.14.2`, `0.14.2`)).toBe(false)
    expect(deviceUpdateAvailable(`0.15.0`, `0.14.2`)).toBe(false)
  })

  it(`fails closed on unknown or unparsable versions`, () => {
    expect(deviceUpdateAvailable(null, `0.14.2`)).toBe(false)
    expect(deviceUpdateAvailable(`0.14.1`, null)).toBe(false)
    expect(deviceUpdateAvailable(undefined, undefined)).toBe(false)
    expect(deviceUpdateAvailable(`not-a-version`, `0.14.2`)).toBe(false)
  })
})

describe(`showDeviceUpdateButton`, () => {
  it(`shows only for an outdated online registered server`, () => {
    expect(showDeviceUpdateButton(server(), `0.14.2`)).toBe(true)
  })

  it(`hides when the device is current`, () => {
    expect(showDeviceUpdateButton(server({ version: `0.14.2` }), `0.14.2`)).toBe(
      false
    )
  })

  it(`hides when the latest version is unknown (env unset)`, () => {
    expect(showDeviceUpdateButton(server(), null)).toBe(false)
    expect(showDeviceUpdateButton(server(), undefined)).toBe(false)
  })

  it(`hides for desktops, offline and unregistered rows`, () => {
    expect(showDeviceUpdateButton(server({ kind: `desktop` }), `0.14.2`)).toBe(
      false
    )
    expect(showDeviceUpdateButton(server({ online: false }), `0.14.2`)).toBe(
      false
    )
    expect(
      showDeviceUpdateButton(server({ registered: false }), `0.14.2`)
    ).toBe(false)
    expect(
      showDeviceUpdateButton(server({ registered: undefined }), `0.14.2`)
    ).toBe(false)
  })

  it(`keeps an in-flight update visible even once versions look current`, () => {
    // The daemon's re-register clears updateRequested and carries the new
    // version — until then the row must keep saying "Updating…"/"Queued".
    expect(
      showDeviceUpdateButton(
        server({ version: `0.14.2`, updateRequested: true }),
        `0.14.2`
      )
    ).toBe(true)
    expect(
      showDeviceUpdateButton(server({ updateRequested: true }), null)
    ).toBe(true)
  })
})
