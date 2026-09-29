import { describe, expect, it } from "vitest"
import {
  canRemoveAccountOn,
  canSignOutAccountOn,
  removeAccountBlockReason,
  removeAccountConfirmCopy,
  removeAmbientAccountConfirmCopy,
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
const AMBIENT = { ...HEALTHY, profileId: `system` }

describe(`removeAccountBlockReason (EXP-862)`, () => {
  it(`offers the removal for a healthy named login on a capable machine`, () => {
    expect(removeAccountBlockReason(CAPABLE, HEALTHY)).toBeNull()
    expect(canRemoveAccountOn(CAPABLE, HEALTHY)).toBe(true)
  })

  it(`offers the machine's own ambient login only on a build that signs out (EXP-1137)`, () => {
    // Removing the ambient login means signing it out and hiding it, which
    // is the sign-out body — `account-remove` alone cannot do it.
    expect(removeAccountBlockReason(CAPABLE, AMBIENT)).toBe(SIGN_OUT_OLD_APP)
    expect(canRemoveAccountOn(CAPABLE, { ...HEALTHY, profileId: `` })).toBe(
      false
    )
    expect(removeAccountBlockReason(SIGNS_OUT, AMBIENT)).toBeNull()
    expect(canRemoveAccountOn(SIGNS_OUT, { ...AMBIENT, signedIn: false })).toBe(
      true
    )
    // A named profile does not need the sign-out cap.
    expect(
      canRemoveAccountOn({ caps: [`agent-login`, `account-sign-out`] }, HEALTHY)
    ).toBe(false)
  })

  it(`refuses a build that cannot run the command, with the server's sentence`, () => {
    // Both caps are needed: `agent-login` to drive the machine's logins at
    // all, `account-remove` for this command itself.
    for (const caps of [[], [`agent-login`], [`account-remove`]]) {
      expect(removeAccountBlockReason({ caps }, HEALTHY)).toBe(
        REMOVE_ACCOUNT_OLD_APP
      )
    }
    expect(removeAccountBlockReason({ caps: undefined }, HEALTHY)).toBe(
      REMOVE_ACCOUNT_OLD_APP
    )
    // The ambient login's refusal names the sign-out sentence instead.
    expect(removeAccountBlockReason({ caps: [] }, AMBIENT)).toBe(
      SIGN_OUT_OLD_APP
    )
  })

  it(`still offers it for a signed-out or expired named login (EXP-944)`, () => {
    // The removal deletes a profile dir; the credential's state never decided
    // whether that is possible, and the server has always taken it.
    expect(
      removeAccountBlockReason(CAPABLE, { ...HEALTHY, signedIn: false })
    ).toBeNull()
    expect(
      canRemoveAccountOn(CAPABLE, { ...HEALTHY, health: `needs_relogin` })
    ).toBe(true)
  })
})

describe(`signOutBlockReason (EXP-1137)`, () => {
  it(`offers the sign-out for any signed-in login on a build that has it`, () => {
    expect(signOutBlockReason(SIGNS_OUT, HEALTHY)).toBeNull()
    expect(canSignOutAccountOn(SIGNS_OUT, AMBIENT)).toBe(true)
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
    expect(canSignOutAccountOn(SIGNS_OUT, { ...AMBIENT, signedIn: false })).toBe(
      false
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

describe(`removeAccountConfirmCopy (EXP-862)`, () => {
  it(`names the login and the machine and promises the account survives`, () => {
    expect(removeAccountConfirmCopy(`work@example.com`, `Mac mini`)).toBe(
      `Delete work@example.com on Mac mini? The login is removed from this device only; the account itself is untouched.`
    )
  })

  it(`writes no em dashes (multi-client copy rule)`, () => {
    const copy = removeAccountConfirmCopy(`Work`, `Laptop`)
    expect(copy.includes(`—`)).toBe(false)
    expect(REMOVE_ACCOUNT_OLD_APP.includes(`—`)).toBe(false)
  })
})

describe(`sign-out and ambient copy (EXP-1137)`, () => {
  it(`pins the ambient removal sentence, naming the terminal CLI`, () => {
    expect(
      removeAmbientAccountConfirmCopy(`me@example.com`, `mint`, `Codex`)
    ).toBe(
      `Remove me@example.com from mint? The machine's own Codex login is signed out there, including for the Codex CLI in the terminal, and hidden here until it signs in again; the account itself is untouched.`
    )
  })

  it(`pins both sign-out sentences`, () => {
    expect(signOutConfirmCopy(`me@example.com`, `mint`)).toBe(
      `Sign me@example.com out on mint? The login stays listed so it can sign in again; the account itself is untouched.`
    )
    expect(signOutConfirmCopy(`me@example.com`, `mint`, `Claude`)).toBe(
      `Sign me@example.com out on mint? That is the machine's own Claude login, so the Claude CLI there is signed out too; the account itself is untouched.`
    )
  })

  it(`writes no em dashes (multi-client copy rule)`, () => {
    for (const copy of [
      removeAmbientAccountConfirmCopy(`Work`, `Laptop`, `Codex`),
      signOutConfirmCopy(`Work`, `Laptop`),
      signOutConfirmCopy(`Work`, `Laptop`, `Codex`),
      SIGN_OUT_OLD_APP,
    ]) {
      expect(copy.includes(`—`)).toBe(false)
    }
  })
})
