import { describe, expect, it, vi } from "vitest"
import {
  ensureGithubStack,
  GitHubStackForkError,
  inferBaseBranch,
  MAX_BASE_CANDIDATES,
  mergeThrough,
} from "@/lib/pr-stacks"
import { GitHubStackError } from "@/lib/integrations/github-pr"

// EXP-1248: native GitHub stacks. Routes match on METHOD + exact PATH (the
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

const stack = (number: number, members: number[], extra: object = {}) => ({
  number,
  open: true,
  base: { ref: `master` },
  pull_requests: members.map((n) => ({ number: n, state: `open`, merged_at: null, head: { ref: `exp/${n}` } })),
  ...extra,
})

const pull = (n: number, base: string) => ({
  state: `open`,
  merged: false,
  head: { ref: `exp/${n}` },
  base: { ref: base },
})

describe(`ensureGithubStack`, () => {
  it(`creates a two-PR stack bottom → top when the lower PR sits on the default branch`, async () => {
    const { impl, calls } = github([
      { path: `/stacks`, status: 200, body: [] },
      { path: `/pulls/1011`, status: 200, body: pull(1011, `master`) },
      { method: `POST`, path: `/stacks`, status: 201, body: stack(7, [1011, 1012]) },
    ])
    const result = await ensureGithubStack({
      repo: `o/r`,
      token: `tok`,
      lowerPrNumber: 1011,
      newPrNumber: 1012,
      stopBranches: [`master`],
      fetchImpl: impl,
    })
    expect(result.number).toBe(7)
    const create = calls.find((call) => call.method === `POST`)!
    expect(create.body).toEqual({ pull_requests: [1011, 1012] })
    expect(create.headers[`x-github-api-version`]).toBe(`2026-03-10`)
    expect(calls[0]!.path).toBe(`/stacks?pull_request=1011&per_page=100`)
  })

  it(`adds the new PR on top of the stack the lower PR already tops`, async () => {
    const { impl, calls } = github([
      { path: `/stacks`, status: 200, body: [stack(7, [1011, 1012])] },
      { method: `POST`, path: `/stacks/7/add`, status: 200, body: stack(7, [1011, 1012, 1013]) },
    ])
    const result = await ensureGithubStack({
      repo: `o/r`,
      token: `tok`,
      lowerPrNumber: 1012,
      newPrNumber: 1013,
      fetchImpl: impl,
    })
    expect(result.pulls.map((p) => p.number)).toEqual([1011, 1012, 1013])
    expect(calls.at(-1)!.body).toEqual({ pull_requests: [1013] })
  })

  it(`is a no-op when the new PR is already stacked on it`, async () => {
    const { impl, calls } = github([
      { path: `/stacks`, status: 200, body: [stack(7, [1011, 1012])] },
    ])
    await ensureGithubStack({ repo: `o/r`, token: `tok`, lowerPrNumber: 1011, newPrNumber: 1012, fetchImpl: impl })
    expect(calls).toHaveLength(1)
  })

  it(`refuses a fork: the lower PR already has another PR stacked on it`, async () => {
    const { impl } = github([
      { path: `/stacks`, status: 200, body: [stack(7, [1011, 1012])] },
    ])
    const error = await ensureGithubStack({
      repo: `o/r`,
      token: `tok`,
      lowerPrNumber: 1011,
      newPrNumber: 1020,
      fetchImpl: impl,
    }).catch((err: unknown) => err)
    expect(error).toBeInstanceOf(GitHubStackForkError)
    expect((error as GitHubStackForkError).status).toBe(409)
    expect((error as Error).message).toContain(`linear`)
  })

  it(`calls a PR below that is mid-stack a fork too`, async () => {
    const { impl } = github([
      { path: `/stacks`, status: 200, body: [] }, // #1008
      { path: `/pulls/1008`, status: 200, body: pull(1008, `exp/1006`) },
      { path: `/pulls`, status: 200, body: [{ number: 1006, html_url: `u`, base: { ref: `master` } }] },
      { path: `/stacks`, status: 200, body: [stack(7, [1006, 1007])] }, // #1006 carries #1007
    ])
    const error = await ensureGithubStack({
      repo: `o/r`,
      token: `tok`,
      lowerPrNumber: 1008,
      newPrNumber: 1009,
      fetchImpl: impl,
    }).catch((err: unknown) => err)
    expect(error).toBeInstanceOf(GitHubStackForkError)
  })

  it(`recurses down an unstacked chain and stacks the whole line in one call`, async () => {
    // VAPP-88 #1004 ← VAPP-89 #1006 ← VAPP-91 #1008; the new PR #1009 sits on #1008.
    const { impl, calls } = github([
      { path: `/stacks`, status: 200, body: [] }, // #1008
      { path: `/pulls/1008`, status: 200, body: pull(1008, `exp/VAPP-89`) },
      { path: `/pulls`, status: 200, body: [{ number: 1006, html_url: `u`, base: { ref: `exp/VAPP-88` } }] },
      { path: `/stacks`, status: 200, body: [] }, // #1006
      { path: `/pulls/1006`, status: 200, body: pull(1006, `exp/VAPP-88`) },
      { path: `/pulls`, status: 200, body: [{ number: 1004, html_url: `u`, base: { ref: `master` } }] },
      { path: `/stacks`, status: 200, body: [] }, // #1004
      { path: `/pulls/1004`, status: 200, body: pull(1004, `master`) },
      { method: `POST`, path: `/stacks`, status: 201, body: stack(9, [1004, 1006, 1008, 1009]) },
    ])
    await ensureGithubStack({
      repo: `o/r`,
      token: `tok`,
      lowerPrNumber: 1008,
      newPrNumber: 1009,
      stopBranches: [`master`],
      fetchImpl: impl,
    })
    expect(calls.at(-1)!.body).toEqual({ pull_requests: [1004, 1006, 1008, 1009] })
  })

  it(`extends the stack the walk lands on`, async () => {
    const { impl, calls } = github([
      { path: `/stacks`, status: 200, body: [] }, // #1008
      { path: `/pulls/1008`, status: 200, body: pull(1008, `exp/1006`) },
      { path: `/pulls`, status: 200, body: [{ number: 1006, html_url: `u`, base: { ref: `exp/1004` } }] },
      { path: `/stacks`, status: 200, body: [stack(3, [1004, 1006])] }, // #1006 tops stack 3
      { method: `POST`, path: `/stacks/3/add`, status: 200, body: stack(3, [1004, 1006, 1008, 1009]) },
    ])
    await ensureGithubStack({ repo: `o/r`, token: `tok`, lowerPrNumber: 1008, newPrNumber: 1009, fetchImpl: impl })
    expect(calls.at(-1)!.body).toEqual({ pull_requests: [1008, 1009] })
  })

  it(`surfaces GitHub's refusal as an error, never silently`, async () => {
    const { impl } = github([
      { path: `/stacks`, status: 200, body: [] },
      { path: `/pulls/1011`, status: 200, body: pull(1011, `master`) },
      { method: `POST`, path: `/stacks`, status: 422, body: { message: `Base ref mismatch` } },
    ])
    const error = await ensureGithubStack({
      repo: `o/r`,
      token: `tok`,
      lowerPrNumber: 1011,
      newPrNumber: 1012,
      stopBranches: [`master`],
      fetchImpl: impl,
    }).catch((err: unknown) => err)
    expect(error).toBeInstanceOf(GitHubStackError)
    expect((error as GitHubStackError).status).toBe(422)
    expect((error as Error).message).toContain(`Base ref mismatch`)
  })
})

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

describe(`mergeThrough`, () => {
  it(`is ONE merge-async on the member, polled to its landing`, async () => {
    const { impl, calls } = github([
      { method: `PUT`, path: `/pulls/1008/merge-async`, status: 202, body: { status: `pending`, details: { uuid: `u-1` } } },
      { path: `/pulls/1008/merge-async/u-1`, status: 200, body: { status: `merged`, sha: `abc` } },
    ])
    const result = await mergeThrough({
      repo: `o/r`,
      token: `tok`,
      prNumber: 1008,
      fetchImpl: impl,
      sleepImpl: async () => {},
    })
    expect(result).toMatchObject({ merged: true, queued: false, sha: `abc` })
    expect(calls.map((call) => `${call.method} ${call.path}`)).toEqual([
      `PUT /pulls/1008/merge-async`,
      `GET /pulls/1008/merge-async/u-1`,
    ])
  })
})
