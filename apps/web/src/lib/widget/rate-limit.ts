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

// LAST hop of x-forwarded-for: every deploy target (Caddy locally, Traefik
// on Coolify) fronts the Bun server and APPENDS the real peer address to any
// client-supplied x-forwarded-for, so the rightmost entry is the only
// proxy-attested one. The leftmost is attacker-controlled — keying buckets on
// it let a header-rotating client mint a fresh limit per request.
export function clientIpFromRequest(request: Request): string {
  const forwarded = request.headers.get(`x-forwarded-for`)
  const hops = forwarded?.split(`,`) ?? []
  const last = hops[hops.length - 1]?.trim()
  return last || `unknown`
}
