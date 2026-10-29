import { randomBytes, timingSafeEqual } from "node:crypto"
import type { Context, Hono } from "hono"
import type { MulticastMessage } from "firebase-admin/messaging"
import { z } from "zod"
import type { ApnsClient } from "./apns"
import type { Logger } from "./log"
import {
  DEAD_CODES,
  DeadlineError,
  createIpLimiter,
  readBodyCapped,
  withDeadline,
  type RelayMessaging,
} from "./shared"
import { hashSecret, type ApnsEnvironment, type InstallRow, type Platform, type RelayStore } from "./store"

// The accountless mode (EXP-1428), mounted under /v2 next to the v1 /send.
//
// v1 trusts ONE caller (the web app, holding PUSH_RELAY_SECRET) with raw
// device tokens. spezl has no server and no accounts, so there is no such
// caller: a device registers itself here and gets back an install secret (to
// manage the install) and mints CAPABILITIES — opaque bearer tokens it hands
// to a peer, so the peer can wake it without ever learning the device token.
// The payload is sealed end to end for the recipient install; the relay only
// checks that it is base64url and small enough, and forwards it.

export interface AccountlessOptions {
  store: RelayStore
  /** The APNs client, or null while APNS_KEY_P8/KEY_ID/TEAM_ID are not set. */
  apns: ApnsClient | null
  /** `APNS_TOPICS`: the bundle ids an install may register for. */
  apnsTopics: string[]
  /** `PUSH_PLACEHOLDER_TITLE`: the visible alert the Notification Service Extension replaces. */
  placeholderTitle?: string
  /** `PUSH_PLACEHOLDER_BODY`: optional second line of that placeholder. */
  placeholderBody?: string
  getMessaging: () => RelayMessaging | null
  now: () => number
  log: Logger
  fcmDeadlineMs: number
  clientIp: (headers: Headers, fallback?: string) => string
  /** The relay's shared failed-auth bucket: false once the IP is past its budget. */
  failedAuthHit: (ip: string) => boolean
  /** Registrations per IP per window (default 20 per hour). */
  registrationLimit?: { max: number; windowMs: number }
}

export interface AccountlessStats {
  registrations: number
  pushRequests: number
  delivered: number
  deadInstalls: number
  rateLimited: number
  authFailures: number
  providerErrors: number
}

// ── Limits ────────────────────────────────────────────────────────────────────

/** The decoded sealed payload: APNs caps a whole payload at 4 KB, the envelope needs the rest. */
export const MAX_SEALED_BYTES = 3072
/** Every /v2 body is small; a sealed blob at the cap is ~4.1 KB of base64url. */
const MAX_V2_BODY_BYTES = 16 * 1024
const MAX_CAPABILITIES_PER_INSTALL = 256
const DEFAULT_MAX_PER_HOUR = 60
const MAX_MAX_PER_HOUR = 600
const MAX_TTL_SECONDS = 2_419_200 // 28 days, FCM's ceiling (APNs keeps ~30)
const DEFAULT_REGISTRATION_LIMIT = { max: 20, windowMs: 60 * 60 * 1000 }
const SWEEP_INTERVAL_MS = 60 * 60 * 1000

export const CAPABILITY_PREFIX = `cap_`

// ── Validation ────────────────────────────────────────────────────────────────

// The device token goes into the APNs request PATH, so its shape is checked
// strictly: hex for APNs, FCM's own alphabet for FCM.
const APNS_TOKEN_RE = /^[0-9a-fA-F]{64,200}$/
const FCM_TOKEN_RE = /^[A-Za-z0-9_:.-]{20,4096}$/
const BASE64URL_RE = /^[A-Za-z0-9_-]+={0,2}$/

const registerSchema = z.object({
  platform: z.enum([`apns`, `fcm`]),
  token: z.string(),
  topic: z.string().min(1).max(255).optional(),
  environment: z.enum([`production`, `sandbox`]).optional(),
})

const rotateSchema = z.object({
  token: z.string(),
  environment: z.enum([`production`, `sandbox`]).optional(),
})

const capabilitySchema = z.object({
  label: z.string().max(64).optional(),
  expiresAt: z.iso.datetime({ offset: true }).optional(),
  maxPerHour: z.number().int().min(1).max(MAX_MAX_PER_HOUR).optional(),
})

const pushSchema = z.object({
  sealed: z.string().min(1),
  collapseId: z.string().min(1).max(64).optional(),
  urgency: z.enum([`high`, `normal`]).optional(),
  ttlSeconds: z.number().int().min(0).max(MAX_TTL_SECONDS).optional(),
})

function tokenValid(platform: Platform, token: string): boolean {
  return (platform === `apns` ? APNS_TOKEN_RE : FCM_TOKEN_RE).test(token)
}

