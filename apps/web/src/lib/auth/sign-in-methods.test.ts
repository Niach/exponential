import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  buildSignInMethods,
  configuredProviders,
  countWaysIn,
  removalLeavesNoWayIn,
} from "@/lib/auth/sign-in-methods"

const everythingOn = {
  emailOtpEnabled: true,
  passwordEnabled: true,
  passkeyEnabled: true,
  googleLoginEnabled: true,
  appleLoginEnabled: true,
  oidcProviders: [{ id: `okta`, name: `Okta` }],
}

const codeOnly = {
  ...everythingOn,
  passwordEnabled: false,
  passkeyEnabled: false,
  googleLoginEnabled: false,
  appleLoginEnabled: false,
  oidcProviders: [],
}

const day = new Date(`2026-09-30T10:00:00Z`)

describe(`configuredProviders`, () => {
  it(`orders Apple, Google, then the OIDC providers`, () => {
    expect(configuredProviders(everythingOn).map((p) => p.id)).toEqual([
      `apple`,
      `google`,
      `okta`,
    ])
  })

  it(`lists nothing on a code-only instance`, () => {
    expect(configuredProviders(codeOnly)).toEqual([])
  })
})

describe(`countWaysIn`, () => {
  it(`counts the email code once, each configured account row, and passkeys`, () => {
    expect(
      countWaysIn({
        accounts: [{ providerId: `google` }, { providerId: `credential` }],
        passkeyCount: 2,
        config: everythingOn,
      })
    ).toBe(5)
  })

  it(`ignores rows for providers the instance no longer offers`, () => {
    expect(
      countWaysIn({
        accounts: [{ providerId: `google` }, { providerId: `gone-oidc` }],
        passkeyCount: 1,
        config: { ...everythingOn, googleLoginEnabled: false, passkeyEnabled: false, emailOtpEnabled: false },
      })
    ).toBe(0)
  })

  it(`the email code alone is a way in`, () => {
    expect(countWaysIn({ accounts: [], passkeyCount: 0, config: codeOnly })).toBe(1)
  })
})

describe(`removalLeavesNoWayIn`, () => {
  const noMail = { ...everythingOn, emailOtpEnabled: false, passwordEnabled: false }

  it(`refuses unlinking the only usable login`, () => {
    expect(
      removalLeavesNoWayIn({
        accounts: [{ providerId: `google` }],
        passkeys: [],
        config: noMail,
        removal: { providerId: `google` },
      })
    ).toBe(true)
  })

  it(`allows unlinking the only row while the email code remains`, () => {
    expect(
      removalLeavesNoWayIn({
        accounts: [{ providerId: `google` }],
        passkeys: [],
        config: everythingOn,
        removal: { providerId: `google` },
      })
    ).toBe(false)
  })

  it(`refuses deleting the last passkey when nothing else signs in`, () => {
    expect(
      removalLeavesNoWayIn({
        accounts: [],
        passkeys: [{ id: `pk1` }],
        config: noMail,
        removal: { passkeyId: `pk1` },
      })
    ).toBe(true)
    expect(
      removalLeavesNoWayIn({
        accounts: [],
        passkeys: [{ id: `pk1` }, { id: `pk2` }],
        config: noMail,
        removal: { passkeyId: `pk1` },
      })
    ).toBe(false)
  })

  it(`lets a dead-provider row go regardless`, () => {
    expect(
      removalLeavesNoWayIn({
        accounts: [{ providerId: `gone-oidc` }, { providerId: `google` }],
        passkeys: [],
        config: noMail,
        removal: { providerId: `gone-oidc` },
      })
    ).toBe(false)
  })
})

describe(`buildSignInMethods`, () => {
  it(`marks configured providers linked or not and appends password + dead rows`, () => {
    const methods = buildSignInMethods({
      user: { email: `a@example.com`, emailVerified: true },
      accounts: [
        { providerId: `google`, createdAt: day },
        { providerId: `credential`, createdAt: day },
        { providerId: `old-idp`, createdAt: day },
      ],
      passkeys: [{ id: `pk`, name: `Mac`, createdAt: day, backedUp: true }],
      config: everythingOn,
    })
    expect(methods.providers.map((p) => [p.id, p.linked, p.available])).toEqual([
      [`apple`, false, true],
      [`google`, true, true],
      [`okta`, false, true],
      [`old-idp`, true, false],
      [`credential`, true, true],
    ])
    expect(methods.providers.find((p) => p.id === `google`)?.linkedAt).toBe(
      day.toISOString()
    )
    expect(methods.providers.find((p) => p.id === `credential`)?.kind).toBe(`password`)
    expect(methods.passkeys).toEqual([
      { id: `pk`, name: `Mac`, createdAt: day.toISOString(), backedUp: true },
    ])
    // code + google + credential + passkey (old-idp is unusable)
    expect(methods.waysIn).toBe(4)
    expect(methods.email).toBe(`a@example.com`)
  })

  it(`hides the password row while none is set`, () => {
    const methods = buildSignInMethods({
      user: { email: `a@example.com`, emailVerified: false },
      accounts: [],
      passkeys: [],
      config: everythingOn,
    })
    expect(methods.providers.some((p) => p.kind === `password`)).toBe(false)
    expect(methods.waysIn).toBe(1)
  })
})
