// #LOGS-2: the web SERVER's Sentry SDK (@sentry/bun: our server runtime is
// Bun.serve via srvx, see server-bun.ts). Imported FIRST by server-bun.ts so
// Bun.serve is instrumented before srvx calls it. Off unless
// GLITCHTIP_INTERNAL_URL + SENTRY_DSN_SERVER are set; envelopes go straight to
// the internal URL (never through the public tunnel).
//
// What it covers: every request is an `http.server` span continuing the
// client's `sentry-trace`/`baggage` (bunServerIntegration), outbound fetches
// (the Electric shape proxy's upstream call among them) are child spans,
// uncaught exceptions / unhandled rejections are captured, console
// warn/error become Sentry logs and every console call a breadcrumb. tRPC
// procedures (lib/trpc.ts) and server functions (start.tsx) add their own
// spans + error capture through @sentry/core, which shares this client.

import * as Sentry from "@sentry/bun"
import {
  deriveEnvironment,
  dsnFor,
  glitchtipInternalUrl,
  internalEnvelopeUrl,
  parseDsn,
  tracesSampleRate,
} from "./config"
import { WEB_RELEASE } from "./release"
import { DATA_COLLECTION, scrubBreadcrumb, scrubEvent, scrubLog } from "./scrub"
import { isClientAbortEvent } from "@/lib/client-abort"
import { isProductionBuild } from "@/lib/production-build"

export function initServerObservability(env = process.env): boolean {
  const dsn = dsnFor(env, `server`)
  const internal = glitchtipInternalUrl(env)
  const parsed = parseDsn(dsn ?? undefined)
  if (!dsn || !internal || !parsed) return false
  Sentry.init({
    dsn,
    tunnel: internalEnvelopeUrl(internal, parsed),
    release: WEB_RELEASE,
    environment: deriveEnvironment(env, isProductionBuild),
    sampleRate: 1,
    tracesSampleRate: tracesSampleRate(env),
    dataCollection: DATA_COLLECTION,
    // Transactions, not v11's default span streaming: GlitchTip ingests
    // transaction envelopes, and beforeSendTransaction (the scrubber) only
    // runs on them.
    traceLifecycle: `static`,
    // The server never propagates our trace into third parties (GitHub,
    // Creem, push relay, …); only the client → server hop carries it.
    tracePropagationTargets: [],
    integrations: [Sentry.consoleLoggingIntegration({ levels: [`warn`, `error`] })],
    // A client that hung up mid-request (Bun's connection-closed AbortError)
    // is expected traffic: never an error issue.
    beforeSend: (event) => (isClientAbortEvent(event) ? null : scrubEvent(event)),
    beforeSendTransaction: (event) => scrubEvent(event),
    beforeSendLog: (log) => scrubLog(log),
    beforeBreadcrumb: (crumb) => scrubBreadcrumb(crumb),
  })
  return true
}

initServerObservability()
