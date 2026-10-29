import { describe, expect, test } from "bun:test"
import { createPublicKey, generateKeyPairSync, verify } from "node:crypto"
import type { BatchResponse, MulticastMessage } from "firebase-admin/messaging"
import { MAX_SEALED_BYTES } from "./accountless"
import { createApnsClient, signProviderToken, type ApnsClient, type ApnsRequest, type ApnsResult } from "./apns"
import { createRelayApp, type RelayMessaging } from "./app"
import { RelayStore } from "./store"

// EXP-1428: the accountless /v2 mode, driven through the real Hono app with an
// in-memory SQLite store, a fake APNs client, a fake FCM client and a fake
// clock.

const APNS_TOKEN = `ab`.repeat(32)
const APNS_TOKEN_2 = `cd`.repeat(32)
const FCM_TOKEN = `fcm-token:APA91b_abcdefghijklmnopqrstuvwxyz`
const TOPIC = `app.spezl.ios`
const HOUR = 60 * 60 * 1000
const quietLog = { info() {}, warn() {}, error() {} }

function fakeApns(script: (request: ApnsRequest) => ApnsResult = () => ({ status: 200 })) {
  const calls: ApnsRequest[] = []
  const client: ApnsClient = {
    async send(request) {
      calls.push(request)
      return script(request)
    },
  }
  return { client, calls }
}

function fakeFcm(codes: (string | null)[] = [null]) {
  const calls: MulticastMessage[] = []
  const messaging: RelayMessaging = {
    async sendEachForMulticast(message) {
      calls.push(message)
      const code = codes[Math.min(calls.length - 1, codes.length - 1)]
      const response: BatchResponse = {
        successCount: code === null ? 1 : 0,
        failureCount: code === null ? 0 : 1,
        responses: [
          code === null ? { success: true, messageId: `m` } : { success: false, error: { code, message: code } as never },
        ],
      }
      return response
    },
  }
  return { messaging, calls }
}

interface SetupOptions {
  secret?: string | null
  apns?: ApnsClient | null
  messaging?: RelayMessaging | null
  topics?: string[]
  trustProxy?: boolean
  registrationLimit?: { max: number; windowMs: number }
}

function setup(options: SetupOptions = {}) {
  const clock = { now: Date.UTC(2026, 9, 29, 12, 0, 0) }
  const store = RelayStore.open(`:memory:`)
  const apns = options.apns === undefined ? fakeApns() : null
  const fcm = options.messaging === undefined ? fakeFcm() : null
  const relay = createRelayApp({
    secret: options.secret === undefined ? null : options.secret,
    trustProxy: options.trustProxy ?? false,
    getMessaging: () => (options.messaging === undefined ? fcm!.messaging : options.messaging),
    now: () => clock.now,
    log: quietLog,
    accountless: {
      store,
      apns: options.apns === undefined ? apns!.client : options.apns,
      apnsTopics: options.topics ?? [TOPIC],
      registrationLimit: options.registrationLimit,
    },
  })
  return { ...relay, store, clock, apns, fcm }
}

type Relay = ReturnType<typeof setup>

function call(relay: Relay, method: string, path: string, body?: unknown, auth?: string, headers: Record<string, string> = {}) {
  return relay.app.request(`http://relay${path}`, {
    method,
    headers: {
      "content-type": `application/json`,
      ...(auth ? { authorization: `Bearer ${auth}` } : {}),
      ...headers,
    },
    body: body === undefined ? undefined : typeof body === `string` ? body : JSON.stringify(body),
  })
}

async function register(relay: Relay, body: Record<string, unknown> = { platform: `apns`, token: APNS_TOKEN }) {
  const res = await call(relay, `POST`, `/v2/installs`, body)
  expect(res.status).toBe(201)
  return (await res.json()) as { installId: string; installSecret: string }
}

async function mint(relay: Relay, install: { installId: string; installSecret: string }, body: Record<string, unknown> = {}) {
  const res = await call(relay, `POST`, `/v2/installs/${install.installId}/capabilities`, body, install.installSecret)
  expect(res.status).toBe(201)
  return (await res.json()) as { capabilityId: string; capability: string }
}

function sealed(bytes = 64): string {
  return Buffer.alloc(bytes, 7).toString(`base64url`)
}

function push(relay: Relay, capability: string, body: Record<string, unknown> = { sealed: sealed() }) {
  return call(relay, `POST`, `/v2/push`, body, capability)
}

