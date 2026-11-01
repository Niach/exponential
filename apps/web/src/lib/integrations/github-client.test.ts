import { beforeEach, describe, expect, it } from "vitest"
import {
  GithubRateLimited,
  githubBudgetKey,
  githubBudgetLow,
  githubBudgetLowForToken,
  githubBudgets,
  githubFetch,
  registerGithubToken,
  resetGithubClientForTests,
} from "@/lib/integrations/github-client"

// EXP-1609: the one GitHub REST client, driven by a fake fetch.

const NOW = 1_800_000_000_000
const RESET_S = NOW / 1000 + 600

type Reply = { status: number; body?: string; headers?: Record<string, string> }

function fakeFetch(replies: Reply[]) {
  const calls: Array<{ url: string; headers: Record<string, string>; method: string }> = []
  const fetch = async (url: string, init: RequestInit) => {
    calls.push({
      url,
      method: String(init.method),
      headers: { ...(init.headers as Record<string, string>) },
    })
    const reply = replies.shift()
    if (!reply) throw new Error(`no more replies`)
    return new Response(reply.status === 304 ? null : (reply.body ?? `{}`), {
      status: reply.status,
      headers: reply.headers,
    })
  }
  return { fetch, calls }
}

function deps(fetch: ReturnType<typeof fakeFetch>[`fetch`], sleeps: number[] = []) {
  return {
    fetch,
    sleep: async (ms: number) => {
      sleeps.push(ms)
    },
    now: () => NOW,
    random: () => 0,
  }
}

const auth = (token: string) => ({ headers: { authorization: `Bearer ${token}` } })

beforeEach(() => resetGithubClientForTests())

describe(`githubFetch conditional requests`, () => {
  it(`sends If-None-Match and serves a 304 from the stored body`, async () => {
    const fake = fakeFetch([
      { status: 200, body: `{"n":1}`, headers: { etag: `"abc"` } },
      { status: 304, headers: { etag: `"abc"` } },
    ])
    const first = await githubFetch(`/repos/a/b/pulls/1`, auth(`tok`), deps(fake.fetch))
    expect(await first.json()).toEqual({ n: 1 })
    const second = await githubFetch(`/repos/a/b/pulls/1`, auth(`tok`), deps(fake.fetch))
    expect(second.status).toBe(200)
    expect(await second.json()).toEqual({ n: 1 })
    expect(fake.calls[0]!.headers[`if-none-match`]).toBeUndefined()
    expect(fake.calls[1]!.headers[`if-none-match`]).toBe(`"abc"`)
  })

  it(`shares the cache across rotated tokens of one installation`, async () => {
    registerGithubToken(`old`, 7)
    registerGithubToken(`new`, 7)
    const fake = fakeFetch([
      { status: 200, body: `[]`, headers: { etag: `"e"` } },
      { status: 304 },
    ])
    await githubFetch(`/repos/a/b/branches/x`, auth(`old`), deps(fake.fetch))
    await githubFetch(`/repos/a/b/branches/x`, auth(`new`), deps(fake.fetch))
    expect(fake.calls[1]!.headers[`if-none-match`]).toBe(`"e"`)
  })

  it(`never makes a write conditional`, async () => {
    const fake = fakeFetch([
      { status: 200, body: `{}`, headers: { etag: `"w"` } },
      { status: 200, body: `{}` },
    ])
    await githubFetch(`/repos/a/b/pulls/1`, { method: `PATCH`, body: `{}` }, deps(fake.fetch))
    await githubFetch(`/repos/a/b/pulls/1`, { method: `PATCH`, body: `{}` }, deps(fake.fetch))
    expect(fake.calls[1]!.headers[`if-none-match`]).toBeUndefined()
  })
})

