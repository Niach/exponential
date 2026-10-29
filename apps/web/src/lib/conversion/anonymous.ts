import { createHash, createHmac } from "node:crypto"
import { proxyAttestedClientIp } from "@/lib/widget/rate-limit"

// Cookieless visitor identity (EXP-362), Plausible-style: a salted hash of
// ip+ua where the salt rotates every UTC day. Deterministic within a day (so
// the landing unique index dedupes repeat visits and signup can link back to
// the same-day landing event) and unlinkable across days — nothing is ever
// stored on the visitor's device, so no EU consent banner is needed.
function dailySalt(secret: string, now: Date): string {
  const day = now.toISOString().slice(0, 10)
  return createHash(`sha256`).update(`${secret}|${day}`).digest(`hex`)
}

export function dailyAnonymousIdFromHeaders(
  headers: Headers,
  now: Date = new Date()
): string | null {
  const secret = process.env.BETTER_AUTH_SECRET
  if (!secret) return null
  // Same proxy-attested hop as the limiters (`proxyAttestedClientIp`,
  // TRUSTED_PROXY_HOPS). No proxy (bare dev server) → no stable identity →
  // null, and callers skip recording.
  const ip = proxyAttestedClientIp(new Request(`http://local/`, { headers }))
  if (!ip) return null
  const ua = headers.get(`user-agent`) ?? ``
  return createHmac(`sha256`, dailySalt(secret, now))
    .update(`${ip}|${ua}`)
    .digest(`hex`)
    .slice(0, 32)
}

export function dailyAnonymousId(
  req: Request,
  now: Date = new Date()
): string | null {
  return dailyAnonymousIdFromHeaders(req.headers, now)
}
