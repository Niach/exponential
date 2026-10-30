import { describe, expect, test } from "bun:test"
import type { BatchResponse, MulticastMessage } from "firebase-admin/messaging"
import {
  DEEP_CHECK_TOKEN,
  TRANSPORT_ERROR_CODE,
  createRelayApp,
  isTransportError,
  type RelayMessaging,
} from "./app"
import { formatLogLine } from "./log"
import { RelayStore } from "./store"

// EXP-1289: the /send compat shim's one transport retry, the deep health
// check and the stats around them, driven through the real Hono app with a
// fake firebase-admin messaging client and a fake clock. The /send
// expectations are frozen: the shim must stay byte-identical until NEW-14
// removes it.

const SECRET = `test-secret`

/** firebase-admin's shape for a session failure (FirebaseMessagingSessionError). */
function sessionError(): Error & { code: string } {
  return Object.assign(
    new Error(
      `Session error while making requests: UNABLE_TO_VERIFY_LEAF_SIGNATURE - unable to verify the first certificate `
    ),
    { code: TRANSPORT_ERROR_CODE }
  )
}

function fcmError(code: string): Error & { code: string } {
  return Object.assign(new Error(code), { code })
}

function batch(codes: (string | null)[]): BatchResponse {
  return {
    successCount: codes.filter((c) => c === null).length,
    failureCount: codes.filter((c) => c !== null).length,
    responses: codes.map((code) =>
      code === null
        ? { success: true, messageId: `m` }
        : { success: false, error: { code, message: code } as never }
    ),
  }
}

type Attempt = (message: MulticastMessage, dryRun?: boolean) => Promise<BatchResponse>

/** A messaging client that answers each call from the next scripted attempt. */
function fakeMessaging(attempts: Attempt[]): RelayMessaging & { calls: { message: MulticastMessage; dryRun?: boolean }[] } {
  const calls: { message: MulticastMessage; dryRun?: boolean }[] = []
  return {
    calls,
    sendEachForMulticast(message, dryRun) {
      calls.push({ message, dryRun })
      const attempt = attempts[Math.min(calls.length - 1, attempts.length - 1)]
      return attempt(message, dryRun)
    },
  }
}

const quietLog = { info() {}, warn() {}, error() {} }

function relay(messaging: RelayMessaging | null, extra: { fcmDeadlineMs?: number; now?: () => number } = {}) {
  return createRelayApp({
    secret: SECRET,
    trustProxy: false,
    getMessaging: () => messaging,
    log: quietLog,
    api: { store: RelayStore.open(`:memory:`), apns: null, apnsTopics: [] },
    ...extra,
  })
}

function sendRequest(tokens = [`tok-1`]) {
  return new Request(`http://relay/send`, {
    method: `POST`,
    headers: { "x-relay-secret": SECRET, "content-type": `application/json` },
    body: JSON.stringify({ tokens, notification: { title: `Hi` }, data: { a: `b` } }),
  })
}

describe(`isTransportError`, () => {
  test(`matches firebase-admin's session failure by code and by message`, () => {
    expect(isTransportError(sessionError())).toBe(true)
    expect(isTransportError(new Error(`Session error while making requests: ECONNRESET`))).toBe(true)
    expect(isTransportError(fcmError(`messaging/invalid-argument`))).toBe(false)
    expect(isTransportError(new Error(`boom`))).toBe(false)
    expect(isTransportError(null)).toBe(false)
  })
})

