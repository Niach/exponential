import { createPrivateKey, sign, type KeyObject } from "node:crypto"
import { connect, constants, type ClientHttp2Session } from "node:http2"
import type { ApnsEnvironment } from "./store"

// The push API talks to APNs directly (EXP-1428): FCM's APNs bridge
// would need a Firebase-registered iOS app per bundle id, and spezl has no
// Firebase project. The HTTP surface depends on the `ApnsClient` interface
// only, so the tests drive it with a fake; this file is the real one.

export interface ApnsRequest {
  /** The hex device token. */
  token: string
  environment: ApnsEnvironment
  /** apns-topic, apns-push-type, apns-priority, … (the client adds authorization). */
  headers: Record<string, string>
  /** The JSON payload, already serialized. */
  payload: string
}

export interface ApnsResult {
  status: number
  /** APNs' `reason` on a non-200 (BadDeviceToken, Unregistered, …). */
  reason?: string
}

export interface ApnsClient {
  send(request: ApnsRequest): Promise<ApnsResult>
}

export interface ApnsCredentials {
  /** `APNS_KEY_P8`: the .p8 auth key's PEM contents. */
  keyP8: string
  /** `APNS_KEY_ID`: the key's 10-character id. */
  keyId: string
  /** `APNS_TEAM_ID`: the Apple developer team id. */
  teamId: string
}

const HOSTS: Record<ApnsEnvironment, string> = {
  production: `https://api.push.apple.com`,
  sandbox: `https://api.sandbox.push.apple.com`,
}

// Apple rejects provider tokens older than an hour and throttles ones
// refreshed more often than every 20 minutes (TooManyProviderTokenUpdates);
// 50 minutes sits safely between.
const JWT_TTL_MS = 50 * 60 * 1000
const REQUEST_TIMEOUT_MS = 8_000

function base64url(input: Buffer | string): string {
  return Buffer.from(input).toString(`base64url`)
}

/** An ES256 provider token (exported for the test that checks its shape). */
export function signProviderToken(key: KeyObject, keyId: string, teamId: string, nowMs: number): string {
  const header = base64url(JSON.stringify({ alg: `ES256`, kid: keyId }))
  const claims = base64url(JSON.stringify({ iss: teamId, iat: Math.floor(nowMs / 1000) }))
  const input = `${header}.${claims}`
  // JWS wants the raw r||s signature, not node's default DER encoding.
  const signature = sign(`sha256`, Buffer.from(input), { key, dsaEncoding: `ieee-p1363` })
  return `${input}.${base64url(signature)}`
}

export function createApnsClient(credentials: ApnsCredentials, now: () => number = Date.now): ApnsClient {
  // Env files often carry the PEM with literal "\n" escapes on one line.
  const key = createPrivateKey(credentials.keyP8.replace(/\\n/g, `\n`))
  let cachedJwt: { token: string; issuedAt: number } | null = null

  function providerToken(): string {
    const current = now()
    if (!cachedJwt || current - cachedJwt.issuedAt >= JWT_TTL_MS) {
      cachedJwt = { token: signProviderToken(key, credentials.keyId, credentials.teamId, current), issuedAt: current }
    }
    return cachedJwt.token
  }

  // One long-lived HTTP/2 session per environment, as Apple asks: opening a
  // connection per push is what gets providers throttled. A session that
  // closes or errors is dropped and the next push opens a fresh one.
  const sessions = new Map<ApnsEnvironment, ClientHttp2Session>()

  function session(environment: ApnsEnvironment): ClientHttp2Session {
    const existing = sessions.get(environment)
    if (existing && !existing.closed && !existing.destroyed) return existing
    const created = connect(HOSTS[environment])
    const drop = () => {
      if (sessions.get(environment) === created) sessions.delete(environment)
    }
    created.on(`close`, drop)
    created.on(`error`, drop)
    created.on(`goaway`, drop)
    // An idle session must not keep the process alive on shutdown.
    created.unref()
    sessions.set(environment, created)
    return created
  }

  return {
    send(request) {
      return new Promise<ApnsResult>((resolve, reject) => {
        let stream: ReturnType<ClientHttp2Session[`request`]>
        try {
          stream = session(request.environment).request({
            [constants.HTTP2_HEADER_METHOD]: `POST`,
            [constants.HTTP2_HEADER_PATH]: `/3/device/${request.token}`,
            authorization: `bearer ${providerToken()}`,
            "content-type": `application/json`,
            ...request.headers,
          })
        } catch (err) {
          reject(err)
          return
        }
        let status = 0
        const chunks: Buffer[] = []
        stream.setTimeout(REQUEST_TIMEOUT_MS, () => {
          stream.close(constants.NGHTTP2_CANCEL)
          reject(new Error(`APNs request timed out after ${REQUEST_TIMEOUT_MS}ms`))
        })
        stream.on(`response`, (headers) => {
          status = Number(headers[constants.HTTP2_HEADER_STATUS] ?? 0)
        })
        stream.on(`data`, (chunk: Buffer) => chunks.push(chunk))
        stream.on(`error`, reject)
        stream.on(`end`, () => {
          let reason: string | undefined
          if (chunks.length > 0) {
            try {
              reason = (JSON.parse(Buffer.concat(chunks).toString(`utf8`)) as { reason?: string }).reason
            } catch {
              reason = undefined
            }
          }
          resolve({ status, reason })
        })
        stream.end(request.payload)
      })
    },
  }
}
