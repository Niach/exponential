// In-process token buckets for the public widget endpoints. Per-replica by
// design: the cloud deploy is a single Coolify instance, so a shared store
// (Redis/PG) is deliberately deferred until the web app ever scales out.
interface Bucket {
  tokens: number
  lastRefillMs: number
}

export type RateLimitResult =
  | { ok: true }
  | { ok: false; retryAfterSeconds: number }

export class TokenBucketLimiter {
  private readonly buckets = new Map<string, Bucket>()

  constructor(
    private readonly opts: {
      // Burst size: max tokens a single key can hold/spend at once.
      capacity: number
      // Sustained rate: tokens regained per hour.
      refillPerHour: number
      // Bound on tracked keys; full buckets are evicted lazily past this.
      maxEntries?: number
    }
  ) {}

  /** Currently-tracked bucket keys — a cheap gauge for the admin
   * performance page (EXP-553). */
  get trackedKeys(): number {
    return this.buckets.size
  }

  tryTake(key: string, nowMs = Date.now()): RateLimitResult {
    const refillPerMs = this.opts.refillPerHour / 3_600_000
    let bucket = this.buckets.get(key)

    if (!bucket) {
      this.evictIfNeeded(nowMs)
      bucket = { tokens: this.opts.capacity, lastRefillMs: nowMs }
      this.buckets.set(key, bucket)
    } else {
      const elapsed = Math.max(0, nowMs - bucket.lastRefillMs)
      bucket.tokens = Math.min(
        this.opts.capacity,
        bucket.tokens + elapsed * refillPerMs
      )
      bucket.lastRefillMs = nowMs
    }

    if (bucket.tokens >= 1) {
      bucket.tokens -= 1
      return { ok: true }
    }

    const deficit = 1 - bucket.tokens
    return {
      ok: false,
      retryAfterSeconds: Math.max(1, Math.ceil(deficit / refillPerMs / 1000)),
    }
  }

  private evictIfNeeded(nowMs: number) {
    const maxEntries = this.opts.maxEntries ?? 10_000
    if (this.buckets.size < maxEntries) return

    const refillPerMs = this.opts.refillPerHour / 3_600_000
    for (const [key, bucket] of this.buckets) {
      const elapsed = Math.max(0, nowMs - bucket.lastRefillMs)
      if (bucket.tokens + elapsed * refillPerMs >= this.opts.capacity) {
        this.buckets.delete(key)
      }
    }
  }
}

