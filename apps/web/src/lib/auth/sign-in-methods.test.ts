import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  API_KEY_MANAGEMENT_PATHS,
  apiKeyCredentialFromHeaders,
  buildSignInMethods,
  configuredProviders,
  countWaysIn,
  emailChangeNotice,
  isApiKeyRefusedPath,
  isIdentityPath,
  removalLeavesNoWayIn,
} from "@/lib/auth/sign-in-methods"

const everythingOn = {
  emailOtpEnabled: true,
  passwordEnabled: true,
  passkeyEnabled: true,
  googleLoginEnabled: true,
  appleLoginEnabled: true,
  githubLoginEnabled: true,
  oidcProviders: [{ id: `okta`, name: `Okta` }],
}

const codeOnly = {
  ...everythingOn,
  passwordEnabled: false,
  passkeyEnabled: false,
  googleLoginEnabled: false,
  appleLoginEnabled: false,
  githubLoginEnabled: false,
  oidcProviders: [],
}

const day = new Date(`2026-09-30T10:00:00Z`)

describe(`configuredProviders`, () => {
  it(`orders Apple, Google, GitHub, then the OIDC providers`, () => {
    expect(configuredProviders(everythingOn).map((p) => p.id)).toEqual([
      `apple`,
      `google`,
      `github`,
      `okta`,
    ])
  })

  it(`lists nothing on a code-only instance`, () => {
    expect(configuredProviders(codeOnly)).toEqual([])
  })
})

