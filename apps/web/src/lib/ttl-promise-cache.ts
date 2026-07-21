// Bounded in-process TTL cache over PROMISES (REV2-7). Entries are inserted
// BEFORE the factory settles, so N concurrent get()s for the same key coalesce
// into ONE factory call — the point: a client's 14 near-simultaneous Electric
// shape long-poll renewals share a single DB query instead of racing 14.
//
// Per-replica by design, like the widget rate limiter (lib/widget/rate-limit.ts)
// and the mobile OAuth code store (lib/auth/mobile-oauth-code.ts): the cloud
// deploy is a single Coolify instance. Callers that mutate the underlying data
// invalidate synchronously after commit; the TTL is only the safety net for a
// missed writer, manual SQL, or a future multi-replica deploy.
//
// Failure semantics: a rejected factory promise is evicted on settle, and a
// resolved value failing `retain` (e.g. a null session) is never kept — so a
// transient DB error or a dead token can never be served from cache; both cost
// exactly one lookup per call, the same as before caching.
interface Entry<V> {
  promise: Promise<V>
  expiresAt: number
}

export class TtlPromiseCache<V> {
  private readonly map = new Map<string, Entry<V>>()
  private readonly ttlMs: number
  private readonly maxEntries: number
  private readonly now: () => number
  private readonly retain: (value: V) => boolean

  constructor(opts: {
    ttlMs: number
    // Bound on tracked keys; expired entries are evicted lazily past this,
    // then the oldest (Map insertion order) if still at the cap.
    maxEntries: number
    now?: () => number
    // Return false to drop a resolved value instead of caching it (in-flight
    // calls still coalesce on the pending promise).
    retain?: (value: V) => boolean
  }) {
    this.ttlMs = opts.ttlMs
    this.maxEntries = opts.maxEntries
    // Lazy global lookup (not a captured `Date.now` reference) so vitest
    // fake timers installed after module load still take effect.
    this.now = opts.now ?? (() => Date.now())
    this.retain = opts.retain ?? (() => true)
  }

  get(key: string, factory: () => Promise<V>): Promise<V> {
    const existing = this.map.get(key)
    const now = this.now()
    if (existing && now < existing.expiresAt) {
      return existing.promise
    }

    this.evictIfNeeded(now)

    // The settle callbacks only evict when the map still holds THIS entry —
    // a slow settle racing a newer entry must not evict the fresh one.
    const entry: Entry<V> = {
      promise: undefined as unknown as Promise<V>,
      expiresAt: now + this.ttlMs,
    }
    entry.promise = factory().then(
      (value) => {
        if (!this.retain(value) && this.map.get(key) === entry) {
          this.map.delete(key)
        }
        return value
      },
      (err: unknown) => {
        if (this.map.get(key) === entry) {
          this.map.delete(key)
        }
        throw err
      }
    )
    this.map.set(key, entry)
    return entry.promise
  }

  delete(key: string): void {
    this.map.delete(key)
  }

  clear(): void {
    this.map.clear()
  }

  get size(): number {
    return this.map.size
  }

  private evictIfNeeded(now: number): void {
    if (this.map.size < this.maxEntries) return
    for (const [key, entry] of this.map) {
      if (now >= entry.expiresAt) {
        this.map.delete(key)
      }
    }
    if (this.map.size < this.maxEntries) return
    // Still at the cap: drop the oldest insertions until one slot frees.
    for (const key of this.map.keys()) {
      this.map.delete(key)
      if (this.map.size < this.maxEntries) return
    }
  }
}