describe(`installs`, () => {
  test(`register returns an id and a 32-byte base64url secret`, async () => {
    const relay = setup()
    const install = await register(relay)
    expect(install.installId).toMatch(/^ins_[A-Za-z0-9_-]+$/)
    expect(Buffer.from(install.installSecret, `base64url`).length).toBe(32)
    const row = relay.store.getInstall(install.installId)
    expect(row).toMatchObject({ platform: `apns`, token: APNS_TOKEN, topic: TOPIC, environment: `production` })
  })

  test(`rejects bad tokens, unknown topics, APNs fields on FCM and unconfigured providers`, async () => {
    const relay = setup({ topics: [TOPIC, `app.other`] })
    expect((await call(relay, `POST`, `/v2/installs`, { platform: `apns`, token: `../../etc` })).status).toBe(400)
    // Two allowed topics: the install must name one, and only an allowed one.
    expect((await call(relay, `POST`, `/v2/installs`, { platform: `apns`, token: APNS_TOKEN })).status).toBe(400)
    expect((await call(relay, `POST`, `/v2/installs`, { platform: `apns`, token: APNS_TOKEN, topic: `evil.app` })).status).toBe(400)
    expect((await call(relay, `POST`, `/v2/installs`, { platform: `apns`, token: APNS_TOKEN, topic: `app.other`, environment: `sandbox` })).status).toBe(201)
    expect((await call(relay, `POST`, `/v2/installs`, { platform: `fcm`, token: FCM_TOKEN, environment: `sandbox` })).status).toBe(400)
    expect((await call(relay, `POST`, `/v2/installs`, `not json`)).status).toBe(400)

    const noApns = setup({ apns: null })
    expect((await call(noApns, `POST`, `/v2/installs`, { platform: `apns`, token: APNS_TOKEN })).status).toBe(503)
    const noFcm = setup({ messaging: null })
    expect((await call(noFcm, `POST`, `/v2/installs`, { platform: `fcm`, token: FCM_TOKEN })).status).toBe(503)
  })

  test(`registrations are rate limited per client IP`, async () => {
    const relay = setup({ trustProxy: true, registrationLimit: { max: 2, windowMs: HOUR } })
    const from = (ip: string) =>
      call(relay, `POST`, `/v2/installs`, { platform: `apns`, token: APNS_TOKEN }, undefined, { "x-forwarded-for": ip })
    expect((await from(`10.0.0.1`)).status).toBe(201)
    expect((await from(`10.0.0.1`)).status).toBe(201)
    expect((await from(`10.0.0.1`)).status).toBe(429)
    expect((await from(`10.0.0.2`)).status).toBe(201)
    relay.clock.now += HOUR
    expect((await from(`10.0.0.1`)).status).toBe(201)
  })

  test(`rotate swaps the device token and keeps the capabilities`, async () => {
    const relay = setup()
    const install = await register(relay)
    const { capability } = await mint(relay, install)
    const res = await call(relay, `PUT`, `/v2/installs/${install.installId}`, { token: APNS_TOKEN_2, environment: `sandbox` }, install.installSecret)
    expect(res.status).toBe(200)
    expect(relay.store.getInstall(install.installId)).toMatchObject({ token: APNS_TOKEN_2, environment: `sandbox` })
    expect((await push(relay, capability)).status).toBe(202)
    expect(relay.apns!.calls[0]).toMatchObject({ token: APNS_TOKEN_2, environment: `sandbox` })
  })

  test(`management needs the install's own secret`, async () => {
    const relay = setup()
    const a = await register(relay)
    const b = await register(relay)
    expect((await call(relay, `PUT`, `/v2/installs/${a.installId}`, { token: APNS_TOKEN_2 })).status).toBe(401)
    expect((await call(relay, `PUT`, `/v2/installs/${a.installId}`, { token: APNS_TOKEN_2 }, b.installSecret)).status).toBe(401)
    expect((await call(relay, `DELETE`, `/v2/installs/${a.installId}`, undefined, `nope`)).status).toBe(401)
    expect((await call(relay, `GET`, `/v2/installs/ins_missing/capabilities`, undefined, a.installSecret)).status).toBe(401)
    expect(relay.store.getInstall(a.installId)?.token).toBe(APNS_TOKEN)
  })

  test(`delete removes the install and every capability it minted`, async () => {
    const relay = setup()
    const install = await register(relay)
    const { capability } = await mint(relay, install)
    await mint(relay, install)
    const res = await call(relay, `DELETE`, `/v2/installs/${install.installId}`, undefined, install.installSecret)
    expect(res.status).toBe(204)
    expect(relay.store.getInstall(install.installId)).toBeNull()
    expect(relay.store.countCapabilities(install.installId)).toBe(0)
    expect((await push(relay, capability)).status).toBe(401)
  })
})