describe(`githubFetch retries`, () => {
  it(`honours retry-after on a 429`, async () => {
    const sleeps: number[] = []
    const fake = fakeFetch([
      { status: 429, headers: { "retry-after": `2` } },
      { status: 200, body: `{"ok":true}` },
    ])
    const res = await githubFetch(`/repos/a/b`, auth(`t`), deps(fake.fetch, sleeps))
    expect(res.status).toBe(200)
    expect(sleeps).toEqual([2000])
  })

  it(`retries GitHub's secondary limit (403 with budget left)`, async () => {
    const sleeps: number[] = []
    const fake = fakeFetch([
      {
        status: 403,
        body: `{"message":"You have exceeded a secondary rate limit."}`,
        headers: { "x-ratelimit-remaining": `4000` },
      },
      { status: 200 },
    ])
    const res = await githubFetch(`/repos/a/b`, auth(`t`), deps(fake.fetch, sleeps))
    expect(res.status).toBe(200)
    expect(sleeps).toHaveLength(1)
    expect(sleeps[0]).toBeGreaterThanOrEqual(1000)
  })

  it(`hands a plain 403 straight back (a permission refusal is not retried)`, async () => {
    const fake = fakeFetch([
      { status: 403, body: `{"message":"Resource not accessible by integration"}`, headers: { "x-ratelimit-remaining": `4000` } },
    ])
    const res = await githubFetch(`/repos/a/b`, auth(`t`), deps(fake.fetch))
    expect(res.status).toBe(403)
    expect(fake.calls).toHaveLength(1)
  })

  it(`throws GithubRateLimited { resetAt } when the reset is far away`, async () => {
    const fake = fakeFetch([
      { status: 403, headers: { "x-ratelimit-remaining": `0`, "x-ratelimit-reset": String(RESET_S), "x-ratelimit-limit": `5000` } },
    ])
    const err = await githubFetch(`/repos/a/b`, auth(`t`), deps(fake.fetch)).catch((e) => e)
    expect(err).toBeInstanceOf(GithubRateLimited)
    expect((err as GithubRateLimited).resetAt?.getTime()).toBe(RESET_S * 1000)
    expect((err as Error).message).toMatch(/rate limit reached.*in ~10 min/)
    expect(fake.calls).toHaveLength(1)
  })

  it(`gives up after bounded backoff on a limit with no hint`, async () => {
    const sleeps: number[] = []
    const fake = fakeFetch([
      { status: 429 },
      { status: 429 },
      { status: 429 },
    ])
    const err = await githubFetch(`/repos/a/b`, auth(`t`), deps(fake.fetch, sleeps)).catch((e) => e)
    expect(err).toBeInstanceOf(GithubRateLimited)
    expect(fake.calls).toHaveLength(3)
    expect(sleeps).toEqual([500, 1000])
  })

  it(`retries a 5xx on a GET, never on a write`, async () => {
    const fake = fakeFetch([{ status: 502 }, { status: 200 }])
    expect((await githubFetch(`/x`, {}, deps(fake.fetch))).status).toBe(200)
    const write = fakeFetch([{ status: 502 }])
    expect((await githubFetch(`/x`, { method: `PUT`, body: `{}` }, deps(write.fetch))).status).toBe(502)
    expect(write.calls).toHaveLength(1)
  })
})

describe(`budget`, () => {
  it(`records remaining/reset per installation and reports low`, async () => {
    registerGithubToken(`inst-tok`, 42)
    const fake = fakeFetch([
      { status: 200, headers: { "x-ratelimit-remaining": `90`, "x-ratelimit-limit": `5000`, "x-ratelimit-reset": String(RESET_S) } },
    ])
    await githubFetch(`/repos/a/b`, auth(`inst-tok`), deps(fake.fetch))
    expect(githubBudgetKey(`inst-tok`)).toBe(`installation:42`)
    expect(githubBudgetLow(`installation:42`, NOW)).toBe(true)
    expect(githubBudgetLowForToken(`inst-tok`, NOW)).toBe(true)
    // after the reset the budget is full again
    expect(githubBudgetLow(`installation:42`, RESET_S * 1000 + 1)).toBe(false)
    expect(githubBudgets(NOW)).toEqual([
      expect.objectContaining({ key: `installation:42`, remaining: 90, limit: 5000 }),
    ])
  })

  it(`a healthy budget is not low, and an unseen one never is`, async () => {
    const fake = fakeFetch([
      { status: 200, headers: { "x-ratelimit-remaining": `4000`, "x-ratelimit-limit": `5000`, "x-ratelimit-reset": String(RESET_S) } },
    ])
    await githubFetch(`/repos/a/b`, auth(`pat`), deps(fake.fetch))
    expect(githubBudgetLowForToken(`pat`, NOW)).toBe(false)
    expect(githubBudgetLowForToken(`never-seen`, NOW)).toBe(false)
  })

  it(`never keys a budget by the raw token`, () => {
    expect(githubBudgetKey(`ghp_secret`)).toMatch(/^token:[0-9a-f]{8}$/)
    expect(githubBudgetKey(null)).toBe(`anon`)
    expect(githubBudgetKey(`a.b.c`)).toBe(`app`)
  })
})
