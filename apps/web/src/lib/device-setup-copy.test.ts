// EXP-1169: one server card on every platform. Web's table
// (`device-setup-copy.ts`) is the source; this test reads the three native
// copy files off disk and asserts every string appears in them as a string
// literal, the same gate `onboarding-copy.test.ts` runs for the wizard. A
// missing native file fails LOUDLY: that is the drift the test exists to
// catch.
import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { DEVICE_SETUP_COPY } from "./device-setup-copy"
import { deviceErrorMessage } from "./auth/device-code"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)

const DESKTOP = `apps/desktop/crates/ui/src/device_setup.rs`
const IOS = `apps/ios/Exponential/UI/Components/DeviceSetupCopy.swift`
const ANDROID = `apps/android/app/src/main/java/com/exponential/app/ui/components/DeviceSetupCopy.kt`

const STRINGS = Object.values(DEVICE_SETUP_COPY)

function assertCarries(file: string) {
  const src = readFileSync(join(repoRoot, file), `utf8`)
  for (const value of STRINGS) {
    expect(
      src.includes(`"${value}"`) ? value : `${file} is missing: ${value}`
    ).toBe(value)
  }
}

describe(`device setup copy`, () => {
  it(`keeps the strings quotable in Swift, Kotlin and Rust source`, () => {
    for (const value of STRINGS) {
      expect(value).not.toContain(`"`)
      expect(value).not.toContain(`\\`)
    }
  })

  it(`maps the device-code errors onto the table`, () => {
    expect(deviceErrorMessage({ error: `expired_token` })).toBe(
      DEVICE_SETUP_COPY.codeExpired
    )
    expect(deviceErrorMessage({ error: `invalid_request` })).toBe(
      DEVICE_SETUP_COPY.codeInvalid
    )
    expect(deviceErrorMessage({ error: `invalid_grant` })).toBe(
      DEVICE_SETUP_COPY.codeInvalid
    )
    expect(deviceErrorMessage({ error: `access_denied` })).toBe(
      DEVICE_SETUP_COPY.codeOtherAccount
    )
    expect(deviceErrorMessage({ error: `server_error` })).toBe(
      DEVICE_SETUP_COPY.failed
    )
  })

  it(`the desktop IDE carries the strings verbatim`, () => {
    assertCarries(DESKTOP)
  })

  it(`iOS carries the strings verbatim`, () => {
    assertCarries(IOS)
  })

  it(`Android carries the strings verbatim`, () => {
    assertCarries(ANDROID)
  })
})