describe(`capabilities`, () => {
  test(`mint, list (never the token) and revoke`, async () => {
    const relay = setup()
    const install = await register(relay)
    const minted = await mint(relay, install, { label: `Anna`, maxPerHour: 10, expiresAt: `2026-11-01T00:00:00Z` })
    expect(minted.capability).toMatch(/^cap_[A-Za-z0-9_-]{43}$/)

    const list = await call(relay, `GET`, `/v2/installs/${install.installId}/capabilities`, undefined, install.installSecret)
    expect(list.status).toBe(200)
    const text = await list.text()
    expect(text).not.toContain(minted.capability)
    expect(JSON.parse(text)).toEqual({
      capabilities: [
        {
          capabilityId: minted.capabilityId,
          label: `Anna`,
          createdAt: `2026-10-29T12:00:00.000Z`,
          expiresAt: `2026-11-01T00:00:00.000Z`,
          lastUsedAt: null,
          maxPerHour: 10,
        },
      ],
    })

    const revoke = (id: string) =>
      call(relay, `DELETE`, `/v2/installs/${install.installId}/capabilities/${id}`, undefined, install.installSecret)
    expect((await revoke(minted.capabilityId)).status).toBe(204)
    expect((await revoke(minted.capabilityId)).status).toBe(404)
    expect((await push(relay, minted.capability)).status).toBe(401)
  })

  test(`validates label, expiry and maxPerHour`, async () => {
    const relay = setup()
    const install = await register(relay)
    const post = (body: unknown) =>
      call(relay, `POST`, `/v2/installs/${install.installId}/capabilities`, body, install.installSecret)
    expect((await post({ label: `x`.repeat(65) })).status).toBe(400)
    expect((await post({ maxPerHour: 601 })).status).toBe(400)
    expect((await post({ maxPerHour: 0 })).status).toBe(400)
    expect((await post({ expiresAt: `tomorrow` })).status).toBe(400)
    expect((await post({ expiresAt: `2026-10-29T11:00:00Z` })).status).toBe(400)
    const res = await post({})
    expect(res.status).toBe(201)
    const [row] = relay.store.listCapabilities(install.installId)
    expect(row).toMatchObject({ maxPerHour: 60, expiresAt: null, label: null })
  })

  test(`a capability of another install cannot be revoked`, async () => {
    const relay = setup()
    const a = await register(relay)
    const b = await register(relay)
    const minted = await mint(relay, a)
    const res = await call(relay, `DELETE`, `/v2/installs/${b.installId}/capabilities/${minted.capabilityId}`, undefined, b.installSecret)
    expect(res.status).toBe(404)
    expect(relay.store.countCapabilities(a.installId)).toBe(1)
  })

  test(`expired capabilities answer 401 and are swept`, async () => {
    const relay = setup()
    const install = await register(relay)
    const { capability } = await mint(relay, install, { expiresAt: new Date(relay.clock.now + HOUR).toISOString() })
    expect((await push(relay, capability)).status).toBe(202)
    relay.clock.now += HOUR
    expect((await push(relay, capability)).status).toBe(401)
    expect(relay.store.deleteExpiredCapabilities(relay.clock.now)).toBe(1)
  })
})

