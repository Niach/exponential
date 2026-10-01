// EXP-862: the account chip's menu rule, locked against the Android twin
// (`AgentAccountsRowsTest.the chip menu offers the repairs that state allows`)
// and the iOS one (`DeviceAccountChips.menuItems`). Three states, three menus,
// the same three strings on every client.
import { describe, expect, it } from "vitest"
import {
  ACTION_REMOVE,
  ACTION_SIGN_IN,
  ACTION_SIGN_OUT,
  accountChipActionable,
  accountChipActions,
  accountChipLabel,
  agentLoginCodeKey,
  agentLoginKey,
  agentLoginLanded,
  agentOfLoginCodeKey,
  type AccountChipRow,
} from "@/components/device-agent-account"
import type { SteerDevice } from "@/lib/steer-devices"

function chip(patch: Partial<AccountChipRow> = {}): AccountChipRow {
  return {
    agent: `claude`,
    profileId: `work`,
    profileLabel: `Work`,
    email: null,
    signedIn: true,
    active: false,
    health: `ok`,
    ...patch,
  }
}

// The EXP-862 build: remove, no sign-out body yet.
const ALL_CAPS = [`agent-login`, `account-switch`, `account-remove`]
// EXP-1137: a build that also signs logins out.
const SIGN_OUT_CAPS = [...ALL_CAPS, `account-sign-out`]

/** `deviceIsMine` reads the absence of an owner, so a teammate's device is
 *  one that names one. */
function device(patch: Partial<SteerDevice> = {}): SteerDevice {
  return {
    rowId: `row-1`,
    deviceId: `dev-1`,
    deviceLabel: `Studio`,
    kind: `desktop`,
    registered: true,
    online: true,
    lastSeenAt: new Date().toISOString(),
    caps: ALL_CAPS,
    ...patch,
  }
}

