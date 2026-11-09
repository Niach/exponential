// #LOGS-2: the web CLIENT's Sentry SDK (@sentry/react). Loaded ONLY through
// the dynamic import in ./client.ts, after the config says diagnostics are on
// and the device's "Send diagnostics" is on, so it never weighs on the
// initial bundle. Named imports keep the chunk tree-shaken: no session
// replay, no feedback widget, no release-health sessions.
//
// Envelopes go to our own tunnel (`/api/monitoring`), never to GlitchTip.
// Nothing here writes a cookie or localStorage.

import {
  captureException,
  close,
  consoleLoggingIntegration,
  init,
  setUser,
  tanstackRouterBrowserTracingIntegration,
} from "@sentry/react"
import type { MonitoringConfig } from "./config"
import { WEB_RELEASE } from "./release"
import { DATA_COLLECTION, scrubBreadcrumb, scrubEvent, scrubLog } from "./scrub"

type Enabled = Extract<MonitoringConfig, { enabled: true }>

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)
}

/**
 * Trace headers go to our own origin only: relative URLs and absolute ones
 * on `origin` (never a third party, whatever it is).
 */
export function ownOriginTargets(origin: string): RegExp[] {
  return [/^\/(?!\/)/, new RegExp(`^${escapeRegExp(origin)}(?:[/?#]|$)`)]
}

/**
 * A live Electric long-poll (`/api/shapes/…?live=true`) parks for ~20 s by
 * design: as a span it would hold every pageload/navigation open until the
 * final timeout. Snapshot fetches (the ones worth timing) stay traced.
 */
export function shouldTraceRequest(url: string): boolean {
  if (!url.includes(`/api/shapes/`)) return true
  return !/[?&]live=true(?:&|$)/.test(url)
}

export function startSdk(router: unknown, config: Enabled): void {
  init({
    dsn: config.dsn,
    tunnel: `/api/monitoring`,
    release: WEB_RELEASE,
    environment: config.environment,
    sampleRate: 1,
    tracesSampleRate: config.tracesSampleRate,
    dataCollection: DATA_COLLECTION,
    // Transactions, not v11's default span streaming: GlitchTip ingests
    // transaction envelopes, and beforeSendTransaction (the scrubber) only
    // runs on them.
    traceLifecycle: `static`,
    tracePropagationTargets: ownOriginTargets(window.location.origin),
    integrations: (defaults) => [
      ...defaults.filter((i) => i.name !== `BrowserSession`),
      tanstackRouterBrowserTracingIntegration(router, {
        shouldCreateSpanForRequest: shouldTraceRequest,
      }),
      consoleLoggingIntegration({ levels: [`warn`, `error`] }),
    ],
    beforeSend: (event) => scrubEvent(event),
    beforeSendTransaction: (event) => scrubEvent(event),
    beforeSendLog: (log) => scrubLog(log),
    beforeBreadcrumb: (crumb) => scrubBreadcrumb(crumb),
  })
}

export async function stopSdk(): Promise<void> {
  await close(2000)
}

export function captureError(error: unknown): void {
  captureException(error)
}

export function setSdkUser(id: string | null): void {
  setUser(id ? { id } : null)
}
