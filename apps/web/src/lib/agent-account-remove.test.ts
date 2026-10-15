import { describe, expect, it } from "vitest"
import {
  canSignOutAccountOn,
  removeAccountConfirmCopy,
  signOutConfirmCopy,
} from "./agent-account-remove"

const HEALTHY = { profileId: `a1b2c3d4`, signedIn: true, health: `ok` as const }

describe(`canSignOutAccountOn (EXP-1137)`, () => {
  it(`offers the sign-out for any signed-in login`, () => {
    expect(canSignOutAccountOn(HEALTHY)).toBe(true)
    // A revoked credential is still a credential on the machine: signing
    // out is how it leaves.
    expect(
      canSignOutAccountOn({ ...HEALTHY, health: `needs_relogin` })
    ).toBe(true)
  })

  it(`has nothing to sign out of on a signed-out login`, () => {
    expect(canSignOutAccountOn({ ...HEALTHY, signedIn: false })).toBe(false)
  })
})

describe(`confirm copy (EXP-862, EXP-1137)`, () => {
  it(`names the login and the machine and promises the account survives`, () => {
    expect(removeAccountConfirmCopy(`work@example.com`, `Mac mini`)).toBe(
      `Delete work@example.com on Mac mini? The login is removed from this device only; the account itself is untouched.`
    )
    expect(signOutConfirmCopy(`me@example.com`, `mint`)).toBe(
      `Sign me@example.com out on mint? The login stays listed so it can sign in again; the account itself is untouched.`
    )
  })

  it(`writes no em dashes (multi-client copy rule)`, () => {
    for (const copy of [
      removeAccountConfirmCopy(`Work`, `Laptop`),
      signOutConfirmCopy(`Work`, `Laptop`),
    ]) {
      expect(copy.includes(`—`)).toBe(false)
    }
  })
})