describe(`accountChipActions`, () => {
  it(`offers the repairs that state allows`, () => {
    // Signed out or expired: the sign-in leads, and (EXP-944) the removal
    // rides along — a dead named profile is exactly what people want gone.
    expect(
      accountChipActions(
        device(),
        chip({ signedIn: false, active: true, health: `signed_out` })
      )
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    expect(
      accountChipActions(device(), chip({ health: `needs_relogin` }))
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    // The ambient login stays a sign-in and nothing else on an EXP-862 build:
    // its config dir is the CLI's own, and that build cannot sign it out.
    expect(
      accountChipActions(
        device(),
        chip({ profileId: `system`, signedIn: false, health: `signed_out` })
      )
    ).toEqual([ACTION_SIGN_IN])
    // Healthy: only the removal, last used or not (EXP-1158: nothing picks
    // the login the machine starts on).
    expect(accountChipActions(device(), chip())).toEqual([ACTION_REMOVE])
    expect(accountChipActions(device(), chip({ active: true }))).toEqual([
      ACTION_REMOVE,
    ])
  })

  it(`gates each entry on its own cap, and never removes the ambient login without the sign-out body`, () => {
    expect(
      accountChipActions(
        device({ caps: [`agent-login`, `account-remove`] }),
        chip()
      )
    ).toEqual([ACTION_REMOVE])
    expect(
      accountChipActions(
        device({ caps: [`agent-login`, `account-switch`] }),
        chip()
      )
    ).toEqual([])
    // The ambient login is the agent CLI's own config dir: an EXP-862 build
    // cannot sign it out, so nothing is left.
    expect(
      accountChipActions(
        device(),
        chip({ profileId: `system`, profileLabel: `Default`, active: true })
      )
    ).toEqual([])
  })

  // EXP-1137: a build with the sign-out body offers "Sign out" on every
  // signed-in login, and "Remove account" on the ambient one too. The order
  // is fixed ×4: sign in, sign out, remove.
  it(`offers a sign-out and the ambient removal on a build that signs out`, () => {
    const machine = device({ caps: SIGN_OUT_CAPS })
    // A named login, healthy: every entry but the sign-in.
    expect(accountChipActions(machine, chip())).toEqual([
      ACTION_SIGN_OUT,
      ACTION_REMOVE,
    ])
    // The ambient login, healthy and active (the screenshot's first row): a
    // sign-out and a removal instead of no menu at all.
    expect(
      accountChipActions(machine, chip({ profileId: `system`, active: true }))
    ).toEqual([ACTION_SIGN_OUT, ACTION_REMOVE])
    // The ambient login, signed out (the screenshot's second row): the
    // sign-in and, new, the removal that hides it.
    expect(
      accountChipActions(
        machine,
        chip({
          profileId: `system`,
          active: true,
          signedIn: false,
          health: `signed_out`,
        })
      )
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    // A revoked credential still signs out: that is how it leaves.
    expect(
      accountChipActions(machine, chip({ health: `needs_relogin` }))
    ).toEqual([ACTION_SIGN_IN, ACTION_SIGN_OUT, ACTION_REMOVE])
    // A signed-out named login has nothing to sign out of.
    expect(
      accountChipActions(
        machine,
        chip({ signedIn: false, health: `signed_out` })
      )
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    // The sign-out cap alone (no `account-remove`) still removes only the
    // ambient login.
    expect(
      accountChipActions(
        device({ caps: [`agent-login`, `account-sign-out`] }),
        chip()
      )
    ).toEqual([ACTION_SIGN_OUT])
  })
})

describe(`accountChipActionable`, () => {
  it(`is a control only on my own online device that takes the commands`, () => {
    expect(accountChipActionable(device(), chip())).toBe(true)
    expect(
      accountChipActionable(
        device({ owner: { id: `u2`, name: `Lukas` } }),
        chip()
      )
    ).toBe(false)
    expect(accountChipActionable(device({ online: false }), chip())).toBe(false)
    expect(
      accountChipActionable(device({ caps: [`account-remove`] }), chip())
    ).toBe(false)
    // Nothing to offer = a statement, not a control.
    expect(
      accountChipActionable(
        device(),
        chip({ profileId: `system`, active: true })
      )
    ).toBe(false)
    // EXP-1137: the same row IS a control on a build that signs out.
    expect(
      accountChipActionable(
        device({ caps: SIGN_OUT_CAPS }),
        chip({ profileId: `system`, active: true })
      )
    ).toBe(true)
  })
})

describe(`accountChipLabel`, () => {
  it(`names the login by its address, never by its profile label`, () => {
    expect(accountChipLabel(chip({ email: `dev@acme.test` }))).toBe(
      `dev@acme.test`
    )
    expect(accountChipLabel(chip())).toBe(`No email`)
  })
})

describe(`agentLoginLanded`, () => {
  it(`reads the TARGETED login, not the account's own flag`, () => {
    // Nothing reported for the agent at all.
    expect(agentLoginLanded(null, {})).toBe(false)
    // A re-login: the CLI still claims a login the probe found revoked, so
    // `signedIn` never moves and only the health says it landed.
    const revoked = {
      signedIn: true,
      profiles: [
        { id: `work`, label: `Work`, signedIn: true, health: `needs_relogin` as const },
      ],
    }
    expect(agentLoginLanded(revoked, { profileId: `work` })).toBe(false)
    expect(
      agentLoginLanded(
        {
          signedIn: true,
          profiles: [
            { id: `work`, label: `Work`, signedIn: true, health: `ok` as const },
          ],
        },
        { profileId: `work` }
      )
    ).toBe(true)
    // Add account: the device has not created the profile yet, and the
    // account it already holds is signed in.
    const held = {
      signedIn: true,
      profiles: [
        { id: `system`, label: `Default`, signedIn: true, health: `ok` as const },
      ],
    }
    expect(agentLoginLanded(held, { newProfileLabel: `Claude account 2` })).toBe(
      false
    )
    expect(
      agentLoginLanded(
        {
          signedIn: true,
          profiles: [
            ...held.profiles,
            { id: `0a1b`, label: `Claude account 2`, signedIn: true },
          ],
        },
        { newProfileLabel: `Claude account 2` }
      )
    ).toBe(true)
  })

  it(`answers for the ambient login off its own row`, () => {
    expect(agentLoginLanded({ signedIn: false }, { profileId: `system` })).toBe(
      false
    )
    expect(agentLoginLanded({ signedIn: true }, { profileId: `system` })).toBe(
      true
    )
    // A device that reports profiles keeps its ACTIVE profile in the top-level
    // fields, so the ambient login is read off the `system` row.
    expect(
      agentLoginLanded(
        {
          signedIn: true,
          profiles: [
            { id: `system`, label: `Default`, signedIn: false },
            { id: `0a1b`, label: `Work`, signedIn: true, active: true },
          ],
        },
        { profileId: `system` }
      )
    ).toBe(false)
  })
})

describe(`login command keys`, () => {
  it(`round-trips the code key and ignores every other key`, () => {
    expect(agentLoginKey(`claude`)).toBe(`login:claude`)
    expect(agentOfLoginCodeKey(agentLoginCodeKey(`claude`))).toBe(`claude`)
    expect(agentOfLoginCodeKey(agentLoginKey(`claude`))).toBeNull()
    expect(agentOfLoginCodeKey(`prune`)).toBeNull()
  })
})
