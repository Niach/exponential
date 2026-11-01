// EXP-1600: Keep signed in is the default claude sign-in — the mode rule
// and the hover card pill a login item's action reads.
import { describe, expect, it } from "vitest"
import { loginIsLongLived } from "@/lib/agent-usage"
import { loginActionLabel } from "@/components/device-logins"

describe(`loginIsLongLived`, () => {
  it(`defaults claude to the long-lived login; false = the browser login`, () => {
    expect(loginIsLongLived(`claude`)).toBe(true)
    expect(loginIsLongLived(`claude`, true)).toBe(true)
    expect(loginIsLongLived(`claude`, false)).toBe(false)
  })

  it(`never applies to codex`, () => {
    expect(loginIsLongLived(`codex`)).toBe(false)
    expect(loginIsLongLived(`codex`, true)).toBe(false)
  })
})

describe(`loginActionLabel`, () => {
  const item = (action: `sign_in` | `keep_signed_in`, status: `signed_out` | `renew` | `switch`) =>
    ({ kind: `login`, agent: `claude`, profileId: `a`, text: ``, action, status }) as const

  it(`reads the fixture copy per action and status`, () => {
    expect(loginActionLabel(item(`sign_in`, `signed_out`))).toBe(`Sign in`)
    expect(loginActionLabel(item(`keep_signed_in`, `switch`))).toBe(`Switch to Keep signed in`)
    expect(loginActionLabel(item(`keep_signed_in`, `renew`))).toBe(`Renew Keep signed in`)
  })
})
