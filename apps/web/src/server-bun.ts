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
import { bootstrapSelfHosted } from "@/lib/bootstrap-self-hosted"
import { startFcmTokenSweepScheduler } from "@/lib/fcm-token-sweep"
import { startEmailDigestScheduler } from "@/lib/notification-email-digest"
import { startProjectTrashScheduler } from "@/lib/project-trash"
import { startCodingSessionSweepScheduler } from "@/lib/coding-session-sweep"
import {
  injectMeta,
  matchPublicPath,
  resolvePublicPageMeta,
} from "@/lib/seo/public-meta"

// Fire-and-forget: seed the public workspace and promote initial admins.
// Idempotent; errors are logged inside bootstrapCloud(). Calling from
// server-bun.ts keeps the entire boostrap module (and its drizzle/pg deps)
// out of the client bundle.
bootstrapCloud().catch(() => {
  // already logged
})

// Self-hosted only: start the outbound PR-merge poller (no-op on cloud, which
// uses the GitHub webhook at /api/webhooks/github instead).
bootstrapSelfHosted()

// Push-first email digest: periodic sweep bundling notifications still unread
// ~1h after the push went out into one email per user (no-op without an email
// transport). In-process guard only — see the module for the multi-instance
// story.
startEmailDigestScheduler()

// Project trash: periodic sweep that hard-deletes projects trashed longer than
// the 48h retention window and reclaims their attachment blobs. In-process
// guard only; the row delete is the atomic multi-instance claim.
startProjectTrashScheduler()

// Coding sessions: periodic sweep that force-ends rows still `running` past
// the staleness window — a crashed desktop never fires its exit hook, and the
// orphaned row would otherwise pin a phantom "coding now" badge forever.
startCodingSessionSweepScheduler()

// FCM tokens: periodic sweep deleting token rows not re-registered within the
// staleness window — the server-side backstop for sign-outs whose best-effort
// unregister never landed and for old client builds that never unregister.
startFcmTokenSweepScheduler()

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
// follow-up. The dogfood feedback widget is cloud-only and same-origin
// ('self' covers it); self-hosted instances redirect to the cloud feedback
// board instead of embedding, so no external script-src entry is needed.
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

// The embeddable widget artifacts (apps/web/public/widget/, built by
// packages/widget) are loaded by third-party pages. Stable filenames with
// moderate TTLs instead of hashed bundles: a hashed widget.js would 404 for
// cached loaders across deploys. ACAO on scripts is defensive (classic
// script tags don't need CORS; API calls carry their own CORS headers).
function withWidgetAssetHeaders(req: Request, response: Response): Response {
  const pathname = new URL(req.url).pathname
  if (!pathname.startsWith(`/widget/`)) return response
  response.headers.set(`Access-Control-Allow-Origin`, `*`)
  response.headers.set(
    `Cache-Control`,
    pathname.endsWith(`/loader.js`)
      ? `public, max-age=300, stale-while-revalidate=86400`
      : `public, max-age=3600, stale-while-revalidate=86400`
  )
  return response
}

// Social-preview rewrite for public feedback boards. The app renders
// client-side, so unfurlers only ever see the generic __root.tsx head; for the
// two public routes we buffer the HTML shell and inject route-specific
// OG/Twitter/canonical meta (and flip noindex → index,follow). Only GET +
// text/html + 200 responses on a matching path are touched; everything else
// passes through untouched. NOTE: dev runs through the nitro-alpha bridge,
// which never reaches this file — this is prod-only (server-bun.ts/srvx).
async function withPublicMeta(req: Request, response: Response): Promise<Response> {
  if (req.method !== `GET`) return response
  if (response.status !== 200) return response
  if (!response.headers.get(`content-type`)?.includes(`text/html`)) {
    return response
  }
  const url = new URL(req.url)
  const match = matchPublicPath(url.pathname)
  if (!match) return response

  // Buffer the shell IMMEDIATELY and always return a fresh Response on this
  // path: srvx's lazy NodeResponse must not be returned (or consumed) after a
  // later await boundary — Bun then rejects it with "Expected a Response
  // object" and serves its default page.
  const body = await response.text()
  const status = response.status
  const statusText = response.statusText
  const headers = new Headers(response.headers)
  headers.delete(`content-length`)

  // Meta injection is best-effort decoration: any failure degrades to the
  // untouched shell, never a broken page.
  let rewritten = body
  try {
    const meta = await resolvePublicPageMeta(match)
    if (meta) {
      const origin =
        process.env.BETTER_AUTH_URL?.replace(/\/$/, ``) || url.origin
      rewritten = injectMeta(body, meta, origin)
    }
  } catch (err) {
    console.error(`[public-meta] injection failed:`, err)
  }
  return new Response(rewritten, { status, statusText, headers })
}

// h3 (inside the nitro chunk) can hand back its lazy NodeResponse wrapper —
// it masquerades as a Response via Symbol.hasInstance/prototype games, but
// Bun.serve requires the real native class and otherwise logs "Expected a
// Response object" and serves its default page. The wrapper exposes a
// `_response` getter that materializes the native Response; use it, with a
// copy-construct fallback for any other imposter.
function ensureNativeResponse(res: Response): Response {
  if (res.constructor === Response) return res
  const materialized = (res as unknown as { _response?: Response })._response
  if (materialized?.constructor === Response) return materialized
  return new Response(res.body, res)
}

let _fetch: (req: Request) => Response | Promise<Response> = async (req) =>
  withSecurityHeaders(
    withWidgetAssetHeaders(
      req,
      await withPublicMeta(req, ensureNativeResponse(await nitroApp.fetch(req)))
    )
  )
const ws = hasWebSocket
  ? wsAdapter({ resolve: resolveWebsocketHooks })
  : undefined

if (hasWebSocket && ws) {
  _fetch = async (req: Request) => {
    if (req.headers.get(`upgrade`) === `websocket`) {
      type BunWebSocketServer = Parameters<typeof ws.handleUpgrade>[1]
      const server = (
        req as unknown as { runtime: { bun: { server: BunWebSocketServer } } }
      ).runtime.bun.server
      const upgraded = ws.handleUpgrade(req, server)
      // crossws returns Response | undefined for non-upgrade fall-through;
      // the guard above ensures we only get here on websocket requests.
      return upgraded as Response | Promise<Response>
    }
    return withSecurityHeaders(
      withWidgetAssetHeaders(
        req,
        await withPublicMeta(
          req,
          ensureNativeResponse(await nitroApp.fetch(req))
        )
      )
    )
  }
}

// Hard cap on request bodies, enforced by Bun BEFORE any handler buffers the
// stream — the per-route Content-Length checks are only fast-path rejects and
// are trivially bypassed by chunked transfer encoding. 16MB covers the
// largest legitimate request (widget submit allows ~12MB, issue image uploads
// similar) while shutting the door on Bun's ~128MB default.
const MAX_REQUEST_BODY_BYTES = 16 * 1024 * 1024

serve({
  port,
  hostname: host,
  tls: cert && key ? { cert, key } : undefined,
  fetch: _fetch,
  bun: {
    idleTimeout: 255,
    maxRequestBodySize: MAX_REQUEST_BODY_BYTES,
    websocket: hasWebSocket ? ws?.websocket : undefined,
  },
})

trapUnhandledErrors()

if ((import.meta as unknown as { _tasks?: unknown })._tasks) {
  startScheduleRunner()
}

export default {}
