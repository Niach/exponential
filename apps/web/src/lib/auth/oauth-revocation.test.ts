import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// REV2-76: account deletion used to revoke ONLY the Sign-in-with-Apple
// pairing, so the Google offline refresh token (accessType: 'offline' — minted
// at first sign-in) and generic OIDC grants cascaded away unrevoked. These
// lock the generalized contract: every token-bearing accounts row is captured
// (provider id included — the dispatcher keys on it), Apple still goes through
// the Apple module, and every other provider gets an RFC 7009 POST that never
// throws.

const h = vi.hoisted(() => ({ appleCalls: [] as unknown[][] }))

vi.mock(`./apple-revocation`, () => ({
  revokeAppleTokensBestEffort: async (rows: unknown[]) => {
    h.appleCalls.push(rows)
  },
}))

import { accounts } from "@/db/auth-schema"
import {
  buildRevocationBody,
  captureOAuthTokens,
  pickRevocableToken,
  resolveRevocationTarget,
  revokeOAuthTokensBestEffort,
} from "./oauth-revocation"

const fetchMock = vi.fn()

beforeEach(() => {
  h.appleCalls.length = 0
  fetchMock.mockReset()
  vi.stubGlobal(`fetch`, fetchMock)
  vi.spyOn(console, `error`).mockImplementation(() => {})
  delete process.env.OIDC_PROVIDERS
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

function ok(body: unknown = {}) {
  return {
    ok: true,
    status: 200,
    json: async () => body,
    text: async () => ``,
  }
}

describe(`captureOAuthTokens`, () => {
  it(`selects the provider id alongside both tokens`, async () => {
    let selected: Record<string, unknown> | undefined
    const rows = [
      { providerId: `google`, accessToken: `at`, refreshToken: `rt` },
    ]
    const db = {
      select: (cols: Record<string, unknown>) => {
        selected = cols
        const chain = {
          from: () => chain,
          where: () => Promise.resolve(rows),
        }
        return chain
      },
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const captured = await captureOAuthTokens(db as any, `user-1`)
    expect(captured).toEqual(rows)
    expect(selected).toEqual({
      providerId: accounts.providerId,
      accessToken: accounts.accessToken,
      refreshToken: accounts.refreshToken,
    })
  })
})

describe(`pickRevocableToken`, () => {
  it(`prefers the refresh token (revoking it retires the whole grant)`, () => {
    expect(
      pickRevocableToken({ accessToken: `at`, refreshToken: `rt` })
    ).toEqual({ token: `rt`, tokenTypeHint: `refresh_token` })
  })

  it(`falls back to the access token`, () => {
    expect(
      pickRevocableToken({ accessToken: `at`, refreshToken: null })
    ).toEqual({ token: `at`, tokenTypeHint: `access_token` })
  })

  it(`treats blank tokens as absent`, () => {
    expect(pickRevocableToken({ accessToken: ` `, refreshToken: null })).toBe(
      null
    )
    expect(pickRevocableToken({ accessToken: null, refreshToken: null })).toBe(
      null
    )
  })
})

describe(`buildRevocationBody`, () => {
  it(`always carries the token and its type hint`, () => {
    const params = new URLSearchParams(
      buildRevocationBody({ token: `rt-1`, tokenTypeHint: `refresh_token` })
    )
    expect(params.get(`token`)).toBe(`rt-1`)
    expect(params.get(`token_type_hint`)).toBe(`refresh_token`)
    expect(params.get(`client_id`)).toBe(null)
    expect(params.get(`client_secret`)).toBe(null)
  })

  it(`adds client authentication when the provider expects it`, () => {
    const params = new URLSearchParams(
      buildRevocationBody({
        token: `tok en&x`,
        tokenTypeHint: `access_token`,
        clientId: `cid`,
        clientSecret: `a.b+c/d=`,
      })
    )
    expect(params.get(`client_id`)).toBe(`cid`)
    expect(params.get(`client_secret`)).toBe(`a.b+c/d=`)
    expect(params.get(`token`)).toBe(`tok en&x`)
  })
})

describe(`resolveRevocationTarget`, () => {
  it(`points Google at its documented endpoint with no client auth`, async () => {
    expect(await resolveRevocationTarget(`google`)).toEqual({
      endpoint: `https://oauth2.googleapis.com/revoke`,
    })
  })

  it(`still targets Google after the client config was rotated away`, async () => {
    // The endpoint takes the bare token, so a deletion must not silently skip
    // revocation just because GOOGLE_CLIENT_ID is no longer set.
    delete process.env.GOOGLE_CLIENT_ID
    delete process.env.GOOGLE_CLIENT_SECRET
    expect(await resolveRevocationTarget(`google`)).toEqual({
      endpoint: `https://oauth2.googleapis.com/revoke`,
    })
  })

  it(`reads the OIDC revocation_endpoint out of the discovery document`, async () => {
    process.env.OIDC_PROVIDERS = JSON.stringify([
      {
        id: `authentik`,
        clientId: `oidc-client`,
        clientSecret: `oidc-secret`,
        discoveryUrl: `https://idp.test/a/.well-known/openid-configuration`,
      },
    ])
    fetchMock.mockResolvedValue(
      ok({ revocation_endpoint: `https://idp.test/a/revoke` })
    )
    expect(await resolveRevocationTarget(`authentik`)).toEqual({
      endpoint: `https://idp.test/a/revoke`,
      clientId: `oidc-client`,
      clientSecret: `oidc-secret`,
    })
  })

  it(`refuses a plaintext http revocation endpoint`, async () => {
    // The endpoint arrives in a remote document and we POST a live OAuth token
    // plus the client secret to it — over http that hands the credential to
    // anyone on the path. Treated exactly like "advertises none".
    process.env.OIDC_PROVIDERS = JSON.stringify([
      {
        id: `authentik`,
        clientId: `oidc-client`,
        clientSecret: `oidc-secret`,
        discoveryUrl: `https://idp.test/plaintext/.well-known/openid-configuration`,
      },
    ])
    fetchMock.mockResolvedValue(
      ok({ revocation_endpoint: `http://idp.test/plaintext/revoke` })
    )
    expect(await resolveRevocationTarget(`authentik`)).toBe(null)
  })

  it(`refuses a non-http(s) revocation endpoint scheme`, async () => {
    process.env.OIDC_PROVIDERS = JSON.stringify([
      {
        id: `authentik`,
        clientId: `oidc-client`,
        clientSecret: `oidc-secret`,
        discoveryUrl: `https://idp.test/scheme/.well-known/openid-configuration`,
      },
    ])
    fetchMock.mockResolvedValue(
      ok({ revocation_endpoint: `file:///etc/passwd` })
    )
    expect(await resolveRevocationTarget(`authentik`)).toBe(null)
  })

  it(`returns null for a provider that advertises no revocation endpoint`, async () => {
    process.env.OIDC_PROVIDERS = JSON.stringify([
      {
        id: `authentik`,
        clientId: `oidc-client`,
        clientSecret: `oidc-secret`,
        discoveryUrl: `https://idp.test/b/.well-known/openid-configuration`,
      },
    ])
    fetchMock.mockResolvedValue(ok({}))
    expect(await resolveRevocationTarget(`authentik`)).toBe(null)
  })

  it(`returns null for an unknown provider`, async () => {
    expect(await resolveRevocationTarget(`credential`)).toBe(null)
  })

  it(`caches a successfully read discovery document`, async () => {
    // Static config: several rows of the same provider (and later deletions)
    // must not refetch it.
    process.env.OIDC_PROVIDERS = JSON.stringify([
      {
        id: `authentik`,
        clientId: `oidc-client`,
        clientSecret: `oidc-secret`,
        discoveryUrl: `https://idp.test/c/.well-known/openid-configuration`,
      },
    ])
    fetchMock.mockResolvedValue(
      ok({ revocation_endpoint: `https://idp.test/c/revoke` })
    )
    await resolveRevocationTarget(`authentik`)
    await resolveRevocationTarget(`authentik`)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it(`retries discovery after a transient failure instead of caching it`, async () => {
    // A single timeout/502 must not disable OIDC revocation for the rest of the
    // process lifetime — that would silently skip every later deletion.
    process.env.OIDC_PROVIDERS = JSON.stringify([
      {
        id: `authentik`,
        clientId: `oidc-client`,
        clientSecret: `oidc-secret`,
        discoveryUrl: `https://idp.test/d/.well-known/openid-configuration`,
      },
    ])
    fetchMock
      .mockRejectedValueOnce(new Error(`timed out`))
      .mockResolvedValueOnce({ ok: false, status: 502, text: async () => `` })
      .mockResolvedValue(
        ok({ revocation_endpoint: `https://idp.test/d/revoke` })
      )
    expect(await resolveRevocationTarget(`authentik`)).toBe(null)
    expect(await resolveRevocationTarget(`authentik`)).toBe(null)
    expect(await resolveRevocationTarget(`authentik`)).toEqual({
      endpoint: `https://idp.test/d/revoke`,
      clientId: `oidc-client`,
      clientSecret: `oidc-secret`,
    })
    expect(fetchMock).toHaveBeenCalledTimes(3)
  })
})

describe(`revokeOAuthTokensBestEffort`, () => {
  it(`revokes the Google grant with the refresh token`, async () => {
    fetchMock.mockResolvedValue(ok())
    await revokeOAuthTokensBestEffort([
      { providerId: `google`, accessToken: `at-1`, refreshToken: `rt-1` },
    ])
    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe(`https://oauth2.googleapis.com/revoke`)
    expect(init.method).toBe(`POST`)
    const params = new URLSearchParams(init.body)
    expect(params.get(`token`)).toBe(`rt-1`)
    expect(params.get(`token_type_hint`)).toBe(`refresh_token`)
    // A hung provider must not stall the (already committed) deletion.
    expect(init.signal).toBeDefined()
  })

  it(`hands Apple rows to the Apple module and sends no generic request`, async () => {
    await revokeOAuthTokensBestEffort([
      { providerId: `apple`, accessToken: null, refreshToken: `apple-rt` },
    ])
    expect(h.appleCalls).toEqual([
      [{ providerId: `apple`, accessToken: null, refreshToken: `apple-rt` }],
    ])
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it(`sends nothing for a provider with no revocation target`, async () => {
    await revokeOAuthTokensBestEffort([
      { providerId: `credential`, accessToken: `at`, refreshToken: null },
    ])
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it(`never throws when the provider is unreachable or rejects`, async () => {
    fetchMock
      .mockRejectedValueOnce(new Error(`network down`))
      .mockResolvedValueOnce({
        ok: false,
        status: 400,
        text: async () => `invalid_token`,
      })
    await expect(
      revokeOAuthTokensBestEffort([
        { providerId: `google`, accessToken: null, refreshToken: `rt-1` },
        { providerId: `google`, accessToken: null, refreshToken: `rt-2` },
      ])
    ).resolves.toBeUndefined()
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })
})
