// Custom Nitro server entry, wired up via `nitro({ entry: ... })` in
// vite.config.ts. Near-verbatim copy of Nitro's stock bun preset
// runtime — the only deviation is `idleTimeout: 255` inside the srvx
// `bun: {}` options block.
//
// Why we own this file:
// Bun.serve defaults idleTimeout to 10s, while Electric SQL long-polls
// regularly block ~20s on the server. On the Traefik → Bun keep-alive
// connection that surfaces as `502 Bad Gateway error=EOF` for every
// shape long-poll and breaks realtime sync. Setting idleTimeout to 255
// (Bun's max) keeps the connection alive for the full long-poll window.
//
// `nitro.entry` is a public NitroConfig field; srvx's `bun` option is
// typed `Omit<Bun.Serve.Options, "fetch">` and is forwarded directly
// into Bun.serve, so this is the framework-supported extension point —
// no patching of generated bundles, no runtime monkey-patching.

// @ts-expect-error — virtual module resolved by Nitro at build time.
import "#nitro-internal-pollyfills"
import { serve } from "srvx/bun"
import wsAdapter from "crossws/adapters/bun"
import { useNitroApp } from "nitro/app"
import { startScheduleRunner } from "nitro/~internal/runtime/task"
import { trapUnhandledErrors } from "nitro/~internal/runtime/error/hooks"
import { resolveWebsocketHooks } from "nitro/~internal/runtime/app"
// @ts-expect-error — virtual feature-flag module emitted by Nitro per build.
import { hasWebSocket } from "#nitro-internal-virtual/feature-flags"
import { bootstrapCloud } from "@/lib/bootstrap-cloud"

// Fire-and-forget: seed the public workspace and promote initial admins.
// Idempotent; errors are logged inside bootstrapCloud(). Calling from
// server-bun.ts keeps the entire boostrap module (and its drizzle/pg deps)
// out of the client bundle.
bootstrapCloud().catch(() => {
  // already logged
})

const port =
  Number.parseInt(process.env.NITRO_PORT || process.env.PORT || ``) || 3000
const host = process.env.NITRO_HOST || process.env.HOST
const cert = process.env.NITRO_SSL_CERT
const key = process.env.NITRO_SSL_KEY

const nitroApp = useNitroApp()
const securityHeadersEnabled = process.env.SECURITY_HEADERS_ENABLED === `true`

// Conservative CSP that allows TanStack Start's inline hydration script,
// Google OAuth redirects (image avatars from googleusercontent), and
// Electric long-poll requests against the same origin. Tightening to
// nonce-based scripts requires a Start-internal change and is left as
// follow-up.
const SECURITY_HEADERS: Record<string, string> = {
  "Content-Security-Policy": [
    `default-src 'self'`,
    `script-src 'self' 'unsafe-inline'`,
    `style-src 'self' 'unsafe-inline' https://fonts.googleapis.com`,
    `img-src 'self' data: blob: https://*.googleusercontent.com`,
    `font-src 'self' data: https://fonts.gstatic.com`,
    `connect-src 'self' https: wss:`,
    `frame-src 'self' https://accounts.google.com`,
    `frame-ancestors 'none'`,
    `base-uri 'self'`,
    `form-action 'self' https://accounts.google.com`,
  ].join(`; `),
  "Referrer-Policy": `strict-origin-when-cross-origin`,
  "X-Content-Type-Options": `nosniff`,
  "X-Frame-Options": `SAMEORIGIN`,
  "Strict-Transport-Security": `max-age=63072000; includeSubDomains`,
}

function withSecurityHeaders(response: Response): Response {
  if (!securityHeadersEnabled) return response
  for (const [key, value] of Object.entries(SECURITY_HEADERS)) {
    if (!response.headers.has(key)) {
      response.headers.set(key, value)
    }
  }
  return response
}

let _fetch: (req: Request) => Response | Promise<Response> = async (req) =>
  withSecurityHeaders(await nitroApp.fetch(req))
const ws = hasWebSocket
  ? wsAdapter({ resolve: resolveWebsocketHooks })
  : undefined

if (hasWebSocket && ws) {
  _fetch = async (req: Request) => {
    if (req.headers.get(`upgrade`) === `websocket`) {
      const upgraded = ws.handleUpgrade(
        req,
        (req as unknown as { runtime: { bun: { server: unknown } } }).runtime
          .bun.server
      )
      // crossws returns Response | undefined for non-upgrade fall-through;
      // the guard above ensures we only get here on websocket requests.
      return upgraded as Response | Promise<Response>
    }
    return withSecurityHeaders(await nitroApp.fetch(req))
  }
}

serve({
  port,
  hostname: host,
  tls: cert && key ? { cert, key } : undefined,
  fetch: _fetch,
  bun: {
    idleTimeout: 255,
    websocket: hasWebSocket ? ws?.websocket : undefined,
  },
})

trapUnhandledErrors()

if ((import.meta as unknown as { _tasks?: unknown })._tasks) {
  startScheduleRunner()
}

export default {}
