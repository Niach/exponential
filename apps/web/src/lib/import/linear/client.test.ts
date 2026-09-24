import { describe, expect, it } from "vitest"
import {
  LinearApiError,
  LinearClient,
  LINEAR_MAX_QUERY_COMPLEXITY,
  paginate,
  type LinearFetch,
} from "@/lib/import/linear/client"

// EXP-630: the Linear GraphQL client's contract — raw-key auth, the
// rate-limit headers, backoff on 429 / RATELIMITED / 5xx, a typed
// complexity error, and the paginator's page-size halving.

type Call = { url: string; init: Parameters<LinearFetch>[1] }

function response(
  status: number,
  body: unknown,
  headers: Record<string, string> = {}
) {
  const map = new Map(Object.entries(headers).map(([key, value]) => [key.toLowerCase(), value]))
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: (name: string) => map.get(name.toLowerCase()) ?? null },
    text: async () => (typeof body === `string` ? body : JSON.stringify(body)),
  }
}

function harness(responses: ReturnType<typeof response>[]) {
  const calls: Call[] = []
  const sleeps: number[] = []
  const queue = [...responses]
  const fetchImpl: LinearFetch = async (url, init) => {
    calls.push({ url, init })
    const next = queue.shift()
    if (!next) throw new TypeError(`fetch failed`)
    return next
  }
  const client = new LinearClient(`lin_api_test`, {
    fetchImpl,
    sleep: async (ms) => {
      sleeps.push(ms)
    },
    now: () => 1_000_000,
    maxAttempts: 3,
  })
  return { client, calls, sleeps }
}

// Measured on the Methode 5 workspace (2026-09-23): a 50-issue page with
// nested history. The assertion pins it under the per-query cap.
const MEASURED_ISSUE_PAGE_COMPLEXITY = 288

describe(`LinearClient`, () => {
  it(`posts the raw key as Authorization (no Bearer) and records the rate-limit headers`, async () => {
    const { client, calls } = harness([
      response(
        200,
        { data: { viewer: { id: `u` } } },
        {
          "x-complexity": String(MEASURED_ISSUE_PAGE_COMPLEXITY),
          "x-ratelimit-requests-remaining": `2499`,
          "x-ratelimit-requests-reset": `1790180483151`,
          "x-ratelimit-complexity-remaining": `2999712`,
          "x-ratelimit-complexity-reset": `1790180483151`,
        }
      ),
    ])
    const data = await client.query<{ viewer: { id: string } }>(`{ viewer { id } }`)
    expect(data.viewer.id).toBe(`u`)
    expect(calls[0]!.url).toBe(`https://api.linear.app/graphql`)
    expect(calls[0]!.init.headers.authorization).toBe(`lin_api_test`)
    expect(calls[0]!.init.headers.authorization).not.toMatch(/^Bearer/)
    expect(JSON.parse(calls[0]!.init.body!)).toEqual({ query: `{ viewer { id } }`, variables: {} })
    expect(client.rateLimit).toEqual({
      requestsRemaining: 2499,
      requestsResetMs: 1790180483151,
      complexityRemaining: 2999712,
      complexityResetMs: 1790180483151,
      complexity: MEASURED_ISSUE_PAGE_COMPLEXITY,
    })
    expect(client.complexityLog).toEqual([MEASURED_ISSUE_PAGE_COMPLEXITY])
    expect(MEASURED_ISSUE_PAGE_COMPLEXITY).toBeLessThan(LINEAR_MAX_QUERY_COMPLEXITY)
  })

  it(`waits for the reset on a 429, then retries`, async () => {
    const { client, sleeps, calls } = harness([
      response(429, ``, { "x-ratelimit-requests-reset": `1005000` }),
      response(200, { data: { ok: true } }),
    ])
    await client.query(`{ ok }`)
    expect(sleeps).toEqual([5_000])
    expect(calls).toHaveLength(2)
  })

  it(`waits before the next request once the remaining budget is nearly gone`, async () => {
    const { client, sleeps } = harness([
      response(200, { data: { a: 1 } }, { "x-ratelimit-requests-remaining": `1`, "x-ratelimit-requests-reset": `1003000` }),
      response(200, { data: { b: 2 } }, { "x-ratelimit-requests-remaining": `2499` }),
    ])
    await client.query(`{ a }`)
    await client.query(`{ b }`)
    expect(sleeps).toEqual([3_000])
  })

  it(`fails fast on an auth error and never retries it`, async () => {
    const { client, calls } = harness([response(401, { errors: [{ message: `Authentication required` }] })])
    await expect(client.query(`{ viewer { id } }`)).rejects.toMatchObject({ kind: `auth`, status: 401 })
    expect(calls).toHaveLength(1)
  })

  it(`retries a 5xx with backoff and gives up after maxAttempts`, async () => {
    const { client, sleeps } = harness([response(502, ``), response(200, { data: { ok: true } })])
    await client.query(`{ ok }`)
    expect(sleeps).toEqual([1_000])

    const dead = harness([response(503, ``), response(503, ``), response(503, ``)])
    await expect(dead.client.query(`{ ok }`)).rejects.toMatchObject({ kind: `http`, status: 503 })
  })

  it(`treats a RATELIMITED GraphQL error like a 429 and a complexity error as typed`, async () => {
    const { client, sleeps } = harness([
      response(
        200,
        { errors: [{ message: `Rate limit exceeded`, extensions: { code: `RATELIMITED` } }] },
        { "x-ratelimit-complexity-reset": `1002000` }
      ),
      response(200, { data: { ok: true } }),
    ])
    await client.query(`{ ok }`)
    expect(sleeps).toEqual([2_000])

    const complex = harness([
      response(200, { errors: [{ message: `Query too complex. Complexity 15920 > 10000` }] }),
    ])
    await expect(complex.client.query(`{ big }`)).rejects.toMatchObject({ kind: `complexity` })
  })

  it(`refuses to wait past the cap`, async () => {
    const { client } = harness([response(429, ``, { "x-ratelimit-requests-reset": `9000000000` })])
    await expect(client.query(`{ ok }`)).rejects.toBeInstanceOf(LinearApiError)
  })
})

describe(`paginate`, () => {
  it(`walks cursors and halves the page size on a complexity error, keeping the cursor`, async () => {
    const seen: [number, string | null][] = []
    const pages = paginate<number>(async (first, after) => {
      seen.push([first, after])
      if (first > 25) throw new LinearApiError(`too complex`, `complexity`)
      if (after === null) return { nodes: [1, 2], pageInfo: { hasNextPage: true, endCursor: `c1` } }
      return { nodes: [3], pageInfo: { hasNextPage: false, endCursor: null } }
    })
    const collected: number[] = []
    for await (const page of pages) collected.push(...page)
    expect(collected).toEqual([1, 2, 3])
    expect(seen).toEqual([
      [50, null],
      [25, null],
      [25, `c1`],
    ])
  })

  it(`rethrows a complexity error once the page size is at its minimum`, async () => {
    const pages = paginate<number>(
      async () => {
        throw new LinearApiError(`too complex`, `complexity`)
      },
      { pageSize: 5 }
    )
    await expect(pages.next()).rejects.toMatchObject({ kind: `complexity` })
  })
})
