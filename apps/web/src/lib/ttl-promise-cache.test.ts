import { describe, expect, it, vi } from "vitest"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"

// REV2-7: the cache's contract is load-bearing for the shape long-poll hot
// path — concurrent renewals must coalesce into one factory call, and
// failures/null-sessions must never be served from cache.

function cacheWithClock<V>(opts?: {
  ttlMs?: number
  maxEntries?: number
  retain?: (value: V) => boolean
}) {
  const clock = { now: 0 }
  const cache = new TtlPromiseCache<V>({
    ttlMs: opts?.ttlMs ?? 10_000,
    maxEntries: opts?.maxEntries ?? 100,
    now: () => clock.now,
    retain: opts?.retain,
  })
  return { cache, clock }
}

describe(`TtlPromiseCache`, () => {
  it(`coalesces concurrent gets for one key into a single factory call`, async () => {
    const { cache } = cacheWithClock<string>()
    let resolveFactory!: (value: string) => void
    const factory = vi.fn(
      () => new Promise<string>((resolve) => (resolveFactory = resolve))
    )

    const first = cache.get(`k`, factory)
    const second = cache.get(`k`, factory)
    resolveFactory(`v`)

    expect(await first).toBe(`v`)
    expect(await second).toBe(`v`)
    expect(factory).toHaveBeenCalledTimes(1)
  })

  it(`serves the cached value within the TTL and refetches after expiry`, async () => {
    const { cache, clock } = cacheWithClock<string>({ ttlMs: 10_000 })
    const factory = vi.fn(async () => `v`)

    await cache.get(`k`, factory)
    clock.now = 9_999
    await cache.get(`k`, factory)
    expect(factory).toHaveBeenCalledTimes(1)

    clock.now = 10_000
    await cache.get(`k`, factory)
    expect(factory).toHaveBeenCalledTimes(2)
  })

  it(`keys are independent`, async () => {
    const { cache } = cacheWithClock<string>()
    const factory = vi.fn(async () => `v`)
    await cache.get(`a`, factory)
    await cache.get(`b`, factory)
    expect(factory).toHaveBeenCalledTimes(2)
  })

  it(`does not cache rejections — the next get retries`, async () => {
    const { cache } = cacheWithClock<string>()
    const factory = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(new Error(`transient`))
      .mockResolvedValue(`v`)

    await expect(cache.get(`k`, factory)).rejects.toThrow(`transient`)
    expect(cache.size).toBe(0)
    expect(await cache.get(`k`, factory)).toBe(`v`)
    expect(factory).toHaveBeenCalledTimes(2)
  })

  it(`drops retain=false values on settle but coalesces while in flight`, async () => {
    const { cache } = cacheWithClock<string | null>({
      retain: (value) => value !== null,
    })
    let resolveFactory!: (value: string | null) => void
    const factory = vi.fn(
      () => new Promise<string | null>((resolve) => (resolveFactory = resolve))
    )

    const first = cache.get(`k`, factory)
    const second = cache.get(`k`, factory)
    expect(factory).toHaveBeenCalledTimes(1)
    resolveFactory(null)
    expect(await first).toBeNull()
    expect(await second).toBeNull()
    expect(cache.size).toBe(0)

    const again = cache.get(`k`, factory)
    expect(factory).toHaveBeenCalledTimes(2)
    resolveFactory(`kept`)
    expect(await again).toBe(`kept`)
    expect(cache.size).toBe(1)
  })

  it(`a slow settle does not evict a newer entry for the same key`, async () => {
    const { cache, clock } = cacheWithClock<string | null>({
      ttlMs: 10_000,
      retain: (value) => value !== null,
    })
    let resolveSlow!: (value: string | null) => void
    const slow = cache.get(
      `k`,
      () => new Promise<string | null>((resolve) => (resolveSlow = resolve))
    )

    // The slow entry expires; a fresh one lands in its place.
    clock.now = 10_001
    const freshFactory = vi.fn(async () => `fresh`)
    await cache.get(`k`, freshFactory)
    expect(cache.size).toBe(1)

    // The stale factory now settles retain=false — it must NOT evict the
    // fresh entry.
    resolveSlow(null)
    expect(await slow).toBeNull()
    expect(cache.size).toBe(1)
    await cache.get(`k`, freshFactory)
    expect(freshFactory).toHaveBeenCalledTimes(1)
  })

  it(`sweeps expired entries at the cap before dropping live ones`, async () => {
    const { cache, clock } = cacheWithClock<string>({
      ttlMs: 10_000,
      maxEntries: 2,
    })
    const factory = vi.fn(async () => `v`)
    await cache.get(`a`, factory)
    await cache.get(`b`, factory)

    clock.now = 10_001
    await cache.get(`c`, factory)
    // a and b were expired and swept; only c remains.
    expect(cache.size).toBe(1)
  })

  it(`drops the oldest entry at the cap when nothing is expired`, async () => {
    const { cache } = cacheWithClock<string>({ ttlMs: 60_000, maxEntries: 2 })
    const factory = vi.fn(async () => `v`)
    await cache.get(`a`, factory)
    await cache.get(`b`, factory)
    await cache.get(`c`, factory)
    expect(cache.size).toBe(2)

    // b survived, a was the oldest and got dropped.
    await cache.get(`b`, factory)
    expect(factory).toHaveBeenCalledTimes(3)
    await cache.get(`a`, factory)
    expect(factory).toHaveBeenCalledTimes(4)
  })

  it(`delete and clear drop entries`, async () => {
    const { cache } = cacheWithClock<string>()
    const factory = vi.fn(async () => `v`)
    await cache.get(`a`, factory)
    await cache.get(`b`, factory)

    cache.delete(`a`)
    expect(cache.size).toBe(1)
    cache.clear()
    expect(cache.size).toBe(0)

    await cache.get(`b`, factory)
    expect(factory).toHaveBeenCalledTimes(3)
  })
})
