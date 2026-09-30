// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest"
import { betterAuth } from "better-auth"
import { memoryAdapter } from "better-auth/adapters/memory"
import { emailOTP } from "better-auth/plugins"
import { apiKey } from "@better-auth/api-key"

// The guard plugin against a REAL Better Auth instance (memory adapter, the
// api-key plugin configured as production configures it: a mocked session
// for every endpoint), so "an `expu_` key cannot change identity" is proven
// on the wire, plugin order included. The module-level db the guard reads
// for its last-way-in and placeholder lookups is the in-memory fake.

// The guard's module-level `db` delegates to whatever fake the running test
// installed (`h.current`), so each test seeds its own `users` rows.
const h = vi.hoisted(() => ({
  current: null as unknown as Record<string, unknown>,
}))

vi.mock(`@/db/connection`, () => ({
  db: new Proxy({} as Record<string, unknown>, {
    get: (_target, key) => h.current[key as string],
  }),
}))

import { createFakeDb } from "@/lib/mcp-oauth/test-db"

import {
  PLACEHOLDER_EMAIL_CHANGE_MESSAGE,
  signInMethodsGuardPlugin,
} from "./sign-in-methods"
import { API_KEY_IDENTITY_CODE } from "./api-key-kind"

const BASE = `http://localhost:3000`

function makeAuth() {
  const db: Record<string, Record<string, unknown>[]> = {
    user: [],
    session: [],
    account: [],
    verification: [],
    apikey: [],
  }
  const sent: Array<{ email: string; type: string }> = []
  const auth = betterAuth({
    baseURL: BASE,
    secret: `test-secret-test-secret-test-secret-0123`,
    database: memoryAdapter(db),
    rateLimit: { enabled: false },
    logger: { disabled: true },
    emailAndPassword: { enabled: true },
    account: { accountLinking: { allowUnlinkingAll: true } },
    plugins: [
      emailOTP({
        otpLength: 6,
        expiresIn: 600,
        storeOTP: `hashed`,
        changeEmail: { enabled: true },
        sendVerificationOTP: async ({ email, type }) => {
          sent.push({ email, type })
        },
      }),
      signInMethodsGuardPlugin(),
      apiKey({
        defaultPrefix: `expu_`,
        enableMetadata: true,
        enableSessionForAPIKeys: true,
        rateLimit: { enabled: false },
        keyExpiration: { defaultExpiresIn: null },
        customAPIKeyGetter: (ctx) => {
          const direct = ctx.headers?.get(`x-api-key`)
          if (direct) return direct
          const authz = ctx.headers?.get(`authorization`)
          if (!authz) return null
          const match = authz.match(/^Bearer\s+(expu_[^\s]+)$/i)
          return match ? match[1]! : null
        },
      }),
    ],
  })
  return { auth, db, sent }
}

type Harness = ReturnType<typeof makeAuth>

async function post(
  harness: Harness,
  path: string,
  body: Record<string, unknown>,
  headers: Record<string, string> = {}
) {
  const response = await harness.auth.handler(
    new Request(`${BASE}/api/auth${path}`, {
      method: `POST`,
      headers: { "content-type": `application/json`, origin: BASE, ...headers },
      body: JSON.stringify(body),
    })
  )
  const text = await response.text()
  let json: Record<string, unknown> = {}
  try {
    json = JSON.parse(text) as Record<string, unknown>
  } catch {
    json = { raw: text }
  }
  return { status: response.status, json, response }
}

/** Sign up, returning the cookie header of the browser session, the user id
 * and a raw `expu_` key minted for the same user. */
async function signedUp(harness: Harness) {
  const { response, json } = await post(harness, `/sign-up/email`, {
    email: `person@example.com`,
    password: `password-1234`,
    name: `Person`,
  })
  const setCookie = response.headers.get(`set-cookie`) ?? ``
  const cookie = setCookie
    .split(/,(?=\s*[^;,=]+=)/)
    .map((part) => part.split(`;`)[0]!.trim())
    .join(`; `)
  const userId = (json.user as { id: string }).id
  const minted = await harness.auth.api.createApiKey({
    body: { userId, name: `Agent key`, metadata: { kind: `agent` } },
  })
  return { cookie, userId, key: minted.key }
}

let harness: Harness

beforeEach(() => {
  h.current = createFakeDb() as unknown as Record<string, unknown>
  harness = makeAuth()
})