describe(`/send transport retry`, () => {
  test(`retries ONCE on a session error and succeeds on the fresh session`, async () => {
    const messaging = fakeMessaging([
      () => Promise.reject(sessionError()),
      () => Promise.resolve(batch([null])),
    ])
    const { app, stats } = relay(messaging)
    const res = await app.request(sendRequest())
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual({ invalidTokens: [] })
    expect(messaging.calls.length).toBe(2)
    expect(messaging.calls[1].message).toEqual(messaging.calls[0].message)
    expect(stats).toMatchObject({ sendRequests: 1, sendOk: 1, sendFailed: 0, sendRetried: 1, tokensOk: 1 })
    expect(stats.lastError).toBeNull()
  })

  test(`a second session error fails the send and counts both`, async () => {
    const messaging = fakeMessaging([() => Promise.reject(sessionError())])
    const { app, stats } = relay(messaging)
    const res = await app.request(sendRequest())
    expect(res.status).toBe(500)
    expect(messaging.calls.length).toBe(2)
    expect(stats).toMatchObject({ sendOk: 0, sendFailed: 1, sendRetried: 1 })
    expect(stats.lastError).toContain(`UNABLE_TO_VERIFY_LEAF_SIGNATURE`)
  })

  test(`never retries a non-transport error`, async () => {
    const messaging = fakeMessaging([() => Promise.reject(fcmError(`messaging/invalid-argument`))])
    const { app, stats } = relay(messaging)
    const res = await app.request(sendRequest())
    expect(res.status).toBe(500)
    expect(messaging.calls.length).toBe(1)
    expect(stats).toMatchObject({ sendFailed: 1, sendRetried: 0 })
  })

  test(`does not retry when the first attempt already used half the budget`, async () => {
    let clock = 0
    const messaging = fakeMessaging([
      () => {
        clock += 4_500
        return Promise.reject(sessionError())
      },
    ])
    const { app, stats } = relay(messaging, { now: () => clock, fcmDeadlineMs: 8_000 })
    const res = await app.request(sendRequest())
    expect(res.status).toBe(500)
    expect(messaging.calls.length).toBe(1)
    expect(stats).toMatchObject({ sendFailed: 1, sendRetried: 0 })
  })

  test(`the retry runs under the budget the first attempt left`, async () => {
    let clock = 0
    const messaging = fakeMessaging([
      () => {
        clock += 100
        return Promise.reject(sessionError())
      },
      () => new Promise<BatchResponse>(() => {}), // hangs: the remaining deadline must cut it
    ])
    const { app, stats } = relay(messaging, { now: () => clock, fcmDeadlineMs: 300 })
    const started = Date.now()
    const res = await app.request(sendRequest())
    expect(res.status).toBe(504)
    expect(Date.now() - started).toBeLessThan(1_000)
    expect(messaging.calls.length).toBe(2)
    expect(stats).toMatchObject({ deadlineTimeouts: 1, sendRetried: 1, sendFailed: 1 })
  })

  test(`a clean send still prunes dead tokens and reports the rest`, async () => {
    const messaging = fakeMessaging([
      () => Promise.resolve(batch([null, `messaging/registration-token-not-registered`, `messaging/internal-error`])),
    ])
    const { app, stats } = relay(messaging)
    const res = await app.request(sendRequest([`tok-1`, `tok-2`, `tok-3`]))
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual({ invalidTokens: [`tok-2`] })
    expect(messaging.calls[0].dryRun).toBeUndefined()
    expect(stats).toMatchObject({ sendOk: 1, tokensRequested: 3, tokensOk: 1, tokensFailed: 2, invalidTokens: 1, sendRetried: 0 })
  })
})

describe(`/healthz`, () => {
  test(`stays a constant without ?deep=1 and needs the secret with it`, async () => {
    const { app } = relay(fakeMessaging([() => Promise.resolve(batch([null]))]))
    const shallow = await app.request(`http://relay/healthz`)
    expect(shallow.status).toBe(200)
    expect(await shallow.json()).toEqual({ ok: true })
    const deepNoSecret = await app.request(`http://relay/healthz?deep=1`)
    expect(deepNoSecret.status).toBe(401)
  })

  test(`deep check sends a dry run to the probe token and passes on any per-token answer`, async () => {
    const messaging = fakeMessaging([() => Promise.resolve(batch([`messaging/invalid-argument`]))])
    const { app, stats } = relay(messaging)
    const res = await app.request(`http://relay/healthz?deep=1`, { headers: { "x-relay-secret": SECRET } })
    expect(res.status).toBe(200)
    const body = (await res.json()) as { ok: boolean; deep: boolean; code: string; ms: number }
    expect(body).toMatchObject({ ok: true, deep: true, code: `messaging/invalid-argument` })
    expect(messaging.calls).toEqual([
      { message: { tokens: [DEEP_CHECK_TOKEN], data: { probe: `1` } }, dryRun: true },
    ])
    expect(stats).toMatchObject({ deepChecks: 1, deepCheckFailures: 0, lastDeepCheckOk: true, sendRequests: 0 })
    expect(stats.lastDeepCheckAt).not.toBeNull()
  })

  test(`deep check fails on a session error and when Firebase is not configured`, async () => {
    const { app, stats } = relay(fakeMessaging([() => Promise.reject(sessionError())]))
    const res = await app.request(`http://relay/healthz?deep=1`, { headers: { "x-relay-secret": SECRET } })
    expect(res.status).toBe(503)
    expect(await res.json()).toMatchObject({ ok: false, deep: true })
    expect(stats).toMatchObject({ deepChecks: 1, deepCheckFailures: 1, lastDeepCheckOk: false })

    const unconfigured = relay(null)
    const res2 = await unconfigured.app.request(`http://relay/healthz?deep=1`, { headers: { "x-relay-secret": SECRET } })
    expect(res2.status).toBe(503)
    expect(await res2.json()).toEqual({ ok: false, deep: true, error: `Firebase not configured` })
  })

  test(`/stats carries the new counters`, async () => {
    const { app } = relay(fakeMessaging([() => Promise.resolve(batch([null]))]))
    const res = await app.request(`http://relay/stats`, { headers: { "x-relay-secret": SECRET } })
    expect(res.status).toBe(200)
    const body = (await res.json()) as Record<string, unknown>
    expect(body).toMatchObject({ ok: true, firebaseConfigured: true, sendRetried: 0, deepChecks: 0, lastDeepCheckOk: null })
  })
})

describe(`log lines`, () => {
  test(`carry an ISO timestamp after the relay prefix`, () => {
    expect(formatLogLine(new Date(Date.UTC(2026, 9, 7, 1, 41, 33, 600)), `FCM multicast failed`)).toBe(
      `[push-relay] 2026-10-07T01:41:33.600Z FCM multicast failed`
    )
  })
})