function randomId(prefix: string, bytes: number): string {
  return `${prefix}${randomBytes(bytes).toString(`base64url`)}`
}

function bearer(c: Context): string | null {
  const header = c.req.raw.headers.get(`authorization`)
  const match = header?.match(/^Bearer\s+(\S+)$/i)
  return match?.[1] ?? null
}

function hashesEqual(a: string, b: string): boolean {
  const left = Buffer.from(a)
  const right = Buffer.from(b)
  return left.length === right.length && timingSafeEqual(left, right)
}

// ── Mount ─────────────────────────────────────────────────────────────────────

export function mountAccountless(app: Hono, options: AccountlessOptions) {
  const {
    store,
    apns,
    apnsTopics,
    placeholderTitle = `New message`,
    placeholderBody,
    getMessaging,
    now,
    log,
    fcmDeadlineMs,
    clientIp,
    failedAuthHit,
  } = options
  const registrations = createIpLimiter(
    (options.registrationLimit ?? DEFAULT_REGISTRATION_LIMIT).max,
    (options.registrationLimit ?? DEFAULT_REGISTRATION_LIMIT).windowMs,
    now
  )

  const stats: AccountlessStats = {
    registrations: 0,
    pushRequests: 0,
    delivered: 0,
    deadInstalls: 0,
    rateLimited: 0,
    authFailures: 0,
    providerErrors: 0,
  }

  setInterval(() => {
    try {
      store.deleteExpiredCapabilities(now())
    } catch (err) {
      log.error(`expired-capability sweep failed`, err)
    }
  }, SWEEP_INTERVAL_MS).unref?.()

  // The socket address when Bun hands us its server as `env`: unlike v1 (one
  // trusted caller behind one egress IP), v2 callers are the whole internet,
  // so without TRUST_PROXY the real peer is a better bucket key than one
  // shared bucket. Behind an untrusted proxy it degrades to the proxy's IP,
  // i.e. the same shared bucket as v1.
  function ipOf(c: Context): string {
    const server = c.env as { requestIP?: (req: Request) => { address: string } | null } | undefined
    const socket = server?.requestIP?.(c.req.raw)?.address
    return clientIp(c.req.raw.headers, socket ?? `unknown`)
  }

  function unauthorized(c: Context): Response {
    stats.authFailures += 1
    if (!failedAuthHit(ipOf(c))) return c.json({ error: `Rate limit exceeded` }, 429)
    return c.json({ error: `Unauthorized` }, 401)
  }

  async function readJson(c: Context): Promise<{ body: unknown } | Response> {
    const contentLength = Number.parseInt(c.req.raw.headers.get(`content-length`) ?? `0`, 10)
    if (contentLength > MAX_V2_BODY_BYTES) return c.json({ error: `Body too large` }, 413)
    const raw = await readBodyCapped(c.req.raw.body, MAX_V2_BODY_BYTES)
    if (raw === null) return c.json({ error: `Body too large` }, 413)
    if (raw.trim() === ``) return { body: {} }
    try {
      return { body: JSON.parse(raw) as unknown }
    } catch {
      return c.json({ error: `Bad request` }, 400)
    }
  }

  /** The install the bearer install secret manages, or the 401/429 to answer. */
  function authInstall(c: Context): InstallRow | Response {
    const secret = bearer(c)
    const install = store.getInstall(c.req.param(`id`) ?? ``)
    if (!secret || !install || !hashesEqual(hashSecret(secret), install.secretHash)) return unauthorized(c)
    return install
  }

  function providerReady(platform: Platform): boolean {
    return platform === `apns` ? apns !== null : getMessaging() !== null
  }

  /** Validates the APNs-only fields; returns the topic/environment to store or an error message. */
  function apnsFields(
    platform: Platform,
    topic: string | undefined,
    environment: ApnsEnvironment | undefined
  ): { topic: string | null; environment: ApnsEnvironment | null } | string {
    if (platform === `fcm`) {
      if (topic !== undefined || environment !== undefined) return `topic and environment are APNs-only`
      return { topic: null, environment: null }
    }
    // One allowed bundle id makes the topic implicit; with several the
    // install must say which app it is.
    const resolved = topic ?? (apnsTopics.length === 1 ? apnsTopics[0] : undefined)
    if (!resolved || !apnsTopics.includes(resolved)) return `topic is not allowed`
    return { topic: resolved, environment: environment ?? `production` }
  }

  // ── Installs ────────────────────────────────────────────────────────────────

  app.post(`/v2/installs`, async (c) => {
    if (!registrations.hit(ipOf(c))) {
      stats.rateLimited += 1
      return c.json({ error: `Rate limit exceeded` }, 429)
    }
    const read = await readJson(c)
    if (read instanceof Response) return read
    const parsed = registerSchema.safeParse(read.body)
    if (!parsed.success) return c.json({ error: `Bad request`, issues: parsed.error.issues }, 400)
    const { platform, token, topic, environment } = parsed.data
    if (!tokenValid(platform, token)) return c.json({ error: `Bad request`, field: `token` }, 400)
    const fields = apnsFields(platform, topic, environment)
    if (typeof fields === `string`) return c.json({ error: `Bad request`, message: fields }, 400)
    if (!providerReady(platform)) return c.json({ error: `${platform === `apns` ? `APNs` : `FCM`} not configured` }, 503)

    const installId = randomId(`ins_`, 16)
    const installSecret = randomBytes(32).toString(`base64url`)
    const current = now()
    store.insertInstall({
      id: installId,
      secretHash: hashSecret(installSecret),
      platform,
      token,
      topic: fields.topic,
      environment: fields.environment,
      createdAt: current,
      updatedAt: current,
    })
    stats.registrations += 1
    return c.json({ installId, installSecret }, 201)
  })

  // Rotates the device token (APNs/FCM hand out a new one after a restore,
  // a reinstall of the OS, …). The install id, its secret and every
  // capability it minted stay valid.
  app.put(`/v2/installs/:id`, async (c) => {
    const install = authInstall(c)
    if (install instanceof Response) return install
    const read = await readJson(c)
    if (read instanceof Response) return read
    const parsed = rotateSchema.safeParse(read.body)
    if (!parsed.success) return c.json({ error: `Bad request`, issues: parsed.error.issues }, 400)
    const { token, environment } = parsed.data
    if (!tokenValid(install.platform, token)) return c.json({ error: `Bad request`, field: `token` }, 400)
    if (install.platform === `fcm` && environment !== undefined) {
      return c.json({ error: `Bad request`, message: `environment is APNs-only` }, 400)
    }
    store.updateInstallToken(install.id, token, environment ?? install.environment, now())
    return c.json({ installId: install.id })
  })

  app.delete(`/v2/installs/:id`, (c) => {
    const install = authInstall(c)
    if (install instanceof Response) return install
    store.deleteInstall(install.id)
    return c.body(null, 204)
  })

  // ── Capabilities ────────────────────────────────────────────────────────────

  app.post(`/v2/installs/:id/capabilities`, async (c) => {
    const install = authInstall(c)
    if (install instanceof Response) return install
    const read = await readJson(c)
    if (read instanceof Response) return read
    const parsed = capabilitySchema.safeParse(read.body)
    if (!parsed.success) return c.json({ error: `Bad request`, issues: parsed.error.issues }, 400)
    const current = now()
    const expiresAt = parsed.data.expiresAt ? Date.parse(parsed.data.expiresAt) : null
    if (expiresAt !== null && expiresAt <= current) {
      return c.json({ error: `Bad request`, message: `expiresAt is in the past` }, 400)
    }
    if (store.countCapabilities(install.id) >= MAX_CAPABILITIES_PER_INSTALL) {
      return c.json({ error: `Too many capabilities` }, 409)
    }
    const capabilityId = randomId(`cid_`, 12)
    const capability = `${CAPABILITY_PREFIX}${randomBytes(32).toString(`base64url`)}`
    store.insertCapability({
      id: capabilityId,
      installId: install.id,
      tokenHash: hashSecret(capability),
      label: parsed.data.label ?? null,
      expiresAt,
      maxPerHour: parsed.data.maxPerHour ?? DEFAULT_MAX_PER_HOUR,
      windowStart: 0,
      windowCount: 0,
      lastUsedAt: null,
      createdAt: current,
    })
    return c.json({ capabilityId, capability }, 201)
  })

  // The capability token itself is never readable again: only its hash is
  // stored, so a lost one is revoked and re-minted.
  app.get(`/v2/installs/:id/capabilities`, (c) => {
    const install = authInstall(c)
    if (install instanceof Response) return install
    const iso = (ms: number | null) => (ms === null ? null : new Date(ms).toISOString())
    return c.json({
      capabilities: store.listCapabilities(install.id).map((row) => ({
        capabilityId: row.id,
        label: row.label,
        createdAt: iso(row.createdAt),
        expiresAt: iso(row.expiresAt),
        lastUsedAt: iso(row.lastUsedAt),
        maxPerHour: row.maxPerHour,
      })),
    })
  })

  app.delete(`/v2/installs/:id/capabilities/:capId`, (c) => {
    const install = authInstall(c)
    if (install instanceof Response) return install
    if (!store.deleteCapability(install.id, c.req.param(`capId`))) return c.json({ error: `Not found` }, 404)
    return c.body(null, 204)
  })

  // ── Push ────────────────────────────────────────────────────────────────────

  function dead(c: Context, install: InstallRow): Response {
    // A dead device token can never come back to life under this install:
    // the app re-registers (or rotates) when it gets a new one. Deleting the
    // install also revokes every capability pointing at it.
    store.deleteInstall(install.id)
    stats.deadInstalls += 1
    log.info(`pruned dead ${install.platform} install ${install.id}`)
    return c.json({ error: `Install gone` }, 410)
  }

  function providerError(c: Context, install: InstallRow, detail: string, status: 502 | 504 = 502): Response {
    stats.providerErrors += 1
    log.error(`${install.platform} push to install ${install.id} failed: ${detail}`)
    return c.json({ error: status === 504 ? `Provider timeout` : `Provider error` }, status)
  }

  app.post(`/v2/push`, async (c) => {
    stats.pushRequests += 1
    const token = bearer(c)
    if (!token?.startsWith(CAPABILITY_PREFIX)) return unauthorized(c)
    const capability = store.findCapability(hashSecret(token))
    const current = now()
    if (!capability || (capability.expiresAt !== null && capability.expiresAt <= current)) return unauthorized(c)
    const install = store.getInstall(capability.installId)
    if (!install) return c.json({ error: `Install gone` }, 410)

    const read = await readJson(c)
    if (read instanceof Response) return read
    const parsed = pushSchema.safeParse(read.body)
    if (!parsed.success) return c.json({ error: `Bad request`, issues: parsed.error.issues }, 400)
    const { sealed, collapseId, urgency = `high`, ttlSeconds } = parsed.data
    if (!BASE64URL_RE.test(sealed)) return c.json({ error: `Bad request`, field: `sealed` }, 400)
    if (Buffer.from(sealed, `base64url`).length > MAX_SEALED_BYTES) {
      return c.json({ error: `Sealed payload too large` }, 413)
    }

    // The budget is spent only by a well-formed push, so a peer's bug does
    // not lock it out for the hour.
    if (!store.consumeCapability(capability, current)) {
      stats.rateLimited += 1
      return c.json({ error: `Rate limit exceeded` }, 429)
    }

    if (install.platform === `apns`) {
      if (!apns) return c.json({ error: `APNs not configured` }, 503)
      const headers: Record<string, string> = {
        "apns-topic": install.topic ?? ``,
        "apns-push-type": `alert`,
        "apns-priority": urgency === `high` ? `10` : `5`,
      }
      if (collapseId !== undefined) headers[`apns-collapse-id`] = collapseId
      // 0 = deliver now or never; unset = APNs' own store-and-forward default.
      if (ttlSeconds !== undefined) {
        headers[`apns-expiration`] = ttlSeconds === 0 ? `0` : String(Math.floor(current / 1000) + ttlSeconds)
      }
      // The visible alert is a fixed placeholder: the Notification Service
      // Extension opens `s` on the device and replaces title and body, so the
      // relay (and Apple) never see the real text.
      const payload = JSON.stringify({
        aps: {
          alert: { title: placeholderTitle, ...(placeholderBody ? { body: placeholderBody } : {}) },
          "mutable-content": 1,
        },
        s: sealed,
      })
      try {
        const result = await withDeadline(
          apns.send({ token: install.token, environment: install.environment ?? `production`, headers, payload }),
          fcmDeadlineMs
        )
        if (result.status === 200) {
          stats.delivered += 1
          return c.json({ delivered: true }, 202)
        }
        if (result.status === 410 || result.reason === `BadDeviceToken` || result.reason === `Unregistered`) {
          return dead(c, install)
        }
        return providerError(c, install, `status ${result.status} ${result.reason ?? ``}`.trim())
      } catch (err) {
        if (err instanceof DeadlineError) return providerError(c, install, `deadline exceeded`, 504)
        return providerError(c, install, String(err))
      }
    }

    const messaging = getMessaging()
    if (!messaging) return c.json({ error: `FCM not configured` }, 503)
    // DATA-ONLY like v1 (EXP-264): the app's service must run to open the
    // sealed payload and build the notification itself.
    const message: MulticastMessage = {
      tokens: [install.token],
      data: { s: sealed },
      android: {
        priority: urgency,
        ...(ttlSeconds !== undefined ? { ttl: ttlSeconds * 1000 } : {}),
        ...(collapseId !== undefined ? { collapseKey: collapseId } : {}),
      },
    }
    try {
      const response = await withDeadline(messaging.sendEachForMulticast(message), fcmDeadlineMs)
      const result = response.responses[0]
      if (result?.success) {
        stats.delivered += 1
        return c.json({ delivered: true }, 202)
      }
      const code = result?.error?.code
      if (code && DEAD_CODES.has(code)) return dead(c, install)
      return providerError(c, install, `code=${code ?? `unknown`}`)
    } catch (err) {
      if (err instanceof DeadlineError) return providerError(c, install, `deadline exceeded`, 504)
      return providerError(c, install, String(err))
    }
  })

  return {
    stats,
    apnsConfigured: () => apns !== null,
  }
}
