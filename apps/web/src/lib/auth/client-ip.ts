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
// proxy-attested hop before Better Auth sees the request. "Last" honours
// TRUSTED_PROXY_HOPS (NEW-85, documented at `proxyAttestedClientIp`).
// FEED-159: the hop is rewritten to its LIMITER address (`rateLimitIpAddress`:
// IPv6 = its /64's network address, IPv4-mapped = the IPv4 one), so Better
// Auth buckets one IPv6 subscriber line once whatever its own normalisation
// does, exactly like the widget limiter.

import { proxyAttestedClientIp, rateLimitIpAddress } from "@/lib/widget/rate-limit"

/**
 * Rewrite `x-forwarded-for` to only its last (proxy-attested) hop, as its
 * limiter address (IPv6 by /64). A request
 * without the header passes through untouched — a proxy-less self-host then
 * keeps Better Auth's fail-open behavior (no limiting), unchanged from
 * before.
 */
export function withProxyAttestedClientIp(request: Request): Request {
  const forwarded = request.headers.get(`x-forwarded-for`)
  if (!forwarded) return request
  const last = proxyAttestedClientIp(request)
  if (!last) return request
  const address = rateLimitIpAddress(last)
  if (address === forwarded) return request
  const headers = new Headers(request.headers)
  headers.set(`x-forwarded-for`, address)
  return new Request(request, { headers })
}
