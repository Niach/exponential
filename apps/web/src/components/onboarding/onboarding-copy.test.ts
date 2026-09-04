// EXP-725: one first-run wizard on every platform. Web's tables
// (`onboarding-copy.ts`) are the source; this test reads the three native
// copy files off disk and asserts every shared string appears in them as a
// string literal — the same gate `getting-started-copy.test.ts` runs for the
// checklist. A missing native file fails LOUDLY: that is the drift the test
// exists to catch.
import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  MOBILE_ONBOARDING_COPY,
  ONBOARDING_COPY,
  onboardingCopyStrings,
} from "@/components/onboarding/onboarding-copy"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`, `..`)

const DESKTOP = `apps/desktop/crates/ui/src/onboarding.rs`
const IOS = `apps/ios/Exponential/UI/Onboarding/OnboardingCopy.swift`
const ANDROID = `apps/android/app/src/main/java/com/exponential/app/ui/onboarding/OnboardingCopy.kt`

const SHARED = onboardingCopyStrings(ONBOARDING_COPY)
const MOBILE = onboardingCopyStrings(MOBILE_ONBOARDING_COPY)

function read(relative: string): string {
  return readFileSync(join(repoRoot, relative), `utf8`)
}

function assertCarries(file: string, strings: string[]) {
  const src = read(file)
  for (const value of strings) {
    expect(
      src.includes(`"${value}"`) ? value : `${file} is missing: ${value}`
    ).toBe(value)
  }
}

describe(`onboarding copy`, () => {
  it(`keeps the strings quotable in Swift, Kotlin and Rust source`, () => {
    for (const value of [...SHARED, ...MOBILE]) {
      expect(value).not.toContain(`"`)
      expect(value).not.toContain(`'`)
      expect(value).not.toContain(`\\`)
      // eslint-disable-next-line no-control-regex
      expect(/^[\x20-\x7e]+$/.test(value) ? value : `non-ASCII: ${value}`).toBe(
        value
      )
    }
  })

  it(`keeps the two capture anchors out of every other string`, () => {
    // The shots pipeline waits on the step headers; Playwright matches
    // substrings, so no subtitle may contain them.
    const anchors = [ONBOARDING_COPY.invite.title, ONBOARDING_COPY.devices.title]
    for (const anchor of anchors) {
      for (const value of [...SHARED, ...MOBILE]) {
        if (value === anchor) continue
        expect(value.includes(anchor) ? `${value} contains ${anchor}` : value).toBe(
          value
        )
      }
    }
  })

  it(`the desktop IDE carries the shared strings verbatim`, () => {
    assertCarries(DESKTOP, SHARED)
  })

  it(`iOS carries the shared and mobile strings verbatim`, () => {
    assertCarries(IOS, [...SHARED, ...MOBILE])
  })

  it(`Android carries the shared and mobile strings verbatim`, () => {
    assertCarries(ANDROID, [...SHARED, ...MOBILE])
  })
})
