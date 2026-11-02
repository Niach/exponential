import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1608: the check-runs read, driven by a fake fetch (no DB, no GitHub).
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn() }))
vi.mock(`@/lib/sessions/messages`, () => ({ insertMessage: vi.fn(), inSession: vi.fn() }))

import { registerGithubToken, resetGithubClientForTests } from "@/lib/integrations/github-client"
import {
  CHECKS_DENIED_TTL_MS,
  checksDenied,
  fetchGithubCheckRuns,
  requiredChecksFor,
  resetPrChecksForTests,
} from "@/lib/integrations/pr-checks"

const NOW = 1_800_000_000_000
const RESET_S = NOW / 1000 + 600

type Reply = { status: number; body?: unknown; headers?: Record<string, string> }

function fakeFetch(replies: Reply[]) {
  const urls: string[] = []
  const fetch = async (url: string) => {
    urls.push(url)
    const reply = replies.shift()
    if (!reply) throw new Error(`no more replies`)
    return new Response(JSON.stringify(reply.body ?? {}), {
      status: reply.status,
      headers: {
        "x-ratelimit-remaining": `4000`,
        "x-ratelimit-limit": `5000`,
        "x-ratelimit-reset": String(RESET_S),
        ...reply.headers,
      },
    })
  }
  return { fetch, urls }
}

const denied: Reply = { status: 403, body: { message: `Resource not accessible by integration` } }

function run(name: string, sha = `sha1`) {
  return { name, status: `completed`, conclusion: `success`, head_sha: sha, started_at: null, completed_at: null }
}

describe(`fetchGithubCheckRuns`, () => {
  beforeEach(() => {
    resetGithubClientForTests()
    resetPrChecksForTests()
    registerGithubToken(`tok-a`, 1)
    registerGithubToken(`tok-a2`, 1)
    registerGithubToken(`tok-b`, 2)
  })

  it(`negative-caches a permission 403 per installation and warns once`, async () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    const { fetch, urls } = fakeFetch([denied])
    const now = () => NOW
    expect(await fetchGithubCheckRuns(`acme/app`, `sha1`, `tok-a`, { fetch, now })).toBeNull()
    // Same installation, a rotated token, another repo: no call.
    expect(await fetchGithubCheckRuns(`acme/other`, `sha2`, `tok-a2`, { fetch, now })).toBeNull()
    expect(urls).toHaveLength(1)
    expect(checksDenied(`tok-a`, NOW)).toBe(true)
    expect(checksDenied(`tok-b`, NOW)).toBe(false)
    expect(warn).toHaveBeenCalledTimes(1)
    expect(String(warn.mock.calls[0]?.[0])).toContain(`Checks: Read-only`)
    // The required-checks reads are skipped too (no checks known anyway).
    expect(await requiredChecksFor(`acme/app`, `master`, `tok-a`)).toEqual(new Set())
    warn.mockRestore()
  })

  it(`asks again once the negative cache expires`, async () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    const { fetch, urls } = fakeFetch([denied, { status: 200, body: { total_count: 1, check_runs: [run(`gate`)] } }])
    expect(await fetchGithubCheckRuns(`acme/app`, `sha1`, `tok-a`, { fetch, now: () => NOW })).toBeNull()
    const later = NOW + CHECKS_DENIED_TTL_MS + 1
    const read = await fetchGithubCheckRuns(`acme/app`, `sha1`, `tok-a`, { fetch, now: () => later })
    expect(read?.checks.map((check) => check.name)).toEqual([`gate`])
    expect(urls).toHaveLength(2)
    warn.mockRestore()
  })

  it(`a rate-limit 403 is no permission refusal (nothing cached)`, async () => {
    const limited: Reply = { status: 403, headers: { "x-ratelimit-remaining": `0` } }
    const { fetch } = fakeFetch([limited, limited, limited])
    const read = await fetchGithubCheckRuns(`acme/app`, `sha1`, `tok-a`, {
      fetch,
      now: () => NOW,
      sleep: async () => {},
    })
    expect(read).toBeNull()
    expect(checksDenied(`tok-a`, NOW)).toBe(false)
  })

  it(`paginates past 100 runs into one complete snapshot`, async () => {
    const page1 = Array.from({ length: 100 }, (_, i) => run(`c${i}`))
    const { fetch, urls } = fakeFetch([
      { status: 200, body: { total_count: 101, check_runs: page1 } },
      { status: 200, body: { total_count: 101, check_runs: [run(`c100`)] } },
    ])
    const read = await fetchGithubCheckRuns(`acme/app`, `sha1`, `tok-a`, { fetch, now: () => NOW })
    expect(read?.checks).toHaveLength(101)
    expect(read?.complete).toBe(true)
    expect(read?.headSha).toBe(`sha1`)
    expect(urls[1]).toContain(`page=2`)
  })

  it(`marks a list cut at the page cap as incomplete (no snapshot delete)`, async () => {
    const full = (p: number) => ({
      status: 200,
      body: { total_count: 900, check_runs: Array.from({ length: 100 }, (_, i) => run(`p${p}c${i}`)) },
    })
    const { fetch, urls } = fakeFetch([1, 2, 3, 4, 5].map(full))
    const read = await fetchGithubCheckRuns(`acme/app`, `sha1`, `tok-a`, { fetch, now: () => NOW })
    expect(urls).toHaveLength(5)
    expect(read?.complete).toBe(false)
    expect(read?.checks).toHaveLength(500)
  })

  it(`reads at the given sha`, async () => {
    const { fetch, urls } = fakeFetch([{ status: 200, body: { total_count: 0, check_runs: [] } }])
    const read = await fetchGithubCheckRuns(`acme/app`, `deadbeef`, `tok-a`, { fetch, now: () => NOW })
    expect(urls[0]).toContain(`/repos/acme/app/commits/deadbeef/check-runs`)
    expect(read).toEqual({ headSha: null, checks: [], complete: true })
  })
})
