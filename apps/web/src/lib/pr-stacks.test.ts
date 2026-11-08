import { describe, expect, it, vi } from "vitest"
import { inferBaseBranch, MAX_BASE_CANDIDATES } from "@/lib/pr-stacks"

// EXP-1248: the inferred base of a stacked PR. Routes match on METHOD + exact PATH (the
// part after /repos/o/r), consumed in order, so `/pulls/24` never answers
// `/pulls/241`.

interface Route {
  method?: string
  path: string
  status: number
  body?: unknown
}

function github(routes: Route[]) {
  const queue = [...routes]
  const calls: Array<{ method: string; path: string; body?: unknown; headers: Record<string, string> }> = []
  const impl = vi.fn(
    async (
      url: string,
      init: { method?: string; body?: string; headers: Record<string, string> }
    ) => {
      const method = init.method ?? `GET`
      const path = url.replace(`https://api.github.com/repos/o/r`, ``).split(`?`)[0]!
      calls.push({
        method,
        path: url.replace(`https://api.github.com/repos/o/r`, ``),
        body: init.body ? JSON.parse(init.body) : undefined,
        headers: init.headers,
      })
      const index = queue.findIndex(
        (route) => route.path === path && (route.method ?? `GET`) === method
      )
      if (index < 0) throw new Error(`unrouted ${method} ${url}`)
      const [route] = queue.splice(index, 1)
      const status = route!.status
      return {
        ok: status >= 200 && status < 300,
        status,
        text: async () => JSON.stringify(route!.body ?? {}),
        json: async () => route!.body ?? {},
      }
    }
  )
  return { impl: impl as never, calls }
}

describe(`inferBaseBranch`, () => {
  it(`picks the nearest open PR whose head is an ancestor of the new head`, async () => {
    const { impl, calls } = github([
      { path: `/compare/exp%2FEXP-1239...exp%2FEXP-1244`, status: 200, body: { status: `ahead`, ahead_by: 2 } },
      { path: `/compare/exp%2FEXP-1200...exp%2FEXP-1244`, status: 200, body: { status: `diverged`, ahead_by: 2, behind_by: 5 } },
      { path: `/compare/exp%2FEXP-1100...exp%2FEXP-1244`, status: 200, body: { status: `ahead`, ahead_by: 9 } },
    ])
    const result = await inferBaseBranch({
      repo: `o/r`,
      token: `tok`,
      head: `exp/EXP-1244`,
      defaultBranch: `master`,
      candidates: [
        { number: 1011, branch: `exp/EXP-1239` },
        { number: 1000, branch: `exp/EXP-1200` },
        { number: 990, branch: `exp/EXP-1100` },
        { number: 1012, branch: `exp/EXP-1244` }, // itself
        { number: 5, branch: `master` }, // a release PR from the default branch
      ],
      fetchImpl: impl,
    })
    expect(result).toEqual({ base: `exp/EXP-1239`, prNumber: 1011 })
    expect(calls).toHaveLength(3)
  })

  it(`falls back to the default branch, skipping a candidate whose branch is gone`, async () => {
    const { impl } = github([
      { path: `/compare/exp%2Fgone...exp%2Fnew`, status: 404, body: { message: `Not Found` } },
      { path: `/compare/exp%2Fsame...exp%2Fnew`, status: 200, body: { status: `identical`, ahead_by: 0 } },
    ])
    const result = await inferBaseBranch({
      repo: `o/r`,
      token: `tok`,
      head: `exp/new`,
      defaultBranch: `master`,
      candidates: [
        { number: 1, branch: `exp/gone` },
        { number: 2, branch: `exp/same` },
      ],
      fetchImpl: impl,
    })
    expect(result).toEqual({ base: `master`, prNumber: null })
  })

  it(`skips the repo's development branches and a compare GitHub fails with a 5xx`, async () => {
    const { impl, calls } = github([
      { path: `/compare/exp%2Fflaky...exp%2Fnew`, status: 502, body: { message: `Bad Gateway` } },
      { path: `/compare/exp%2Fa...exp%2Fnew`, status: 200, body: { status: `ahead`, ahead_by: 3 } },
    ])
    const result = await inferBaseBranch({
      repo: `o/r`,
      token: `tok`,
      head: `exp/new`,
      defaultBranch: `main`,
      stopBranches: [`develop`, `release/1.x`],
      candidates: [
        { number: 1, branch: `exp/flaky` },
        { number: 2, branch: `exp/a` },
        { number: 3, branch: `develop` }, // the team's pin: a release PR
        { number: 4, branch: `release/1.x` }, // a board's pin
      ],
      fetchImpl: impl,
    })
    expect(result).toEqual({ base: `exp/a`, prNumber: 2 })
    expect(calls).toHaveLength(2)
  })

  it(`throws any other compare failure`, async () => {
    const { impl } = github([
      { path: `/compare/exp%2Fa...exp%2Fnew`, status: 403, body: { message: `Forbidden` } },
    ])
    await expect(
      inferBaseBranch({
        repo: `o/r`,
        token: `tok`,
        head: `exp/new`,
        defaultBranch: `main`,
        candidates: [{ number: 1, branch: `exp/a` }],
        fetchImpl: impl,
      })
    ).rejects.toThrow(/403/)
  })

  it(`compares only the newest candidates`, async () => {
    const candidates = Array.from({ length: MAX_BASE_CANDIDATES + 5 }, (_, i) => ({
      number: i + 1,
      branch: `exp/c${i + 1}`,
    }))
    const { impl, calls } = github(
      candidates.map((c) => ({
        path: `/compare/exp%2Fc${c.number}...exp%2Fnew`,
        status: 200,
        body: { status: `diverged`, ahead_by: 1, behind_by: 1 },
      }))
    )
    await inferBaseBranch({ repo: `o/r`, token: `tok`, head: `exp/new`, defaultBranch: `main`, candidates, fetchImpl: impl })
    expect(calls).toHaveLength(MAX_BASE_CANDIDATES)
    expect(calls.some((call) => call.path.startsWith(`/compare/exp%2Fc1...`))).toBe(false)
  })

  it(`lists the repo's open PRs when no candidates are given`, async () => {
    const { impl } = github([
      { path: `/pulls`, status: 200, body: [{ number: 7, html_url: `u`, title: `t`, created_at: `x`, head: { ref: `exp/a` }, base: { ref: `master` } }] },
      { path: `/compare/exp%2Fa...exp%2Fb`, status: 200, body: { status: `ahead`, ahead_by: 1 } },
    ])
    const result = await inferBaseBranch({ repo: `o/r`, token: `tok`, head: `exp/b`, defaultBranch: `master`, fetchImpl: impl })
    expect(result).toEqual({ base: `exp/a`, prNumber: 7 })
  })
})
