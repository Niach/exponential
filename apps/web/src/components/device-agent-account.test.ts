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
  agentLoginBaseline,
  agentLoginKey,
  agentLoginLanding,
  agentOfLoginCodeKey,
  alreadyAddedCopy,
  type AccountChipRow,
} from "@/components/device-agent-account"
import type { SteerDevice } from "@/lib/steer-devices"

function chip(patch: Partial<AccountChipRow> = {}): AccountChipRow {
  return {
    agent: `claude`,
    profileId: `work`,
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
    // rides along — a dead profile is exactly what people want gone.
    expect(
      accountChipActions(
        device(),
        chip({ signedIn: false, active: true, health: `signed_out` })
      )
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    expect(
      accountChipActions(device(), chip({ health: `needs_relogin` }))
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    // Healthy: only the removal, last used or not (EXP-1158: nothing picks
    // the login the machine starts on).
    expect(accountChipActions(device(), chip())).toEqual([ACTION_REMOVE])
    expect(accountChipActions(device(), chip({ active: true }))).toEqual([
      ACTION_REMOVE,
    ])
  })

  it(`gates each entry on its own cap`, () => {
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
  })

  // EXP-1137: a build with the sign-out body offers "Sign out" on every
  // signed-in login. The order is fixed ×4: sign in, sign out, remove.
  it(`offers a sign-out on a build that signs out`, () => {
    const machine = device({ caps: SIGN_OUT_CAPS })
    expect(accountChipActions(machine, chip())).toEqual([
      ACTION_SIGN_OUT,
      ACTION_REMOVE,
    ])
    // A revoked credential still signs out: that is how it leaves.
    expect(
      accountChipActions(machine, chip({ health: `needs_relogin` }))
    ).toEqual([ACTION_SIGN_IN, ACTION_SIGN_OUT, ACTION_REMOVE])
    // A signed-out login has nothing to sign out of.
    expect(
      accountChipActions(
        machine,
        chip({ signedIn: false, health: `signed_out` })
      )
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    // The sign-out cap alone (no `account-remove`) removes nothing.
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
      accountChipActionable(device({ caps: [`agent-login`] }), chip())
    ).toBe(false)
  })
})

describe(`accountChipLabel`, () => {
  it(`names the login by its address`, () => {
    expect(accountChipLabel(chip({ email: `dev@acme.test` }))).toBe(
      `dev@acme.test`
    )
    expect(accountChipLabel(chip())).toBe(`No email`)
  })
})

// The landing rule ×4: accounts A, B, C on the machine, C expired.
describe(`agentLoginLanding`, () => {
  const T0 = `2026-10-01T10:00:00.000Z`
  const T1 = `2026-10-10T19:00:00.000Z`
  const before = {
    profiles: [
      { id: `a`, signedIn: true, email: `a@acme.test`, lastLoginAt: T0 },
      { id: `b`, signedIn: true, email: `b@acme.test` },
      {
        id: `c`,
        signedIn: true,
        email: `c@acme.test`,
        health: `needs_relogin` as const,
        lastLoginAt: T0,
      },
    ],
  }
  const baseline = agentLoginBaseline(before)
  const after = (id: string) => ({
    profiles: before.profiles.map((profile) =>
      profile.id === id ? { ...profile, lastLoginAt: T1 } : profile
    ),
  })

  it(`captures every profile's lastLoginAt`, () => {
    expect(baseline).toEqual({ a: T0, b: null, c: T0 })
    expect(agentLoginBaseline(null)).toEqual({})
  })

  it(`has not landed while nothing moved, whatever signedIn says`, () => {
    expect(agentLoginLanding(before, baseline, `c`)).toBeNull()
    expect(agentLoginLanding(null, baseline)).toBeNull()
  })

  it(`lands on the intended profile without a warning`, () => {
    expect(agentLoginLanding(after(`c`), baseline, `c`)).toEqual({
      profileId: `c`,
      email: `c@acme.test`,
      duplicate: false,
    })
  })

  it(`Sign in on C as A: A is refreshed and it is a duplicate`, () => {
    const landing = agentLoginLanding(after(`a`), baseline, `c`)
    expect(landing).toEqual({ profileId: `a`, email: `a@acme.test`, duplicate: true })
    expect(alreadyAddedCopy(landing!.email)).toBe(
      `a@acme.test was already added. Refreshed it.`
    )
    // A profile with no stamp before counts once it gets one.
    expect(agentLoginLanding(after(`b`), baseline, `c`)?.profileId).toBe(`b`)
  })

  it(`Add account: an existing email is a duplicate, a new profile is not`, () => {
    expect(agentLoginLanding(after(`a`), baseline)?.duplicate).toBe(true)
    expect(
      agentLoginLanding(
        {
          profiles: [
            ...before.profiles,
            { id: `d`, signedIn: true, email: `d@acme.test`, lastLoginAt: T1 },
          ],
        },
        baseline
      )
    ).toEqual({ profileId: `d`, email: `d@acme.test`, duplicate: false })
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
