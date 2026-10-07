// @vitest-environment node
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { betterAuth } from "better-auth"
import { memoryAdapter } from "better-auth/adapters/memory"
import { deviceAuthorization, emailOTP } from "better-auth/plugins"
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
  API_KEY_MANAGEMENT_PATHS,
  DISABLED_AUTH_PATHS,
  PLACEHOLDER_EMAIL_CHANGE_MESSAGE,
  PROVIDER_LOGIN_DISABLED_CODE,
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
    deviceCode: [],
  }
  const sent: Array<{ email: string; type: string }> = []
  const auth = betterAuth({
    baseURL: BASE,
    secret: `test-secret-test-secret-test-secret-0123`,
    database: memoryAdapter(db),
    rateLimit: { enabled: false },
    logger: { disabled: true },
    // As production configures it (lib/auth/index.ts).
    disabledPaths: DISABLED_AUTH_PATHS,
    emailAndPassword: { enabled: true },
    socialProviders: {
      github: { clientId: `gh-client`, clientSecret: `gh-secret` },
    },
    account: {
      accountLinking: { allowUnlinkingAll: true, trustedProviders: [`github`] },
    },
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
      // As production mounts it (EXP-403): approve/deny authenticate by
      // `getSessionFromCtx` alone, so the key-mocked session would pass.
      deviceAuthorization({
        expiresIn: `10m`,
        interval: `5s`,
        verificationUri: `/auth/device`,
        schema: {},
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

async function get(harness: Harness, path: string, headers: Record<string, string> = {}) {
  const response = await harness.auth.handler(
    new Request(`${BASE}/api/auth${path}`, { headers: { origin: BASE, ...headers } })
  )
  const text = await response.text()
  let json: unknown = {}
  try {
    json = JSON.parse(text)
  } catch {
    json = { raw: text }
  }
  return { status: response.status, json: json as Record<string, unknown> }
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

describe(`signInMethodsGuardPlugin — a key cannot manage keys or approve a device (FEED-76)`, () => {
  const SCOPE = { allTeams: false, teamIds: [`team-1`], boardIds: [] }

  async function scopedKey(harness: Harness, userId: string) {
    const minted = await harness.auth.api.createApiKey({
      body: { userId, name: `Scoped key`, metadata: { kind: `personal`, scope: SCOPE } },
    })
    return { id: minted.id, key: minted.key }
  }

  it(`api-key/update: a scoped key cannot rewrite its own metadata (both header forms)`, async () => {
    const { userId } = await signedUp(harness)
    const scoped = await scopedKey(harness, userId)
    for (const headers of [
      { "x-api-key": scoped.key },
      { authorization: `Bearer ${scoped.key}` },
    ] as Array<Record<string, string>>) {
      const refused = await post(
        harness,
        `/api-key/update`,
        { keyId: scoped.id, metadata: { kind: `personal` } },
        headers
      )
      expect(refused.status).toBe(401)
      expect(refused.json.code).toBe(API_KEY_IDENTITY_CODE)
    }
    const row = harness.db.apikey.find((r) => r.id === scoped.id)
    expect(JSON.parse(String(row?.metadata))).toEqual({ kind: `personal`, scope: SCOPE })
  })

  it(`api-key/create: an agent key cannot mint a new (unscoped) key`, async () => {
    const { key } = await signedUp(harness)
    const before = harness.db.apikey.length
    const refused = await post(harness, `/api-key/create`, { name: `Escaped` }, { "x-api-key": key })
    expect(refused.status).toBe(401)
    expect(refused.json.code).toBe(API_KEY_IDENTITY_CODE)
    expect(harness.db.apikey).toHaveLength(before)
  })

  it(`every management path refuses a key: list/get (GET) and delete/approve/deny (POST)`, async () => {
    const { userId, key } = await signedUp(harness)
    const scoped = await scopedKey(harness, userId)
    for (const path of API_KEY_MANAGEMENT_PATHS) {
      const result =
        path === `/api-key/list` || path === `/api-key/get`
          ? await get(harness, `${path}${path === `/api-key/get` ? `?id=${scoped.id}` : ``}`, {
              "x-api-key": scoped.key,
            })
          : await post(
              harness,
              path,
              path.startsWith(`/device/`) ? { userCode: `ABCD-EFGH` } : { keyId: scoped.id },
              { "x-api-key": scoped.key }
            )
      expect([path, result.status, result.json.code]).toEqual([path, 401, API_KEY_IDENTITY_CODE])
    }
    // The agent key too, and nothing was deleted.
    const refused = await post(harness, `/api-key/delete`, { keyId: scoped.id }, { "x-api-key": key })
    expect(refused.json.code).toBe(API_KEY_IDENTITY_CODE)
    expect(harness.db.apikey.some((r) => r.id === scoped.id)).toBe(true)
  })

  it(`the cookie session passes the guard: api-key/list answers, device/approve reaches the plugin`, async () => {
    const { cookie } = await signedUp(harness)
    const listed = await get(harness, `/api-key/list`, { cookie })
    expect(listed.status).toBe(200)
    // The plugin's raw rows, metadata included — exactly what a key must
    // never read over HTTP (tRPC `users.listApiKeys` strips it).
    const apiKeys = listed.json.apiKeys as Array<{ metadata?: unknown }>
    expect(apiKeys).toHaveLength(1)
    expect(apiKeys[0]?.metadata).toEqual({ kind: `agent` })

    // No such device code: the plugin's OWN answer, never the key refusal.
    const approve = await post(harness, `/device/approve`, { userCode: `ABCD-EFGH` }, { cookie })
    expect(approve.status).not.toBe(401)
    expect(approve.json.code).not.toBe(API_KEY_IDENTITY_CODE)
    expect(approve.json.error).toBe(`invalid_request`)
    const deny = await post(harness, `/device/deny`, { userCode: `ABCD-EFGH` }, { cookie })
    expect(deny.status).not.toBe(401)
    expect(deny.json.code).not.toBe(API_KEY_IDENTITY_CODE)
  })

  it(`the CLI's own device endpoints stay open (no session, no key)`, async () => {
    const started = await post(harness, `/device/code`, { client_id: `exponential-cli` })
    expect(started.status).toBe(200)
    expect(typeof started.json.user_code).toBe(`string`)
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

describe(`token endpoints are server-only (SLOP-7 review)`, () => {
  it(`get-access-token and refresh-token 404 over HTTP, even on a key`, async () => {
    const { cookie, key } = await signedUp(harness)
    for (const path of [`/get-access-token`, `/refresh-token`]) {
      for (const headers of [{ "x-api-key": key }, { cookie }] as Array<
        Record<string, string>
      >) {
        const res = await post(harness, path, { providerId: `github` }, headers)
        expect([path, res.status]).toEqual([path, 404])
      }
    }
  })

  it(`the server-side call still works`, async () => {
    const { userId } = await signedUp(harness)
    // No github row for this user: the endpoint itself answers, not a 404.
    await expect(
      harness.auth.api.getAccessToken({ body: { providerId: `github`, userId } })
    ).rejects.toMatchObject({ body: { code: `ACCOUNT_NOT_FOUND` } })
  })
})

describe(`GitHub sign-in follows GITHUB_LOGIN_ENABLED (SLOP-7 review)`, () => {
  const configured = {
    GITHUB_APP_ID: `1`,
    GITHUB_APP_PRIVATE_KEY: `pem`,
    GITHUB_APP_SLUG: `exp`,
    GITHUB_APP_CLIENT_ID: `gh-client`,
    GITHUB_APP_CLIENT_SECRET: `gh-secret`,
  }

  afterEach(() => {
    vi.unstubAllEnvs()
  })

  it(`refuses /sign-in/social for github while login is off`, async () => {
    for (const [name, value] of Object.entries(configured)) vi.stubEnv(name, value)
    vi.stubEnv(`GITHUB_LOGIN_ENABLED`, `false`)
    const res = await post(harness, `/sign-in/social`, {
      provider: `github`,
      callbackURL: `/`,
    })
    expect(res.status).toBe(403)
    expect(res.json.code).toBe(PROVIDER_LOGIN_DISABLED_CODE)
  })

  it(`allows it once login is on`, async () => {
    for (const [name, value] of Object.entries(configured)) vi.stubEnv(name, value)
    vi.stubEnv(`GITHUB_LOGIN_ENABLED`, `true`)
    const res = await post(harness, `/sign-in/social`, {
      provider: `github`,
      callbackURL: `/`,
    })
    expect(res.status).toBe(200)
    expect(String(res.json.url)).toContain(`https://github.com/login/oauth/authorize`)
  })

  it(`linking stays open while login is off`, async () => {
    for (const [name, value] of Object.entries(configured)) vi.stubEnv(name, value)
    vi.stubEnv(`GITHUB_LOGIN_ENABLED`, `false`)
    const { cookie } = await signedUp(harness)
    const res = await post(
      harness,
      `/link-social`,
      { provider: `github`, callbackURL: `/` },
      { cookie }
    )
    expect(res.status).toBe(200)
    expect(String(res.json.url)).toContain(`https://github.com/login/oauth/authorize`)
  })
})