// Also reused by the other public-write limiter (the /api/contact endpoint).
export function envInt(name: string, fallback: number): number {
  const parsed = Number.parseInt(process.env[name] ?? ``, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

// THE widget submit rate-limit policy (NEW-19). Every number lives in
// WIDGET_SUBMIT_LIMITS below, each env-tunable; this comment is the
// documentation of the limit.
//
// - EVERY submit (identified or anonymous) counts per client IP: burst 20,
//   refill 120/hour (= 2 a minute). Reporters behind one office IP share it.
// - The plan/per-key bucket (submit-limit.ts) applies on top: self-hosted
//   per key (burst 20, 60/hour), cloud per team by plan (free: burst 20,
//   60/hour). Its burst matches the per-IP one so a single reporter's run of
//   20 is never cut short by the shared bucket first.
// - The confirmation MAIL to a reporter address: 6/hour, burst 3 per
//   address. It never rejects a report — past it the issue is still created
//   and only the mail is skipped (`emailDelivered: false`). It exists because
//   the per-team bucket is unlimited on paid plans, so without it one address
//   could be mail-bombed via IP/key rotation.
//
// Order (handleWidgetSubmit): the per-IP bucket first, then the shared
// per-key/per-team one, short-circuiting — a request already rejected by its
// own IP must never drain the shared bucket, or one hostile client 429s every
// legit reporter of that widget.
export const WIDGET_SUBMIT_LIMITS = {
  ip: {
    burstEnv: `WIDGET_RATE_LIMIT_IP_BURST`,
    burst: 20,
    hourlyEnv: `WIDGET_RATE_LIMIT_PER_IP_HOURLY`,
    hourly: 120,
  },
  key: {
    burstEnv: `WIDGET_RATE_LIMIT_KEY_BURST`,
    burst: 20,
    hourlyEnv: `WIDGET_RATE_LIMIT_PER_KEY_HOURLY`,
    hourly: 60,
  },
  confirmationMail: {
    burstEnv: `WIDGET_RATE_LIMIT_RECIPIENT_BURST`,
    burst: 3,
    hourlyEnv: `WIDGET_RATE_LIMIT_PER_RECIPIENT_HOURLY`,
    hourly: 6,
  },
} as const

function limiterFor(
  limit: (typeof WIDGET_SUBMIT_LIMITS)[keyof typeof WIDGET_SUBMIT_LIMITS]
): TokenBucketLimiter {
  return new TokenBucketLimiter({
    capacity: envInt(limit.burstEnv, limit.burst),
    refillPerHour: envInt(limit.hourlyEnv, limit.hourly),
  })
}

interface WidgetRateLimiters {
  perKeyLimiter: TokenBucketLimiter
  perIpLimiter: TokenBucketLimiter
  perRecipientLimiter: TokenBucketLimiter
}

let widgetLimiters: WidgetRateLimiters | null = null

export function getWidgetRateLimiters(): WidgetRateLimiters {
  widgetLimiters ??= {
    perKeyLimiter: limiterFor(WIDGET_SUBMIT_LIMITS.key),
    perIpLimiter: limiterFor(WIDGET_SUBMIT_LIMITS.ip),
    perRecipientLimiter: limiterFor(WIDGET_SUBMIT_LIMITS.confirmationMail),
  }
  return widgetLimiters
}

export function resetWidgetRateLimitersForTest(): void {
  widgetLimiters = null
}

// The submit's per-IP token (the per-key/per-team one is taken AFTER it).
export function takeWidgetIpToken(
  ip: string,
  nowMs = Date.now()
): RateLimitResult {
  return getWidgetRateLimiters().perIpLimiter.tryTake(`ip:${ip}`, nowMs)
}

// The confirmation-mail gate, keyed on the lowercased address: false = skip
// the mail, never the report.
export function takeWidgetConfirmationMailToken(
  email: string,
  nowMs = Date.now()
): boolean {
  return getWidgetRateLimiters().perRecipientLimiter.tryTake(
    `reporter-email:${email.trim().toLowerCase()}`,
    nowMs
  ).ok
}

// Per-IP bucket for the anonymous GET /api/widget/config (REV-25). Separate
// from the submit buckets and far more generous: the config fetch fires on
// every host-page load and one NAT'd office/CGNAT IP can front hundreds of
// legitimate visitors (each browser re-fetches at most every 5 minutes — the
// response is `max-age=300`). It still has to exist because the key lookup
// costs a DB round trip before the origin allowlist can reject anything, so
// an unthrottled key-scanning loop drives the shared pool at line rate.
let configIpLimiter: TokenBucketLimiter | null = null

export function getWidgetConfigIpLimiter(): TokenBucketLimiter {
  configIpLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`WIDGET_CONFIG_RATE_LIMIT_IP_BURST`, 60),
    refillPerHour: envInt(`WIDGET_CONFIG_RATE_LIMIT_PER_IP_HOURLY`, 600),
  })
  return configIpLimiter
}

// THE proxy assumption every per-IP limiter rests on (NEW-85). Every deploy
// target fronts the Bun server with a reverse proxy that APPENDS the peer it
// accepted the connection from to any client-supplied x-forwarded-for:
// Traefik on Coolify (cloud + staging), Caddy in dev and in the self-host
// compose. So the rightmost entry is the only proxy-attested one; everything
// left of it is whatever the client sent (keying buckets on the leftmost let
// a header-rotating client mint a fresh limit per request).
//
// TRUSTED_PROXY_HOPS (default 1) = how many appending proxies sit in front of
// the server. Behind ONE (the stock deploys) the client is the LAST hop; put
// a CDN/load balancer that also appends in front of Traefik/Caddy and set it
// to 2, and the client is the second-to-last. Setting it higher than the real
// chain hands the leftmost (spoofable) entry back, so never over-count. A
// request with fewer hops than configured (it skipped a proxy, e.g. a direct
// hit on the container port) keys on its leftmost entry.
export function trustedProxyHops(): number {
  return envInt(`TRUSTED_PROXY_HOPS`, 1)
}

/** The proxy-attested client address off x-forwarded-for (see above), or
 * null without the header (a bare dev server). Not normalised. */
export function proxyAttestedClientIp(
  request: Request,
  hops = trustedProxyHops()
): string | null {
  const forwarded = request.headers.get(`x-forwarded-for`)
  const entries = (forwarded ?? ``)
    .split(`,`)
    .map((entry) => entry.trim())
    .filter(Boolean)
  if (entries.length === 0) return null
  return entries[Math.max(0, entries.length - hops)]!
}

