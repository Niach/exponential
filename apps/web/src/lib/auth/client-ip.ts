// EXP-503: Better Auth's rate limiter keys its per-IP buckets on `getIp`,
// which reads the FIRST x-forwarded-for entry — but every deploy target
// (Caddy locally and self-hosted, Traefik on Coolify) APPENDS the real peer
// address to any client-supplied x-forwarded-for, so the leftmost entry is
// attacker-controlled. Keying on it lets a header-rotating client mint a
// fresh limit per request (bypassing the sign-in limiter entirely) or spoof
// a victim's IP to exhaust the victim's strict sign-in bucket. The widget
// limiter already keys on the LAST hop for exactly this reason
// (lib/widget/rate-limit.ts `clientIpFromRequest`); `ipAddressHeaders` can't
// express a last-hop strategy, so rewrite the header to the one
// proxy-attested hop before Better Auth sees the request.

/**
 * Rewrite `x-forwarded-for` to only its last (proxy-attested) hop. A request
 * without the header passes through untouched — a proxy-less self-host then
 * keeps Better Auth's fail-open behavior (no limiting), unchanged from
 * before.
 */
export function withProxyAttestedClientIp(request: Request): Request {
  const forwarded = request.headers.get(`x-forwarded-for`)
  if (!forwarded) return request
  const last = forwarded.split(`,`).pop()?.trim()
  if (!last || last === forwarded) return request
  const headers = new Headers(request.headers)
  headers.set(`x-forwarded-for`, last)
  return new Request(request, { headers })
}