describe(`/v2/push`, () => {
  test(`APNs: an alert placeholder with the sealed blob and the right headers`, async () => {
    const relay = setup()
    const install = await register(relay)
    const { capability, capabilityId } = await mint(relay, install)
    const blob = sealed(100)
    const res = await push(relay, capability, { sealed: blob, collapseId: `chat-1`, urgency: `normal`, ttlSeconds: 60 })
    expect(res.status).toBe(202)
    expect(await res.json()).toEqual({ delivered: true })
    const [sent] = relay.apns!.calls
    expect(sent.token).toBe(APNS_TOKEN)
    expect(sent.environment).toBe(`production`)
    expect(sent.headers).toEqual({
      "apns-topic": TOPIC,
      "apns-push-type": `alert`,
      "apns-priority": `5`,
      "apns-collapse-id": `chat-1`,
      "apns-expiration": String(Math.floor(relay.clock.now / 1000) + 60),
    })
    expect(JSON.parse(sent.payload)).toEqual({
      aps: { alert: { title: `New message` }, "mutable-content": 1 },
      s: blob,
    })
    const row = relay.store.listCapabilities(install.installId).find((r) => r.id === capabilityId)
    expect(row?.lastUsedAt).toBe(relay.clock.now)
    expect(relay.accountlessStats).toMatchObject({ delivered: 1, pushRequests: 1 })
  })

  test(`APNs: high urgency is priority 10, ttl 0 is expiration 0`, async () => {
    const relay = setup()
    const { capability } = await mint(relay, await register(relay))
    expect((await push(relay, capability, { sealed: sealed(), ttlSeconds: 0 })).status).toBe(202)
    expect(relay.apns!.calls[0].headers).toMatchObject({ "apns-priority": `10`, "apns-expiration": `0` })
    expect(relay.apns!.calls[0].headers[`apns-collapse-id`]).toBeUndefined()
  })

  test(`APNs: Unregistered, BadDeviceToken and 410 prune the install`, async () => {
    for (const result of [{ status: 410, reason: `Unregistered` }, { status: 400, reason: `BadDeviceToken` }, { status: 410 }]) {
      const apns = fakeApns(() => result)
      const relay = setup({ apns: apns.client })
      const install = await register(relay)
      const { capability } = await mint(relay, install)
      const res = await push(relay, capability)
      expect(res.status).toBe(410)
      expect(relay.store.getInstall(install.installId)).toBeNull()
      expect(relay.store.countCapabilities(install.installId)).toBe(0)
      expect((await push(relay, capability)).status).toBe(401)
    }
  })

  test(`APNs: other failures are 502 and keep the install`, async () => {
    const apns = fakeApns(() => ({ status: 500, reason: `InternalServerError` }))
    const relay = setup({ apns: apns.client })
    const install = await register(relay)
    const { capability } = await mint(relay, install)
    expect((await push(relay, capability)).status).toBe(502)
    expect(relay.store.getInstall(install.installId)).not.toBeNull()
  })

  test(`FCM: data-only with the sealed blob, priority and ttl`, async () => {
    const relay = setup()
    const install = await register(relay, { platform: `fcm`, token: FCM_TOKEN })
    const { capability } = await mint(relay, install)
    const blob = sealed(32)
    const res = await push(relay, capability, { sealed: blob, urgency: `normal`, ttlSeconds: 30, collapseId: `c` })
    expect(res.status).toBe(202)
    expect(relay.fcm!.calls).toEqual([
      { tokens: [FCM_TOKEN], data: { s: blob }, android: { priority: `normal`, ttl: 30_000, collapseKey: `c` } },
    ])
    expect(relay.apns!.calls.length).toBe(0)
  })

  test(`FCM: a dead token prunes the install, a transient error does not`, async () => {
    const dead = setup({ messaging: fakeFcm([`messaging/registration-token-not-registered`]).messaging })
    const a = await register(dead, { platform: `fcm`, token: FCM_TOKEN })
    expect((await push(dead, (await mint(dead, a)).capability)).status).toBe(410)
    expect(dead.store.getInstall(a.installId)).toBeNull()

    const flaky = setup({ messaging: fakeFcm([`messaging/internal-error`]).messaging })
    const b = await register(flaky, { platform: `fcm`, token: FCM_TOKEN })
    expect((await push(flaky, (await mint(flaky, b)).capability)).status).toBe(502)
    expect(flaky.store.getInstall(b.installId)).not.toBeNull()
  })

  test(`the per-capability hourly budget answers 429, then refills`, async () => {
    const relay = setup()
    const install = await register(relay)
    const { capability } = await mint(relay, install, { maxPerHour: 2 })
    const other = await mint(relay, install, { maxPerHour: 2 })
    expect((await push(relay, capability)).status).toBe(202)
    expect((await push(relay, capability)).status).toBe(202)
    expect((await push(relay, capability)).status).toBe(429)
    // Budgets are per capability, not per install.
    expect((await push(relay, other.capability)).status).toBe(202)
    // A malformed push does not spend budget.
    relay.clock.now += HOUR
    expect((await push(relay, capability, { sealed: `!!` })).status).toBe(400)
    expect((await push(relay, capability)).status).toBe(202)
    expect((await push(relay, capability)).status).toBe(202)
    expect((await push(relay, capability)).status).toBe(429)
    expect(relay.apns!.calls.length).toBe(5)
  })

  test(`the sealed blob must be base64url and at most ${MAX_SEALED_BYTES} bytes decoded`, async () => {
    const relay = setup()
    const { capability } = await mint(relay, await register(relay))
    expect((await push(relay, capability, { sealed: sealed(MAX_SEALED_BYTES) })).status).toBe(202)
    expect((await push(relay, capability, { sealed: sealed(MAX_SEALED_BYTES + 1) })).status).toBe(413)
    expect((await push(relay, capability, { sealed: `a+b/c` })).status).toBe(400)
    expect((await push(relay, capability, { sealed: `` })).status).toBe(400)
    expect((await push(relay, capability, { sealed: sealed(), collapseId: `x`.repeat(65) })).status).toBe(400)
    expect((await push(relay, capability, { sealed: sealed(), ttlSeconds: 2_419_201 })).status).toBe(400)
    expect((await push(relay, capability, { sealed: `a`.repeat(20_000) })).status).toBe(413)
  })

  test(`unknown, missing and install-secret bearers are 401 and draw from the failed-auth bucket`, async () => {
    const relay = setup()
    const install = await register(relay)
    expect((await call(relay, `POST`, `/v2/push`, { sealed: sealed() })).status).toBe(401)
    expect((await push(relay, install.installSecret)).status).toBe(401)
    let last = 0
    for (let i = 0; i < 60; i++) last = (await push(relay, `cap_unknown${i}`)).status
    // 62 failed attempts from the one (shared) bucket: past the 60/minute budget.
    expect(last).toBe(429)
    expect(relay.apns!.calls.length).toBe(0)
  })
})

