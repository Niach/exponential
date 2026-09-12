import { describe, expect, it } from "vitest"
import {
  defaultCandidateId,
  resolveLaunchDeviceId,
} from "@/lib/launch-device"

// EXP-836: the launch surfaces' device precedence, as a pure rule.
describe(`resolveLaunchDeviceId`, () => {
  const mac = { deviceId: `mac`, isDefault: true }
  const mint = { deviceId: `mint` }
  const pi = { deviceId: `pi` }

  it(`lets an explicit request win over the default machine and a pick`, () => {
    expect(
      resolveLaunchDeviceId([mac, mint], { requested: `mint` })
    ).toBe(`mint`)
    expect(
      resolveLaunchDeviceId([mac, mint], { requested: `mint`, picked: `mac` })
    ).toBe(`mint`)
  })

  it(`falls back while the requested machine is not a candidate`, () => {
    // The devices shape has not hydrated it yet (or it went offline): the
    // fallback renders, and the caller keeps the request so the machine wins
    // the moment it appears.
    expect(resolveLaunchDeviceId([mac, pi], { requested: `mint` })).toBe(`mac`)
    expect(resolveLaunchDeviceId([], { requested: `mint` })).toBeNull()
  })

  it(`prefers a pick over the default, and the default over the first row`, () => {
    expect(resolveLaunchDeviceId([pi, mac], { picked: `pi` })).toBe(`pi`)
    expect(resolveLaunchDeviceId([pi, mac])).toBe(`mac`)
    expect(resolveLaunchDeviceId([pi, mint])).toBe(`pi`)
  })

  it(`drops a stale pick instead of selecting nothing`, () => {
    expect(resolveLaunchDeviceId([pi, mac], { picked: `gone` })).toBe(`mac`)
  })

  it(`reports the flagged default, or null`, () => {
    expect(defaultCandidateId([pi, mac])).toBe(`mac`)
    expect(defaultCandidateId([pi, mint])).toBeNull()
    expect(defaultCandidateId([])).toBeNull()
  })
})