describe(`countWaysIn`, () => {
  it(`counts each configured account row and passkeys, never the email code`, () => {
    expect(
      countWaysIn({
        accounts: [{ providerId: `google` }, { providerId: `credential` }],
        passkeyCount: 2,
        config: everythingOn,
      })
    ).toBe(4)
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

  it(`the email code alone is not a way in (EXP-1209)`, () => {
    expect(countWaysIn({ accounts: [], passkeyCount: 0, config: codeOnly })).toBe(0)
  })
})

describe(`the GitHub row without GitHub login (SLOP-7)`, () => {
  it(`is the repositories connection, not a sign-in method, and not a way in`, () => {
    const config = { ...everythingOn, githubLoginEnabled: false }
    const methods = buildSignInMethods({
      user: { email: `a@b.c`, emailVerified: true },
      accounts: [{ providerId: `github`, createdAt: day }],
      passkeys: [],
      config,
    })
    expect(methods.providers.some((p) => p.id === `github`)).toBe(false)
    expect(
      countWaysIn({ accounts: [{ providerId: `github` }], passkeyCount: 0, config })
    ).toBe(0)
  })

  it(`counts as a way in once GitHub login is on`, () => {
    expect(
      countWaysIn({
        accounts: [{ providerId: `github` }],
        passkeyCount: 0,
        config: { ...everythingOn, emailOtpEnabled: false },
      })
    ).toBe(1)
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

  it(`only Google + the email code => unlinking Google is refused (EXP-1209)`, () => {
    expect(
      removalLeavesNoWayIn({
        accounts: [{ providerId: `google` }],
        passkeys: [],
        config: everythingOn,
        removal: { providerId: `google` },
      })
    ).toBe(true)
  })

  it(`allows unlinking a row while another method remains`, () => {
    expect(
      removalLeavesNoWayIn({
        accounts: [{ providerId: `google` }, { providerId: `credential` }],
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

  it(`removing a row for a provider no longer offered is allowed even with no other way in`, () => {
    expect(
      removalLeavesNoWayIn({
        accounts: [{ providerId: `old-idp` }],
        passkeys: [],
        config: everythingOn,
        removal: { providerId: `old-idp` },
      })
    ).toBe(false)
  })

  it(`removing the password row while password login is off is allowed`, () => {
    expect(
      removalLeavesNoWayIn({
        accounts: [{ providerId: `credential` }],
        passkeys: [],
        config: { ...everythingOn, passwordEnabled: false },
        removal: { providerId: `credential` },
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
      [`github`, false, true],
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
    // google + credential + passkey (old-idp is unusable, the code is no way in)
    expect(methods.waysIn).toBe(3)
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
    expect(methods.waysIn).toBe(0)
  })
})

// The guard plugin's pure pieces; the plugin itself runs against a real
// Better Auth instance in sign-in-methods-guard.test.ts.
describe(`isIdentityPath`, () => {
  it(`matches every identity-changing endpoint and the passkey family`, () => {
    for (const path of [
      `/email-otp/request-email-change`,
      `/email-otp/change-email`,
      `/link-social`,
      `/oauth2/link`,
      `/unlink-account`,
      `/passkey/delete-passkey`,
      `/passkey/generate-register-options`,
      `/passkey/verify-registration`,
      `/passkey/update-passkey`,
    ]) {
      expect(isIdentityPath(path)).toBe(true)
    }
    for (const path of [`/get-session`, `/sign-in/email`, `/email-otp/send-verification-otp`, `/update-user`, undefined]) {
      expect(isIdentityPath(path)).toBe(false)
    }
  })
})

describe(`isApiKeyRefusedPath (FEED-76)`, () => {
  it(`the key-management + device-approval list is exact: a new plugin route is a decision`, () => {
    expect([...API_KEY_MANAGEMENT_PATHS]).toEqual([
      `/api-key/create`,
      `/api-key/get`,
      `/api-key/list`,
      `/api-key/update`,
      `/api-key/delete`,
      `/device/approve`,
      `/device/deny`,
    ])
  })

  it(`refuses every identity path AND every management path, nothing else`, () => {
    for (const path of [
      ...API_KEY_MANAGEMENT_PATHS,
      `/unlink-account`,
      `/passkey/delete-passkey`,
    ]) {
      expect([path, isApiKeyRefusedPath(path)]).toEqual([path, true])
    }
    // The management paths are NOT identity paths: the last-way-in and
    // placeholder rules never run on them.
    for (const path of API_KEY_MANAGEMENT_PATHS) {
      expect([path, isIdentityPath(path)]).toEqual([path, false])
    }
    for (const path of [
      `/get-session`,
      `/sign-in/email`,
      `/device/code`,
      `/device/token`,
      `/device`,
      `/api-key/verify`,
      `/update-user`,
      undefined,
    ]) {
      expect([path, isApiKeyRefusedPath(path)]).toEqual([path, false])
    }
  })
})

describe(`apiKeyCredentialFromHeaders`, () => {
  it(`reads x-api-key or a Bearer expu_ token, nothing else`, () => {
    expect(apiKeyCredentialFromHeaders(new Headers({ "x-api-key": `expu_abc` }))).toBe(`expu_abc`)
    expect(apiKeyCredentialFromHeaders(new Headers({ authorization: `Bearer expu_abc` }))).toBe(`expu_abc`)
    expect(apiKeyCredentialFromHeaders(new Headers({ authorization: `bearer EXPU_abc` }))).toBe(`EXPU_abc`)
    // A session bearer token is not a key.
    expect(apiKeyCredentialFromHeaders(new Headers({ authorization: `Bearer sess_abc` }))).toBeNull()
    expect(apiKeyCredentialFromHeaders(new Headers())).toBeNull()
    expect(apiKeyCredentialFromHeaders(undefined)).toBeNull()
  })
})

describe(`emailChangeNotice`, () => {
  it(`names the old address only for a real change`, () => {
    const session = { user: { email: `Old@Example.com` } }
    expect(emailChangeNotice({ email: `new@example.com` }, session)).toEqual({
      to: `old@example.com`,
      newEmail: `new@example.com`,
    })
    expect(emailChangeNotice({ email: `old@example.com` }, session)).toBeNull()
    expect(emailChangeNotice({}, session)).toBeNull()
    expect(emailChangeNotice({ email: 42 }, session)).toBeNull()
    expect(emailChangeNotice({ email: `new@example.com` }, null)).toBeNull()
    expect(emailChangeNotice({ email: `new@example.com` }, { user: {} })).toBeNull()
  })
})
