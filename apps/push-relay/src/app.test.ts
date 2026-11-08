import { describe, expect, test } from "bun:test"
import type { BatchResponse, MulticastMessage } from "firebase-admin/messaging"
import { DEEP_CHECK_TOKEN, createRelayApp, type RelayMessaging } from "./app"
import { formatLogLine } from "./log"
import { RelayStore } from "./store"

// EXP-1289: the deep health check and the stats around it, driven through
// the real Hono app with a fake firebase-admin messaging client.

const SECRET = `test-secret`

/** firebase-admin's shape for a session failure (FirebaseMessagingSessionError). */
function sessionError(): Error & { code: string } {
  return Object.assign(
    new Error(
      `Session error while making requests: UNABLE_TO_VERIFY_LEAF_SIGNATURE - unable to verify the first certificate `
    ),
    { code: `messaging/app/network-error` }
  )
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
    expect(stats).toMatchObject({ deepChecks: 1, deepCheckFailures: 0, lastDeepCheckOk: true })
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

  test(`/stats carries the deep-check counters and the push API's, no /send counters`, async () => {
    const { app } = relay(fakeMessaging([() => Promise.resolve(batch([null]))]))
    const res = await app.request(`http://relay/stats`, { headers: { "x-relay-secret": SECRET } })
    expect(res.status).toBe(200)
    const body = (await res.json()) as Record<string, unknown>
    expect(body).toMatchObject({
      ok: true,
      firebaseConfigured: true,
      deepChecks: 0,
      deepCheckFailures: 0,
      lastDeepCheckAt: null,
      lastDeepCheckOk: null,
      api: { registrations: 0, pushRequests: 0 },
    })
    for (const gone of [`sendRequests`, `sendOk`, `sendFailed`, `sendRetried`, `tokensRequested`, `invalidTokens`, `deadlineTimeouts`, `lastError`]) {
      expect(body).not.toHaveProperty(gone)
    }
  })
})

describe(`log lines`, () => {
  test(`carry an ISO timestamp after the relay prefix`, () => {
    expect(formatLogLine(new Date(Date.UTC(2026, 9, 7, 1, 41, 33, 600)), `FCM multicast failed`)).toBe(
      `[push-relay] 2026-10-07T01:41:33.600Z FCM multicast failed`
    )
  })
})
