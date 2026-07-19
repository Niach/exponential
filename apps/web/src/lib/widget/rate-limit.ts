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

// Module singletons used by the submit endpoint. Env-tunable so e2e tests can
// lower them; defaults: 60 submissions/hour per key (burst 10) and 60/hour
// per IP (burst 5).
let perKeyLimiter: TokenBucketLimiter | null = null
let perIpLimiter: TokenBucketLimiter | null = null

export function getWidgetRateLimiters() {
  perKeyLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`WIDGET_RATE_LIMIT_KEY_BURST`, 10),
    refillPerHour: envInt(`WIDGET_RATE_LIMIT_PER_KEY_HOURLY`, 60),
  })
  perIpLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`WIDGET_RATE_LIMIT_IP_BURST`, 5),
    refillPerHour: envInt(`WIDGET_RATE_LIMIT_PER_IP_HOURLY`, 60),
  })
  return { perKeyLimiter, perIpLimiter }
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
