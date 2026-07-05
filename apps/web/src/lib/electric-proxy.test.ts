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