const IPV4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/

function ipv4Octets(text: string): number[] | null {
  const match = IPV4.exec(text)
  if (!match) return null
  const octets = match.slice(1).map(Number)
  return octets.every((octet) => octet <= 255) ? octets : null
}

/** Eight 16-bit groups of an IPv6 address (compressed `::` and a dotted IPv4
 * tail allowed), or null when it is not one. */
function ipv6Groups(text: string): number[] | null {
  let body = text
  const tail: number[] = []
  const lastColon = body.lastIndexOf(`:`)
  if (body.includes(`.`)) {
    const v4 = ipv4Octets(body.slice(lastColon + 1))
    if (!v4) return null
    tail.push((v4[0]! << 8) | v4[1]!, (v4[2]! << 8) | v4[3]!)
    body = body.slice(0, lastColon + 1)
    // `::1.2.3.4` leaves `::`; `64:ff9b::1.2.3.4` leaves `64:ff9b::`; a plain
    // `x:...:y:1.2.3.4` leaves a trailing single colon to drop.
    if (!body.endsWith(`::`)) body = body.slice(0, -1)
  }
  const halves = body.split(`::`)
  if (halves.length > 2) return null
  const parse = (part: string) =>
    part === `` ? [] : part.split(`:`).map((g) => (/^[0-9a-f]{1,4}$/.test(g) ? parseInt(g, 16) : NaN))
  const head = parse(halves[0]!)
  const rest = halves.length === 2 ? parse(halves[1]!) : []
  const explicit = head.length + rest.length + tail.length
  if ([...head, ...rest].some(Number.isNaN)) return null
  if (halves.length === 1) return explicit === 8 ? [...head, ...tail] : null
  if (explicit > 7) return null
  return [...head, ...Array(8 - explicit).fill(0), ...rest, ...tail]
}

/**
 * The per-IP LIMITER key for one client address (NEW-85). IPv4 keys on the
 * address. IPv6 keys on its /64: one subscriber line (home, phone, VPS) gets
 * a whole /64 and picks interface ids at will, so a per-address key let one
 * client rotate through 2^64 fresh buckets. An IPv4-mapped address
 * (`::ffff:1.2.3.4`, what a dual-stack socket reports) is the IPv4 one.
 * Brackets, a port and a zone id (`[fe80::1%eth0]:443`) are dropped first;
 * anything unparseable keys on its own lowercased text.
 */
export function rateLimitIpKey(raw: string): string {
  const { address, v6 } = limiterAddress(raw)
  return v6 ? `${address}/64` : address
}

/**
 * FEED-159: `rateLimitIpKey` as a plain IP literal, for a limiter that
 * insists on a valid address (Better Auth's `getIp` drops anything else):
 * IPv4 and IPv4-mapped = the IPv4 address, IPv6 = the /64's network
 * address (`2001:db8:1:2::`), unparseable = its own lowercased text.
 */
export function rateLimitIpAddress(raw: string): string {
  return limiterAddress(raw).address
}

function limiterAddress(raw: string): { address: string; v6: boolean } {
  let text = raw.trim().toLowerCase()
  const bracketed = /^\[([^\]]+)\](?::\d+)?$/.exec(text)
  if (bracketed) text = bracketed[1]!
  else if (/^[\d.]+:\d+$/.test(text)) text = text.slice(0, text.lastIndexOf(`:`))
  const zone = text.indexOf(`%`)
  if (zone !== -1) text = text.slice(0, zone)
  const v4 = ipv4Octets(text)
  if (v4) return { address: v4.join(`.`), v6: false }
  const groups = text.includes(`:`) ? ipv6Groups(text) : null
  if (!groups) return { address: text || `unknown`, v6: false }
  if (groups.slice(0, 5).every((g) => g === 0) && groups[5] === 0xffff) {
    const mapped = [groups[6]! >> 8, groups[6]! & 255, groups[7]! >> 8, groups[7]! & 255]
    return { address: mapped.join(`.`), v6: false }
  }
  return { address: `${groups.slice(0, 4).map((g) => g.toString(16)).join(`:`)}::`, v6: true }
}

/** The per-IP limiter key of a request: its proxy-attested client address
 * (TRUSTED_PROXY_HOPS), IPv6 grouped by /64. `unknown` without the header.
 * Every caller keys a token bucket on it; none stores it as an address. */
export function clientIpFromRequest(request: Request): string {
  const ip = proxyAttestedClientIp(request)
  return ip ? rateLimitIpKey(ip) : `unknown`
}