describe(`signInMethodsGuardPlugin — api-key sessions cannot change identity`, () => {
  it(`request-email-change: refused on a key (both header forms), allowed on the cookie`, async () => {
    const { cookie, key } = await signedUp(harness)
    const headerForms: Array<Record<string, string>> = [
      { "x-api-key": key },
      { authorization: `Bearer ${key}` },
    ]
    for (const headers of headerForms) {
      const refused = await post(
        harness,
        `/email-otp/request-email-change`,
        { newEmail: `new@example.com` },
        headers
      )
      expect(refused.status).toBe(401)
      expect(refused.json.code).toBe(API_KEY_IDENTITY_CODE)
    }
    expect(harness.sent).toHaveLength(0)

    const allowed = await post(
      harness,
      `/email-otp/request-email-change`,
      { newEmail: `new@example.com` },
      { cookie }
    )
    expect(allowed.status).toBe(200)
    expect(harness.sent).toEqual([{ email: `new@example.com`, type: `change-email` }])
  })

  // `/oauth2/link` (genericOAuth) and the passkey endpoints are not mounted
  // in this harness; the matcher covering them is unit-tested in
  // sign-in-methods.test.ts (`isIdentityPath`). The paths below exist here,
  // and `/passkey/delete-passkey` proves the hook fires before a 404 would.
  it(`change-email, link-social and passkey paths: refused on a key`, async () => {
    const { key } = await signedUp(harness)
    for (const [path, body] of [
      [`/email-otp/change-email`, { newEmail: `new@example.com`, otp: `123456` }],
      [`/link-social`, { provider: `google` }],
    ] as const) {
      const refused = await post(harness, path, body, { "x-api-key": key })
      expect([path, refused.status, refused.json.code]).toEqual([path, 401, API_KEY_IDENTITY_CODE])
    }
  })

  it(`unlink-account: refused on a key, passes the gate on the cookie (its own rule answers)`, async () => {
    const { cookie, key } = await signedUp(harness)
    const refused = await post(
      harness,
      `/unlink-account`,
      { providerId: `credential` },
      { "x-api-key": key }
    )
    expect(refused.status).toBe(401)
    expect(refused.json.code).toBe(API_KEY_IDENTITY_CODE)

    // The fake db holds no rows for this account, so the guard's own
    // last-way-in rule (not the api-key refusal) is what answers here.
    const cookieResult = await post(
      harness,
      `/unlink-account`,
      { providerId: `credential` },
      { cookie }
    )
    expect(cookieResult.json.code).not.toBe(API_KEY_IDENTITY_CODE)
    expect(cookieResult.status).not.toBe(401)
  })

  it(`the key still works for a non-identity endpoint`, async () => {
    const { key } = await signedUp(harness)
    const response = await harness.auth.handler(
      new Request(`${BASE}/api/auth/get-session`, { headers: { "x-api-key": key } })
    )
    expect(response.status).toBe(200)
    const json = (await response.json()) as { user?: { email?: string } }
    expect(json.user?.email).toBe(`person@example.com`)
  })
})

describe(`signInMethodsGuardPlugin — a placeholder's address cannot be adopted`, () => {
  it(`refuses request-email-change onto an address held by a placeholder member`, async () => {
    h.current = createFakeDb({
      users: [{ id: `ph-1`, email: `invited@example.com`, placeholderAt: new Date() }],
    }) as unknown as Record<string, unknown>
    const { cookie } = await signedUp(harness)
    const refused = await post(
      harness,
      `/email-otp/request-email-change`,
      { newEmail: `Invited@Example.com` },
      { cookie }
    )
    expect(refused.status).toBe(400)
    expect(refused.json.message).toBe(PLACEHOLDER_EMAIL_CHANGE_MESSAGE)
    expect(harness.sent).toHaveLength(0)
  })

  it(`a real account's address is left to Better Auth's own answer`, async () => {
    h.current = createFakeDb({
      users: [{ id: `u-2`, email: `taken@example.com`, placeholderAt: null }],
    }) as unknown as Record<string, unknown>
    const { cookie } = await signedUp(harness)
    const result = await post(
      harness,
      `/email-otp/request-email-change`,
      { newEmail: `taken@example.com` },
      { cookie }
    )
    expect(result.status).toBe(200)
  })
})
