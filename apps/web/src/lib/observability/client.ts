// #LOGS-2: the client half's entry, tiny and Sentry-free. The root route
// calls `startClientObservability(router)` once; it asks the server for the
// web config, and only when diagnostics are enabled AND this device's "Send
// diagnostics" is on does it load ./client-sdk (the @sentry/react chunk).
// Errors caught before that (the router's error boundary, early window
// errors) are buffered and handed over once it loads; switched off, the SDK
// is closed and the buffer dropped.

import type { MonitoringConfig } from "./config"
import { readSendDiagnostics, subscribeSendDiagnostics } from "./preference"

type Sdk = typeof import("./client-sdk")
type Enabled = Extract<MonitoringConfig, { enabled: true }>

const MAX_BUFFERED = 20

let started = false
let config: Enabled | null = null
let sdk: Sdk | null = null
let running = false
let userId: string | null = null
let buffered: unknown[] = []
let detachEarly: (() => void) | null = null

/** Fetch the web config (`{ enabled: false }` on any failure). */
async function fetchConfig(): Promise<MonitoringConfig> {
  try {
    const res = await fetch(`/api/monitoring/config?platform=web`)
    if (!res.ok) return { enabled: false }
    return (await res.json()) as MonitoringConfig
  } catch {
    return { enabled: false }
  }
}

function buffer(error: unknown) {
  if (buffered.length < MAX_BUFFERED) buffered.push(error)
}

function attachEarlyHandlers() {
  if (typeof window === `undefined` || detachEarly) return
  const onError = (e: ErrorEvent) => buffer(e.error ?? e.message)
  const onRejection = (e: PromiseRejectionEvent) => buffer(e.reason)
  window.addEventListener(`error`, onError)
  window.addEventListener(`unhandledrejection`, onRejection)
  detachEarly = () => {
    window.removeEventListener(`error`, onError)
    window.removeEventListener(`unhandledrejection`, onRejection)
    detachEarly = null
  }
}

async function apply(router: unknown) {
  const want = config !== null && readSendDiagnostics()
  if (want && !running) {
    sdk ??= await import(`./client-sdk`)
    if (running || !config || !readSendDiagnostics()) return
    sdk.startSdk(router, config)
    running = true
    detachEarly?.()
    sdk.setSdkUser(userId)
    for (const error of buffered) sdk.captureError(error)
    buffered = []
  } else if (!want) {
    detachEarly?.()
    buffered = []
    if (running && sdk) {
      running = false
      await sdk.stopSdk()
    }
  }
}

export function startClientObservability(router: unknown): void {
  if (started || typeof window === `undefined`) return
  started = true
  if (readSendDiagnostics()) attachEarlyHandlers()
  void fetchConfig().then((c) => {
    config = c.enabled ? c : null
    subscribeSendDiagnostics(() => void apply(router))
    return apply(router)
  })
}

/** The router's error boundary (`defaultOnCatch`) reports through here. */
export function captureClientError(error: unknown): void {
  if (running && sdk) sdk.captureError(error)
  else if (started && (config !== null || detachEarly) && readSendDiagnostics()) {
    buffer(error)
  }
}

/** The signed-in user's id only (no email, no name), or null when signed out. */
export function setObservedUser(id: string | null): void {
  userId = id
  if (running && sdk) sdk.setSdkUser(id)
}
