import { describe, expect, it } from "vitest"
import {
  canRemoveAccountOn,
  removeAccountBlockReason,
  removeAccountConfirmCopy,
  REMOVE_ACCOUNT_OLD_APP,
} from "./agent-account-remove"

const CAPABLE = { caps: [`agent-login`, `account-remove`] }
const HEALTHY = { profileId: `a1b2c3d4`, signedIn: true, health: `ok` as const }

describe(`removeAccountBlockReason (EXP-862)`, () => {
  it(`offers the removal for a healthy named login on a capable machine`, () => {
    expect(removeAccountBlockReason(CAPABLE, HEALTHY)).toBeNull()
    expect(canRemoveAccountOn(CAPABLE, HEALTHY)).toBe(true)
  })

  it(`never offers it for the machine's own ambient login`, () => {
    const reason = removeAccountBlockReason(CAPABLE, {
      ...HEALTHY,
      profileId: `system`,
    })
    expect(reason).toContain(`the machine's own agent login`)
    expect(canRemoveAccountOn(CAPABLE, { ...HEALTHY, profileId: `` })).toBe(
      false
    )
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
  })

  it(`leaves a signed-out or expired login to its sign-in`, () => {
    expect(
      removeAccountBlockReason(CAPABLE, { ...HEALTHY, signedIn: false })
    ).toContain(`signed out`)
    expect(
      canRemoveAccountOn(CAPABLE, { ...HEALTHY, health: `needs_relogin` })
    ).toBe(false)
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
