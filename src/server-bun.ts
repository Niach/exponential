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

const port =
  Number.parseInt(process.env.NITRO_PORT || process.env.PORT || ``) || 3000
const host = process.env.NITRO_HOST || process.env.HOST
const cert = process.env.NITRO_SSL_CERT
const key = process.env.NITRO_SSL_KEY

const nitroApp = useNitroApp()
let _fetch: (req: Request) => Response | Promise<Response> = nitroApp.fetch
const ws = hasWebSocket
  ? wsAdapter({ resolve: resolveWebsocketHooks })
  : undefined

if (hasWebSocket && ws) {
  _fetch = (req: Request) => {
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
    return nitroApp.fetch(req)
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
