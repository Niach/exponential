import { describe, expect, it } from "vitest"
import {
  canRemoveAccountOn,
  canSignOutAccountOn,
  removeAccountBlockReason,
  removeAccountConfirmCopy,
  REMOVE_ACCOUNT_OLD_APP,
  signOutBlockReason,
  signOutConfirmCopy,
  SIGN_OUT_OLD_APP,
} from "./agent-account-remove"

const CAPABLE = { caps: [`agent-login`, `account-remove`] }
// EXP-1137: a build that also signs logins out.
const SIGNS_OUT = {
  caps: [`agent-login`, `account-remove`, `account-sign-out`],
}
const HEALTHY = { profileId: `a1b2c3d4`, signedIn: true, health: `ok` as const }

describe(`removeAccountBlockReason (EXP-862)`, () => {
  it(`offers the removal on a capable machine`, () => {
    expect(removeAccountBlockReason(CAPABLE)).toBeNull()
    expect(canRemoveAccountOn(CAPABLE)).toBe(true)
    // The sign-out cap alone does not remove.
    expect(
      canRemoveAccountOn({ caps: [`agent-login`, `account-sign-out`] })
    ).toBe(false)
  })

  it(`refuses a build that cannot run the command, with the server's sentence`, () => {
    // Both caps are needed: `agent-login` to drive the machine's logins at
    // all, `account-remove` for this command itself.
    for (const caps of [[], [`agent-login`], [`account-remove`]]) {
      expect(removeAccountBlockReason({ caps })).toBe(REMOVE_ACCOUNT_OLD_APP)
    }
    expect(removeAccountBlockReason({ caps: undefined })).toBe(
      REMOVE_ACCOUNT_OLD_APP
    )
  })
})

describe(`signOutBlockReason (EXP-1137)`, () => {
  it(`offers the sign-out for any signed-in login on a build that has it`, () => {
    expect(signOutBlockReason(SIGNS_OUT, HEALTHY)).toBeNull()
    // A revoked credential is still a credential on the machine: signing
    // out is how it leaves.
    expect(
      canSignOutAccountOn(SIGNS_OUT, { ...HEALTHY, health: `needs_relogin` })
    ).toBe(true)
  })

  it(`has nothing to sign out of on a signed-out login`, () => {
    expect(signOutBlockReason(SIGNS_OUT, { ...HEALTHY, signedIn: false })).toBe(
      `That login is already signed out there.`
    )
  })

  it(`refuses a build without both caps, with the server's sentence`, () => {
    for (const caps of [
      [],
      [`agent-login`],
      [`account-sign-out`],
      [`agent-login`, `account-remove`],
    ]) {
      expect(signOutBlockReason({ caps }, HEALTHY)).toBe(SIGN_OUT_OLD_APP)
    }
    expect(canSignOutAccountOn(CAPABLE, HEALTHY)).toBe(false)
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
      REMOVE_ACCOUNT_OLD_APP,
      SIGN_OUT_OLD_APP,
    ]) {
      expect(copy.includes(`—`)).toBe(false)
    }
  })
})