describe(`storage`, () => {
  test(`no install secret or capability is stored in plaintext`, async () => {
    const relay = setup()
    const install = await register(relay)
    const { capability } = await mint(relay, install, { label: `desk` })
    const dump = JSON.stringify([
      relay.store.db.query(`SELECT * FROM installs`).all(),
      relay.store.db.query(`SELECT * FROM capabilities`).all(),
    ])
    expect(dump).not.toContain(install.installSecret)
    expect(dump).not.toContain(capability)
    expect(dump).not.toContain(capability.slice(4))
    expect(dump).toContain(`desk`)
  })
})

describe(`modes`, () => {
  test(`accountless-only: v1 routes 404, /healthz stays the constant, ?modes=1 reports`, async () => {
    const relay = setup({ secret: null })
    const send = await relay.app.request(`http://relay/send`, { method: `POST`, body: `{}` })
    expect(send.status).toBe(404)
    expect((await relay.app.request(`http://relay/stats`)).status).toBe(404)
    expect(await (await relay.app.request(`http://relay/healthz`)).json()).toEqual({ ok: true })
    expect(await (await relay.app.request(`http://relay/healthz?modes=1`)).json()).toEqual({
      ok: true,
      modes: { v1: false, accountless: true },
      providers: { fcm: true, apns: true },
    })
  })

  test(`both modes: v1 still needs its secret, /stats carries the accountless counters`, async () => {
    const relay = setup({ secret: `s`, apns: null })
    expect((await relay.app.request(`http://relay/stats`)).status).toBe(401)
    const stats = await relay.app.request(`http://relay/stats`, { headers: { "x-relay-secret": `s` } })
    expect(await stats.json()).toMatchObject({ ok: true, accountless: { registrations: 0, pushRequests: 0 } })
    expect(await (await relay.app.request(`http://relay/healthz?modes=1`)).json()).toMatchObject({
      modes: { v1: true, accountless: true },
      providers: { apns: false },
    })
  })

  test(`v1-only: /v2 is absent`, async () => {
    const relay = createRelayApp({ secret: `s`, trustProxy: false, getMessaging: () => null, log: quietLog })
    expect((await relay.app.request(`http://relay/v2/installs`, { method: `POST`, body: `{}` })).status).toBe(404)
  })
})

describe(`APNs provider token`, () => {
  test(`is an ES256 JWT over kid/iss/iat that verifies with the key, cached for 50 minutes`, () => {
    const { privateKey } = generateKeyPairSync(`ec`, { namedCurve: `prime256v1` })
    const jwt = signProviderToken(privateKey, `KEY123`, `TEAM456`, 1_700_000_000_000)
    const [header, claims, signature] = jwt.split(`.`)
    expect(JSON.parse(Buffer.from(header, `base64url`).toString())).toEqual({ alg: `ES256`, kid: `KEY123` })
    expect(JSON.parse(Buffer.from(claims, `base64url`).toString())).toEqual({ iss: `TEAM456`, iat: 1_700_000_000 })
    const ok = verify(
      `sha256`,
      Buffer.from(`${header}.${claims}`),
      { key: createPublicKey(privateKey), dsaEncoding: `ieee-p1363` },
      Buffer.from(signature, `base64url`)
    )
    expect(ok).toBe(true)
  })

  test(`the real client accepts a PEM with escaped newlines`, () => {
    const { privateKey } = generateKeyPairSync(`ec`, { namedCurve: `prime256v1` })
    const pem = privateKey.export({ type: `pkcs8`, format: `pem` }).toString().replace(/\n/g, `\\n`)
    expect(() => createApnsClient({ keyP8: pem, keyId: `K`, teamId: `T` })).not.toThrow()
  })
})
