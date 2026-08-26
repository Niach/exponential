import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  fetchPullFiles,
  githubRateLimitMessage,
  listOpenPulls,
  type GitHubFetch,
} from "@/lib/integrations/github-pr"

function headers(map: Record<string, string>) {
  return { get: (name: string) => map[name.toLowerCase()] ?? null }
}

function filesResponse(body: unknown) {
  return {
    ok: true,
    status: 200,
    headers: headers({}),
    text: async () => JSON.stringify(body),
    json: async () => body,
  }
}

function errorResponse(status: number, map: Record<string, string>) {
  return {
    ok: false,
    status,
    headers: headers(map),
    text: async () => ``,
    json: async () => ({}),
  }
}

const FILE = {
  filename: `apps/web/src/app.tsx`,
  status: `modified`,
  additions: 3,
  deletions: 1,
  patch: `@@ -1 +1 @@`,
}

// The env var flips the "authed" posture of every helper below, and a real
// shell may well have one exported.
const savedToken = process.env.GITHUB_TOKEN
beforeEach(() => {
  delete process.env.GITHUB_TOKEN
})
afterEach(() => {
  if (savedToken === undefined) delete process.env.GITHUB_TOKEN
  else process.env.GITHUB_TOKEN = savedToken
})

describe(`githubRateLimitMessage`, () => {
  it(`returns null for a status that is not a rate limit`, () => {
    expect(
      githubRateLimitMessage(
        { status: 404, headers: headers({ "x-ratelimit-remaining": `0` }) },
        `Niach/exponential`,
        false
      )
    ).toBeNull()
  })

  // A 403 with budget left is a permission problem (private repo, no token) —
  // telling the user to wait would be wrong.
  it(`returns null for a 403 with budget remaining`, () => {
    expect(
      githubRateLimitMessage(
        { status: 403, headers: headers({ "x-ratelimit-remaining": `12` }) },
        `Niach/exponential`,
        false
      )
    ).toBeNull()
  })

  it(`reports the reset delay in minutes and points at GITHUB_TOKEN when unauthenticated`, () => {
    const reset = Math.floor((Date.now() + 7 * 60_000) / 1000)
    expect(
      githubRateLimitMessage(
        {
          status: 403,
          headers: headers({
            "x-ratelimit-remaining": `0`,
            "x-ratelimit-reset": String(reset),
          }),
        },
        `Niach/exponential`,
        false
      )
    ).toBe(
      `GitHub rate limit reached for Niach/exponential. Try again in ~7 min — or set GITHUB_TOKEN on the server for public-repo reads.`
    )
  })

  it(`drops the GITHUB_TOKEN hint when the call was authenticated`, () => {
    const reset = Math.floor((Date.now() + 60_000) / 1000)
    expect(
      githubRateLimitMessage(
        {
          status: 429,
          headers: headers({
            "x-ratelimit-remaining": `0`,
            "x-ratelimit-reset": String(reset),
          }),
        },
        `Niach/exponential`,
        true
      )
    ).toBe(`GitHub rate limit reached for Niach/exponential. Try again in ~1 min.`)
  })

  it(`falls back to "Try again later" without a reset header`, () => {
    expect(
      githubRateLimitMessage(
        { status: 403, headers: headers({ "x-ratelimit-remaining": `0` }) },
        `Niach/exponential`,
        true
      )
    ).toBe(`GitHub rate limit reached for Niach/exponential. Try again later.`)
  })
})

describe(`fetchPullFiles`, () => {
  it(`serves a repeat read from the 60s cache instead of hitting GitHub again`, async () => {
    const doFetch = vi.fn(async () =>
      filesResponse([FILE])
    ) as unknown as GitHubFetch
    const first = await fetchPullFiles(`Niach/cache-hit`, 1, null, doFetch)
    const second = await fetchPullFiles(`Niach/cache-hit`, 1, null, doFetch)
    expect(first).toEqual([FILE])
    expect(second).toEqual([FILE])
    expect(doFetch).toHaveBeenCalledTimes(1)
  })

  it(`keys the cache on the PR and the auth posture`, async () => {
    const doFetch = vi.fn(async () =>
      filesResponse([FILE])
    ) as unknown as GitHubFetch
    await fetchPullFiles(`Niach/cache-key`, 1, null, doFetch)
    await fetchPullFiles(`Niach/cache-key`, 2, null, doFetch)
    await fetchPullFiles(`Niach/cache-key`, 1, `ghs_token`, doFetch)
    expect(doFetch).toHaveBeenCalledTimes(3)
  })

  // A rejected entry must evict — a transient 500 that stuck for 60s would be
  // worse than the rate limit this cache exists to avoid.
  it(`does not cache a failure`, async () => {
    const doFetch = vi
      .fn()
      .mockResolvedValueOnce(errorResponse(500, {}))
      .mockResolvedValueOnce(filesResponse([FILE])) as unknown as GitHubFetch
    await expect(
      fetchPullFiles(`Niach/cache-fail`, 1, null, doFetch)
    ).rejects.toThrow(`GitHub returned 500`)
    await expect(
      fetchPullFiles(`Niach/cache-fail`, 1, null, doFetch)
    ).resolves.toEqual([FILE])
    expect(doFetch).toHaveBeenCalledTimes(2)
  })

  it(`surfaces the readable rate-limit message`, async () => {
    const reset = Math.floor((Date.now() + 3 * 60_000) / 1000)
    const doFetch = vi.fn(async () =>
      errorResponse(403, {
        "x-ratelimit-remaining": `0`,
        "x-ratelimit-reset": String(reset),
      })
    ) as unknown as GitHubFetch
    await expect(
      fetchPullFiles(`Niach/rate-limited`, 9, null, doFetch)
    ).rejects.toThrow(
      `GitHub rate limit reached for Niach/rate-limited. Try again in ~3 min — or set GITHUB_TOKEN on the server for public-repo reads.`
    )
  })
})

describe(`listOpenPulls`, () => {
  it(`surfaces the readable rate-limit message`, async () => {
    const reset = Math.floor((Date.now() + 2 * 60_000) / 1000)
    const doFetch = vi.fn(async () =>
      errorResponse(403, {
        "x-ratelimit-remaining": `0`,
        "x-ratelimit-reset": String(reset),
      })
    ) as unknown as GitHubFetch
    await expect(listOpenPulls(`Niach/rate-limited`, null, doFetch)).rejects.toThrow(
      `GitHub rate limit reached for Niach/rate-limited. Try again in ~2 min — or set GITHUB_TOKEN on the server for public-repo reads.`
    )
  })

  it(`keeps the plain status error for a non-rate-limit failure`, async () => {
    const doFetch = vi.fn(async () =>
      errorResponse(404, {})
    ) as unknown as GitHubFetch
    await expect(listOpenPulls(`Niach/gone`, null, doFetch)).rejects.toThrow(
      `GitHub returned 404 listing pulls for Niach/gone`
    )
  })
})
