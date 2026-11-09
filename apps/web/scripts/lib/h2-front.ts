/**
 * An HTTP/2 front for a plain-HTTP capture base (`CAPTURE_BASE_URL=http://…`).
 *
 * Over HTTP/1.1 Chromium opens at most 6 connections per origin, and the app's
 * Electric shapes hold that many live long-polls open for good. Every request
 * made AFTER the first paint then queues forever: a client-side navigation's
 * route chunk, `get-session`, `teams.getBySlug`. The router stays pending and
 * the capture never shows the target screen (a deep link works, its code ships
 * with the document). The documented stack dodges this through the Caddy h2
 * proxy on https://localhost:3000; an isolated stack that points the capturer
 * straight at its web server (`http://localhost:51xx`) does not. So the
 * capturer puts its own TLS + h2 proxy in front of such a base: one multiplexed
 * connection, no cap. Captures already accept self-signed certs
 * (`launchContext`).
 *
 * The proxy speaks for the target's origin: `Host`/`Origin`/`Referer` are
 * rewritten to it (Better Auth checks the origin) and `Location` back to the
 * proxy, so sign-in and redirects behave exactly as against the base itself.
 */
import { execFileSync } from "node:child_process"
import { mkdtempSync, readFileSync, rmSync } from "node:fs"
import http, { type IncomingHttpHeaders, type OutgoingHttpHeaders } from "node:http"
import http2, { type Http2ServerRequest, type Http2ServerResponse } from "node:http2"
import type { AddressInfo } from "node:net"
import { tmpdir } from "node:os"
import { join } from "node:path"

export interface CaptureBase {
  /** What the browser should open. */
  url: string
  close: () => Promise<void>
}

/** Hop-by-hop headers HTTP/2 forbids (RFC 9113 §8.2.2), plus h2 pseudo-headers. */
const HOP_BY_HOP = new Set([
  `connection`,
  `keep-alive`,
  `proxy-connection`,
  `transfer-encoding`,
  `upgrade`,
  `te`,
  `http2-settings`,
  `host`,
])

/** True when the base needs the h2 front: plain `http://`. */
export function needsH2Front(base: string): boolean {
  return new URL(base).protocol === `http:`
}

/** The request headers to forward, speaking for `target`'s origin. */
export function forwardRequestHeaders(
  headers: IncomingHttpHeaders,
  front: string,
  target: URL
): OutgoingHttpHeaders {
  const out: OutgoingHttpHeaders = {}
  for (const [name, value] of Object.entries(headers)) {
    if (value === undefined || name.startsWith(`:`) || HOP_BY_HOP.has(name)) continue
    out[name] = typeof value === `string` ? swapOrigin(value, front, target.origin) : value
  }
  out.host = target.host
  return out
}

/** The response headers to send back, pointing the target's origin at the front. */
export function forwardResponseHeaders(
  headers: IncomingHttpHeaders,
  front: string,
  target: URL
): OutgoingHttpHeaders {
  const out: OutgoingHttpHeaders = {}
  for (const [name, value] of Object.entries(headers)) {
    if (value === undefined || HOP_BY_HOP.has(name)) continue
    out[name] = name === `location` && typeof value === `string` ? swapOrigin(value, target.origin, front) : value
  }
  return out
}

function swapOrigin(value: string, from: string, to: string): string {
  return value === from || value.startsWith(`${from}/`) ? `${to}${value.slice(from.length)}` : value
}

/** A throwaway self-signed localhost cert (openssl ships on every capture host). */
function selfSignedCert(): { key: Buffer; cert: Buffer } {
  const dir = mkdtempSync(join(tmpdir(), `capture-h2-`))
  try {
    execFileSync(
      `openssl`,
      [
        `req`, `-x509`, `-newkey`, `rsa:2048`, `-nodes`, `-days`, `2`, `-subj`, `/CN=localhost`,
        `-keyout`, join(dir, `key.pem`), `-out`, join(dir, `cert.pem`),
      ],
      { stdio: `ignore` }
    )
    return { key: readFileSync(join(dir, `key.pem`)), cert: readFileSync(join(dir, `cert.pem`)) }
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
}

/**
 * The base the capturer should open: `base` itself when it is already https
 * (the Caddy proxy), else a local h2 front for it.
 */
export async function captureBase(base: string): Promise<CaptureBase> {
  if (!needsH2Front(base)) return { url: base, close: async () => {} }
  const target = new URL(base)
  let front = ``
  const server = http2.createSecureServer(
    { ...selfSignedCert(), allowHTTP1: true },
    (req: Http2ServerRequest, res: Http2ServerResponse) => {
      const upstream = http.request(
        {
          hostname: target.hostname,
          port: target.port || 80,
          method: req.method,
          path: req.url,
          headers: forwardRequestHeaders(req.headers, front, target),
        },
        (answer) => {
          res.writeHead(answer.statusCode ?? 502, forwardResponseHeaders(answer.headers, front, target))
          answer.pipe(res)
        }
      )
      upstream.on(`error`, () => {
        if (!res.headersSent) res.writeHead(502)
        res.end()
      })
      // A page that navigates away aborts its long-polls: drop the upstream too.
      res.on(`close`, () => upstream.destroy())
      req.pipe(upstream)
    }
  )
  const sessions = new Set<http2.ServerHttp2Session>()
  server.on(`session`, (session) => {
    sessions.add(session)
    session.on(`close`, () => sessions.delete(session))
  })
  await new Promise<void>((done) => server.listen(0, `127.0.0.1`, done))
  front = `https://localhost:${(server.address() as AddressInfo).port}`
  return {
    url: front,
    close: () =>
      new Promise<void>((done) => {
        server.close(() => done())
        for (const session of sessions) session.destroy()
      }),
  }
}
