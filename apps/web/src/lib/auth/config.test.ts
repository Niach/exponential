import { afterEach, describe, expect, it, vi } from "vitest"
import {
  buildAuthConfig,
  isAuthRateLimitEnabled,
  isPasswordSignupDisabled,
} from "@/lib/auth/config"

afterEach(() => {
  vi.unstubAllEnvs()
})

describe(`buildAuthConfig`, () => {
  it(`advertises the device-code flow so the CLI can feature-detect it`, () => {
    // Older self-hosted instances lack the field entirely; the CLI treats
    // absent-or-false as "fall back to password login". Current builds always
    // advertise it (EXP-403).
    expect(buildAuthConfig().deviceFlowEnabled).toBe(true)
  })
})

// Both toggles derive their default from the BUILD (import.meta.env.PROD),
// not runtime NODE_ENV (REV-5) — under vitest that is the dev default, so
// only the dev default and the explicit overrides are assertable here.
describe(`isPasswordSignupDisabled`, () => {
  it(`defaults to open sign-up in dev builds`, () => {
    vi.stubEnv(`AUTH_SIGNUP_ENABLED`, ``)
    expect(isPasswordSignupDisabled()).toBe(false)
  })

  it(`follows the explicit override in either direction`, () => {
    vi.stubEnv(`AUTH_SIGNUP_ENABLED`, `false`)
    expect(isPasswordSignupDisabled()).toBe(true)
    vi.stubEnv(`AUTH_SIGNUP_ENABLED`, `true`)
    expect(isPasswordSignupDisabled()).toBe(false)
  })
})

describe(`isAuthRateLimitEnabled`, () => {
  it(`defaults to off in dev builds`, () => {
    vi.stubEnv(`AUTH_RATE_LIMIT_ENABLED`, ``)
    expect(isAuthRateLimitEnabled()).toBe(false)
  })

  it(`follows the explicit override in either direction`, () => {
    vi.stubEnv(`AUTH_RATE_LIMIT_ENABLED`, `true`)
    expect(isAuthRateLimitEnabled()).toBe(true)
    vi.stubEnv(`AUTH_RATE_LIMIT_ENABLED`, `false`)
    expect(isAuthRateLimitEnabled()).toBe(false)
  })
})
