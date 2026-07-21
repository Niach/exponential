import { afterEach, describe, expect, it, vi } from "vitest"
import { proxyElectricRequest } from "@/lib/electric-proxy"

describe(`proxyElectricRequest`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it(`overrides upstream caching headers so shape responses are never cached`, async () => {
    // Electric snapshot responses ship long-lived public cache-control with
    // no auth-aware vary — cached authed snapshots poisoned macOS URLCache.
    // The proxy must force never-cache on every response.
    vi.stubGlobal(
      `fetch`,
      vi.fn().mockResolvedValue(
        new Response(`[]`, {
          status: 200,
          headers: {
            "cache-control": `public, max-age=604800, stale-while-revalidate=2629746`,
            "electric-handle": `123-456`,
          },
        })
      )
    )

    const response = await proxyElectricRequest(
      new URL(`http://localhost:30000/v1/shape?table=issues`)
    )

    expect(response.status).toBe(200)
    expect(response.headers.get(`cache-control`)).toBe(`private, no-store`)
    // vary must cover every credential the shape route accepts, including
    // x-api-key (accepted alongside authorization and the session cookie).
    expect(response.headers.get(`vary`)).toBe(`authorization, cookie, x-api-key`)
    // Electric protocol headers must still pass through untouched.
    expect(response.headers.get(`electric-handle`)).toBe(`123-456`)
  })
})

// REV2-5: initial-snapshot (offset=-1) proxying is bounded by a semaphore —
// snapshot bodies are buffered wholly in memory, so a thundering herd (every
// member of a team cold-starting 14 shapes) must degrade to FIFO queueing,
// not unbounded Bun heap. These tests drive the gate through deferred fetch
// resolutions. Each test drains every in-flight request before finishing so
// the module-level slot count stays clean across tests.

type Deferred = {
  resolve: (response: Response) => void
  reject: (error: Error) => void
}

function deferredFetch(): {
  mock: ReturnType<typeof vi.fn>
  calls: Deferred[]
} {
  const calls: Deferred[] = []
  const mock = vi.fn(
    () =>
      new Promise<Response>((resolve, reject) => {
        calls.push({ resolve, reject })
      })
  )
  return { mock, calls }
}

function snapshotUrl(index: number): URL {
  return new URL(`http://electric.local/v1/shape?table=t${index}&offset=-1`)
}

function livePollUrl(): URL {
  return new URL(
    `http://electric.local/v1/shape?table=t&offset=123_0&live=true`
  )
}

// Lets queued acquisitions / releases run their microtasks.
async function settle() {
  for (let i = 0; i < 10; i++) await Promise.resolve()
}

describe(`snapshot proxy concurrency gate`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it(`caps concurrent snapshot fetches and promotes queued requests FIFO`, async () => {
    const { mock, calls } = deferredFetch()
    vi.stubGlobal(`fetch`, mock)

    const pending = Array.from({ length: 10 }, (_, i) =>
      proxyElectricRequest(snapshotUrl(i))
    )
    await settle()

    // Only 8 snapshots may buffer concurrently; the other 2 queue.
    expect(mock).toHaveBeenCalledTimes(8)

    calls[0].resolve(new Response(`snapshot-0`))
    await settle()
    expect(mock).toHaveBeenCalledTimes(9)

    calls[1].resolve(new Response(`snapshot-1`))
    await settle()
    expect(mock).toHaveBeenCalledTimes(10)

    for (const call of calls.slice(2)) call.resolve(new Response(`ok`))
    const responses = await Promise.all(pending)
    for (const response of responses) expect(response.status).toBe(200)
  })

  it(`live long-polls bypass the gate even while snapshots saturate it`, async () => {
    const { mock, calls } = deferredFetch()
    vi.stubGlobal(`fetch`, mock)

    const snapshots = Array.from({ length: 9 }, (_, i) =>
      proxyElectricRequest(snapshotUrl(i))
    )
    await settle()
    expect(mock).toHaveBeenCalledTimes(8)

    // A live poll must not queue behind the saturated snapshot gate — its
    // body is tiny and gating it would starve every synced client.
    const livePoll = proxyElectricRequest(livePollUrl())
    await settle()
    expect(mock).toHaveBeenCalledTimes(9)

    for (const call of calls) call.resolve(new Response(`ok`))
    await settle()
    // The 9th snapshot got its slot after a release.
    expect(mock).toHaveBeenCalledTimes(10)
    calls[9].resolve(new Response(`ok`))

    const responses = await Promise.all([...snapshots, livePoll])
    for (const response of responses) expect(response.status).toBe(200)
  })

  it(`a snapshot aborted while queued answers 499 without ever fetching`, async () => {
    const { mock, calls } = deferredFetch()
    vi.stubGlobal(`fetch`, mock)

    const holders = Array.from({ length: 8 }, (_, i) =>
      proxyElectricRequest(snapshotUrl(i))
    )
    await settle()
    expect(mock).toHaveBeenCalledTimes(8)

    const controller = new AbortController()
    const queued = proxyElectricRequest(snapshotUrl(99), controller.signal)
    await settle()
    controller.abort()

    const response = await queued
    expect(response.status).toBe(499)
    expect(mock).toHaveBeenCalledTimes(8)

    // Draining the holders must not over-release the slot the aborted
    // request never held.
    for (const call of calls) call.resolve(new Response(`ok`))
    await Promise.all(holders)
  })

  it(`releases the slot when the upstream fetch fails`, async () => {
    const { mock, calls } = deferredFetch()
    vi.stubGlobal(`fetch`, mock)

    const holders = Array.from({ length: 8 }, (_, i) =>
      proxyElectricRequest(snapshotUrl(i))
    )
    const queued = proxyElectricRequest(snapshotUrl(99))
    await settle()
    expect(mock).toHaveBeenCalledTimes(8)

    calls[0].reject(new Error(`upstream down`))
    await settle()
    // The failed snapshot's slot went to the queued request.
    expect(mock).toHaveBeenCalledTimes(9)

    for (const call of calls.slice(1)) call.resolve(new Response(`ok`))
    const [failed, ...rest] = await Promise.all([
      holders[0],
      ...holders.slice(1),
      queued,
    ])
    expect(failed.status).toBe(502)
    for (const response of rest) expect(response.status).toBe(200)
  })
})
