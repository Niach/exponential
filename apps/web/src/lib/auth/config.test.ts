import { describe, expect, it } from "vitest"
import { buildAuthConfig } from "@/lib/auth/config"

describe(`buildAuthConfig`, () => {
  it(`advertises the device-code flow so the CLI can feature-detect it`, () => {
    // Older self-hosted instances lack the field entirely; the CLI treats
    // absent-or-false as "fall back to password login". Current builds always
    // advertise it (EXP-403).
    expect(buildAuthConfig().deviceFlowEnabled).toBe(true)
  })
})
