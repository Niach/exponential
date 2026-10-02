import { describe, expect, it } from "vitest"
import { joinDeviceStep } from "./join-device-step"

describe(`joinDeviceStep (EXP-1169)`, () => {
  it(`shows the devices step only to a joiner who owns no device`, () => {
    expect(joinDeviceStep([], false)).toBe(`step`)
    expect(joinDeviceStep([{ deviceId: `a` }], false)).toBe(`enter`)
  })

  it(`waits for the devices shape, but never past the timeout`, () => {
    expect(joinDeviceStep(null, false)).toBe(`wait`)
    expect(joinDeviceStep(null, true)).toBe(`enter`)
    // A list that landed decides, timed out or not.
    expect(joinDeviceStep([], true)).toBe(`step`)
  })
})
