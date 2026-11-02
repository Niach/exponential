// EXP-1626: Persist login — the mode rule (the browser login unless asked),
// the hover card pill a login item's action reads, and the persisted line.
import { describe, expect, it } from "vitest"
import { loginIsLongLived, persistedLine } from "@/lib/agent-usage"
import { loginActionLabel } from "@/components/device-logins"

describe(`loginIsLongLived`, () => {
  it(`is the browser login unless Persist login asks for the token`, () => {
    expect(loginIsLongLived(`claude`)).toBe(false)
    expect(loginIsLongLived(`claude`, true)).toBe(true)
    expect(loginIsLongLived(`claude`, false)).toBe(false)
  })

  it(`never applies to codex`, () => {
    expect(loginIsLongLived(`codex`)).toBe(false)
    expect(loginIsLongLived(`codex`, true)).toBe(false)
  })
})

describe(`loginActionLabel`, () => {
  const item = (action: `sign_in` | `keep_signed_in`, status: `signed_out` | `renew`) =>
    ({ kind: `login`, agent: `claude`, profileId: `a`, text: ``, action, status }) as const

  it(`reads the fixture copy per action`, () => {
    expect(loginActionLabel(item(`sign_in`, `signed_out`))).toBe(`Sign in`)
    expect(loginActionLabel(item(`keep_signed_in`, `renew`))).toBe(`Renew persisted login`)
  })
})

describe(`persistedLine`, () => {
  it(`says which login feeds a persisted profile's usage`, () => {
    const at = `2027-11-01T00:00:00.000Z`
    expect(persistedLine({ tokenExpiresAt: at, usageSource: null })).toBe(
      `Persisted · usage from the browser login`
    )
    expect(persistedLine({ tokenExpiresAt: at, usageSource: `token` })).toBe(
      `Persisted · browser login expired, weekly usage only`
    )
  })

  it(`is absent without a token`, () => {
    expect(persistedLine({ tokenExpiresAt: null, usageSource: `token` })).toBeNull()
  })
})
