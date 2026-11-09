import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { describe, expect, it } from "vitest"
import { DEVICE_CLIENTS, deviceClientName, isAllowedDeviceClient } from "./device-clients"

// GHSA-q84f-53jg-9ppm: the device-code grant's client allow-list.
describe(`device-code client allow-list`, () => {
  it(`allows the desktop/CLI client and rejects every other id`, () => {
    expect(isAllowedDeviceClient(`exponential-cli`)).toBe(true)
    expect(isAllowedDeviceClient(`evil-client`)).toBe(false)
    expect(isAllowedDeviceClient(``)).toBe(false)
    // Prototype keys are not clients.
    expect(isAllowedDeviceClient(`toString`)).toBe(false)
    expect(isAllowedDeviceClient(`__proto__`)).toBe(false)
  })

  it(`names only allow-listed clients`, () => {
    expect(deviceClientName(`exponential-cli`)).toBe(`Exponential desktop or CLI`)
    expect(deviceClientName(`evil-client`)).toBeNull()
    expect(deviceClientName(null)).toBeNull()
  })

  it(`matches the client_id the desktop/CLI sends (login.rs DEVICE_CLIENT_ID)`, () => {
    const loginRs = readFileSync(
      resolve(__dirname, `../../../../desktop/crates/api/src/login.rs`),
      `utf8`
    )
    const id = loginRs.match(/pub const DEVICE_CLIENT_ID: &str = "([^"]+)";/)?.[1]
    expect(id).toBeDefined()
    expect(Object.keys(DEVICE_CLIENTS)).toContain(id)
  })
})
