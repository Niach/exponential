// EXP-862: the account chip's menu rule, locked against the Android twin
// (`AgentAccountsRowsTest.the chip menu offers the repairs that state allows`)
// and the iOS one (`DeviceAccountChips.menuItems`). Three states, three menus,
// the same three strings on every client.
import { describe, expect, it } from "vitest"
import {
  ACTION_KEEP_SIGNED_IN,
  ACTION_REMOVE,
  ACTION_RENEW_LONG_LIVED,
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
  keepSignedInTarget,
  loginName,
  reportedLoginName,
  switchedName,
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
    ...patch,
  }
}

describe(`accountChipActions`, () => {
  // The order is fixed ×4: sign in, keep signed in (claude), sign out,
  // remove.
  it(`offers the repairs that state allows`, () => {
    const codex = (patch: Partial<AccountChipRow> = {}) =>
      chip({ agent: `codex`, ...patch })
    // Signed out or expired: the sign-in leads, and (EXP-944) the removal
    // rides along — a dead profile is exactly what people want gone. A
    // signed-out login has nothing to sign out of.
    expect(
      accountChipActions(
        codex({ signedIn: false, active: true, health: `signed_out` })
      )
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    // A revoked credential still signs out (EXP-1137): that is how it leaves.
    expect(
      accountChipActions(codex({ health: `needs_relogin` }))
    ).toEqual([ACTION_SIGN_IN, ACTION_SIGN_OUT, ACTION_REMOVE])
    // Healthy: sign out + remove, last used or not (EXP-1158: nothing picks
    // the login the machine starts on).
    expect(accountChipActions(codex())).toEqual([
      ACTION_SIGN_OUT,
      ACTION_REMOVE,
    ])
    expect(accountChipActions(codex({ active: true }))).toEqual([
      ACTION_SIGN_OUT,
      ACTION_REMOVE,
    ])
  })

  // EXP-1600: a claude "Sign in" IS Keep signed in, so a row that signs in
  // has no second entry; a signed-in row offers the switch, or the renewal
  // while it holds a token.
  it(`offers claude's switch or renewal only when it does not sign in`, () => {
    expect(
      accountChipActions(chip({ signedIn: false, health: `signed_out` }))
    ).toEqual([ACTION_SIGN_IN, ACTION_REMOVE])
    expect(accountChipActions(chip({ health: `needs_relogin` }))).toEqual([
      ACTION_SIGN_IN,
      ACTION_SIGN_OUT,
      ACTION_REMOVE,
    ])
    expect(accountChipActions(chip())).toEqual([
      ACTION_KEEP_SIGNED_IN,
      ACTION_SIGN_OUT,
      ACTION_REMOVE,
    ])
    expect(
      accountChipActions(chip({ tokenExpiresAt: `2027-11-01T00:00:00.000Z` }))
    ).toEqual([ACTION_RENEW_LONG_LIVED, ACTION_SIGN_OUT, ACTION_REMOVE])
    // Never on codex, token or not.
    expect(
      accountChipActions(
        chip({ agent: `codex`, tokenExpiresAt: `2027-11-01T00:00:00.000Z` })
      )
    ).toEqual([ACTION_SIGN_OUT, ACTION_REMOVE])
  })

  it(`labels the long-lived entries byte-identically ×4`, () => {
    expect(ACTION_KEEP_SIGNED_IN).toBe(`Switch to Keep signed in`)
    expect(ACTION_RENEW_LONG_LIVED).toBe(`Renew Keep signed in`)
  })
})

describe(`accountChipActionable`, () => {
  it(`is a control only on my own online device`, () => {
    expect(accountChipActionable(device())).toBe(true)
    expect(
      accountChipActionable(device({ owner: { id: `u2`, name: `Lukas` } }))
    ).toBe(false)
    expect(accountChipActionable(device({ online: false }))).toBe(false)
    // No build cap gates it any more (every daemon at the floor runs them).
    expect(accountChipActionable(device({ caps: [] }))).toBe(true)
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

// EXP-1626: a login's label = its name, else its email, else the fallback.
describe(`loginName`, () => {
  it(`prefers the name a long-lived Add gave the profile`, () => {
    expect(loginName({ name: `Work`, email: `dev@acme.test`, plan: `max` })).toBe(`Work`)
    expect(loginName({ name: `  `, email: `dev@acme.test` })).toBe(`dev@acme.test`)
    expect(loginName({ name: null, email: null, plan: `pro` })).toBe(`pro`)
    expect(loginName({})).toBe(`No email`)
    expect(accountChipLabel(chip({ name: `Work`, email: `dev@acme.test` }))).toBe(`Work`)
  })

  it(`reads the name the machine reports for one profile`, () => {
    const accounts = {
      claude: {
        profiles: [
          { id: `a`, name: ` Work ` },
          { id: `b` },
        ],
      },
    } as never
    expect(reportedLoginName(accounts, `claude`, `a`)).toBe(`Work`)
    expect(reportedLoginName(accounts, `claude`, `b`)).toBeNull()
    expect(reportedLoginName(accounts, `codex`, `a`)).toBeNull()
    expect(reportedLoginName(undefined, `claude`, `a`)).toBeNull()
  })
})

// EXP-1626: Switch ADDS a named long-lived profile; Renew stays in place.
describe(`keepSignedInTarget`, () => {
  it(`adds a new named profile for a switch`, () => {
    expect(switchedName(`dev@acme.test`)).toBe(`dev@acme.test (kept signed in)`)
    expect(keepSignedInTarget(chip({ profileId: `work` }), `dev@acme.test`)).toEqual({
      longLived: true,
      name: `dev@acme.test (kept signed in)`,
    })
  })

  it(`renews the token profile in place`, () => {
    expect(
      keepSignedInTarget(
        chip({ profileId: `work`, tokenExpiresAt: `2027-10-01T00:00:00.000Z` }),
        `Work`
      )
    ).toEqual({ profileId: `work`, longLived: true })
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
