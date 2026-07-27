import { gunzipSync } from "node:zlib"
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
    expect(response.headers.get(`vary`)).toBe(
      `authorization, cookie, x-api-key, accept-encoding`
    )
    // Electric protocol headers must still pass through untouched.
    expect(response.headers.get(`electric-handle`)).toBe(`123-456`)
  })
})

// EXP-304: shape bodies are fully buffered here anyway, so compressing them for
// clients that ask costs one pass over memory we already hold — and Electric's
// JSON compresses ~10x, which is what a mobile client's cold snapshot pays for.
describe(`proxyElectricRequest gzip`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  function stubBody(body: string, headers: Record<string, string> = {}) {
    vi.stubGlobal(
      `fetch`,
      vi.fn().mockResolvedValue(new Response(body, { status: 200, headers }))
    )
  }

  // Comfortably past GZIP_MIN_BYTES and highly compressible, like real shape JSON.
  const bigBody = JSON.stringify(
    Array.from({ length: 200 }, (_, index) => ({
      headers: { operation: `insert` },
      key: `"public"."issues"/"${index}"`,
      value: { id: `${index}`, title: `a repetitive issue title` },
    }))
  )

  it(`gzips the body when the client advertises gzip`, async () => {
    stubBody(bigBody)

    const response = await proxyElectricRequest(
      new URL(`http://localhost:30000/v1/shape?table=issues`),
      undefined,
      `gzip, deflate, br`
    )

    expect(response.headers.get(`content-encoding`)).toBe(`gzip`)
    const compressedLength = Number(response.headers.get(`content-length`))
    expect(compressedLength).toBeGreaterThan(0)
    expect(compressedLength).toBeLessThan(bigBody.length)
    // The wire bytes must gunzip back to exactly what Electric sent. (A locally
    // constructed Response does NOT auto-decode the way a fetched one does —
    // decoding is the client's job, which is the whole point of the header.)
    const wire = new Uint8Array(await response.arrayBuffer())
    expect(wire.byteLength).toBe(compressedLength)
    expect(gunzipSync(wire).toString(`utf8`)).toBe(bigBody)
  })

  it(`leaves the body alone when the client does not ask for gzip`, async () => {
    stubBody(bigBody)

    const response = await proxyElectricRequest(
      new URL(`http://localhost:30000/v1/shape?table=issues`),
      undefined,
      `identity`
    )

    expect(response.headers.get(`content-encoding`)).toBeNull()
    await expect(response.text()).resolves.toBe(bigBody)
  })

  it(`does not compress tiny live long-poll responses`, async () => {
    // An idle live poll is ~40 bytes of up-to-date control message; gzip would
    // make it bigger and burn CPU on every one of 15 shapes per client.
    const tiny = `[{"headers":{"control":"up-to-date"}}]`
    stubBody(tiny)

    const response = await proxyElectricRequest(
      new URL(`http://localhost:30000/v1/shape?table=issues&offset=0_0`),
      undefined,
      `gzip`
    )

    expect(response.headers.get(`content-encoding`)).toBeNull()
    await expect(response.text()).resolves.toBe(tiny)
  })

  it(`never double-encodes an already-encoded upstream body`, async () => {
    stubBody(bigBody, { "content-encoding": `gzip` })

    const response = await proxyElectricRequest(
      new URL(`http://localhost:30000/v1/shape?table=issues`),
      undefined,
      `gzip`
    )

    expect(response.headers.get(`content-encoding`)).toBeNull()
    await expect(response.text()).resolves.toBe(bigBody)
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

    const pending = Array.from({ length: 19 }, (_, i) =>
      proxyElectricRequest(snapshotUrl(i))
    )
    await settle()

    // Only 17 snapshots may buffer concurrently; the other 2 queue. The bound
    // is one client's full shape count (15) plus headroom (EXP-264) — a single
    // cold start must never queue behind itself.
    expect(mock).toHaveBeenCalledTimes(17)

    calls[0].resolve(new Response(`snapshot-0`))
    await settle()
    expect(mock).toHaveBeenCalledTimes(18)

    calls[1].resolve(new Response(`snapshot-1`))
    await settle()
    expect(mock).toHaveBeenCalledTimes(19)

    for (const call of calls.slice(2)) call.resolve(new Response(`ok`))
    const responses = await Promise.all(pending)
    for (const response of responses) expect(response.status).toBe(200)
  })

  it(`live long-polls bypass the gate even while snapshots saturate it`, async () => {
    const { mock, calls } = deferredFetch()
    vi.stubGlobal(`fetch`, mock)

    const snapshots = Array.from({ length: 18 }, (_, i) =>
      proxyElectricRequest(snapshotUrl(i))
    )
    await settle()
    expect(mock).toHaveBeenCalledTimes(17)

    // A live poll must not queue behind the saturated snapshot gate — its
    // body is tiny and gating it would starve every synced client.
    const livePoll = proxyElectricRequest(livePollUrl())
    await settle()
    expect(mock).toHaveBeenCalledTimes(18)

    for (const call of calls) call.resolve(new Response(`ok`))
    await settle()
    // The 18th snapshot got its slot after a release.
    expect(mock).toHaveBeenCalledTimes(19)
    calls[18].resolve(new Response(`ok`))

    const responses = await Promise.all([...snapshots, livePoll])
    for (const response of responses) expect(response.status).toBe(200)
  })

  it(`a snapshot aborted while queued answers 499 without ever fetching`, async () => {
    const { mock, calls } = deferredFetch()
    vi.stubGlobal(`fetch`, mock)

    const holders = Array.from({ length: 17 }, (_, i) =>
      proxyElectricRequest(snapshotUrl(i))
    )
    await settle()
    expect(mock).toHaveBeenCalledTimes(17)

    const controller = new AbortController()
    const queued = proxyElectricRequest(snapshotUrl(99), controller.signal)
    await settle()
    controller.abort()

    const response = await queued
    expect(response.status).toBe(499)
    expect(mock).toHaveBeenCalledTimes(17)

    // Draining the holders must not over-release the slot the aborted
    // request never held.
    for (const call of calls) call.resolve(new Response(`ok`))
    await Promise.all(holders)
  })

  it(`releases the slot when the upstream fetch fails`, async () => {
    const { mock, calls } = deferredFetch()
    vi.stubGlobal(`fetch`, mock)

    const holders = Array.from({ length: 17 }, (_, i) =>
      proxyElectricRequest(snapshotUrl(i))
    )
    const queued = proxyElectricRequest(snapshotUrl(99))
    await settle()
    expect(mock).toHaveBeenCalledTimes(17)

    calls[0].reject(new Error(`upstream down`))
    await settle()
    // The failed snapshot's slot went to the queued request.
    expect(mock).toHaveBeenCalledTimes(18)

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
