import { describe, expect, it, vi } from "vitest"
import {
  branchExists,
  classifyPrBase,
  diagnoseUnmergeablePr,
  getPullRequest,
  type GitHubFetch,
  listOpenPullsByBase,
  listPullsByHead,
  resolvePrBaseState,
  retargetPullRequest,
  GitHubMergeError,
} from "@/lib/integrations/github-pr"

function jsonResponse(status: number, body: unknown) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(body),
    json: async () => body,
  }
}

describe(`classifyPrBase`, () => {
  it(`classifies a default-branch base as default`, () => {
    expect(
      classifyPrBase({
        baseRef: `master`,
        defaultBranch: `master`,
        parentPulls: [],
        baseBranchExists: true,
      })
    ).toEqual({
      kind: `default`,
      rebaseOnto: `master`,
      retargetTo: null,
      parentPrNumber: null,
    })
  })

  it(`classifies an open parent PR as a live stack — rebase onto the base, no retarget`, () => {
    expect(
      classifyPrBase({
        baseRef: `exp/EXP-314`,
        defaultBranch: `master`,
        parentPulls: [{ number: 240, state: `open`, merged: false }],
        baseBranchExists: true,
      })
    ).toEqual({
      kind: `open-parent`,
      rebaseOnto: `exp/EXP-314`,
      retargetTo: null,
      parentPrNumber: 240,
    })
  })

  // The EXP-320 shape: the parent PR was squash-merged but its branch left in
  // place. Content-containment checks can never detect this (squash rewrites
  // the commits), so the classification must come from the parent PR's state.
  it(`classifies a squash-merged parent with an undeleted branch as merged-parent (EXP-320 regression)`, () => {
    expect(
      classifyPrBase({
        baseRef: `exp/EXP-314`,
        defaultBranch: `master`,
        parentPulls: [{ number: 240, state: `closed`, merged: true }],
        baseBranchExists: true,
      })
    ).toEqual({
      kind: `merged-parent`,
      rebaseOnto: `master`,
      retargetTo: `master`,
      parentPrNumber: 240,
    })
  })

  it(`classifies a closed-unmerged parent as closed-parent — retarget to default`, () => {
    expect(
      classifyPrBase({
        baseRef: `exp/EXP-314`,
        defaultBranch: `master`,
        parentPulls: [{ number: 240, state: `closed`, merged: false }],
        baseBranchExists: true,
      })
    ).toEqual({
      kind: `closed-parent`,
      rebaseOnto: `master`,
      retargetTo: `master`,
      parentPrNumber: 240,
    })
  })

  it(`prefers an open parent over merged and closed ones (recycled branch names)`, () => {
    const result = classifyPrBase({
      baseRef: `exp/EXP-314`,
      defaultBranch: `master`,
      parentPulls: [
        { number: 200, state: `closed`, merged: true },
        { number: 240, state: `open`, merged: false },
        { number: 180, state: `closed`, merged: false },
      ],
      baseBranchExists: true,
    })
    expect(result.kind).toBe(`open-parent`)
    expect(result.parentPrNumber).toBe(240)
  })

  it(`prefers a merged parent over a closed-unmerged one`, () => {
    const result = classifyPrBase({
      baseRef: `exp/EXP-314`,
      defaultBranch: `master`,
      parentPulls: [
        { number: 180, state: `closed`, merged: false },
        { number: 200, state: `closed`, merged: true },
      ],
      baseBranchExists: true,
    })
    expect(result.kind).toBe(`merged-parent`)
    expect(result.parentPrNumber).toBe(200)
  })

  it(`classifies a PR-less existing base branch as a deliberate custom base`, () => {
    expect(
      classifyPrBase({
        baseRef: `release/1.x`,
        defaultBranch: `master`,
        parentPulls: [],
        baseBranchExists: true,
      })
    ).toEqual({
      kind: `custom-branch`,
      rebaseOnto: `release/1.x`,
      retargetTo: null,
      parentPrNumber: null,
    })
  })

  it(`classifies a vanished base branch as missing — retarget to default`, () => {
    expect(
      classifyPrBase({
        baseRef: `exp/EXP-314`,
        defaultBranch: `master`,
        parentPulls: [],
        baseBranchExists: false,
      })
    ).toEqual({
      kind: `missing-branch`,
      rebaseOnto: `master`,
      retargetTo: `master`,
      parentPrNumber: null,
    })
  })
})

describe(`getPullRequest`, () => {
  it(`maps head/base refs and mergeable state`, async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse(200, {
        state: `open`,
        merged: false,
        head: { ref: `exp/EXP-320` },
        base: { ref: `exp/EXP-314` },
        mergeable: false,
        mergeable_state: `dirty`,
      })
    ) as unknown as GitHubFetch
    const pull = await getPullRequest(`o/r`, 241, `tok`, fetchImpl)
    expect(pull).toEqual({
      state: `open`,
      merged: false,
      headRef: `exp/EXP-320`,
      baseRef: `exp/EXP-314`,
      mergeable: false,
      mergeableState: `dirty`,
    })
  })

  it(`throws on a non-2xx response`, async () => {
    const fetchImpl = (async () =>
      jsonResponse(404, { message: `Not Found` })) as unknown as GitHubFetch
    await expect(getPullRequest(`o/r`, 241, `tok`, fetchImpl)).rejects.toThrow(
      `GitHub returned 404`
    )
  })
})

describe(`listPullsByHead`, () => {
  it(`scopes the head filter to the repo owner and URL-encodes the ref`, async () => {
    const fetchImpl = vi.fn(async (_url: string) =>
      jsonResponse(200, [
        { number: 240, state: `closed`, merged_at: `2026-07-20T00:00:00Z` },
        { number: 199, state: `closed`, merged_at: null },
      ])
    )
    const pulls = await listPullsByHead(
      `owner/repo`,
      `exp/EXP-314`,
      `tok`,
      fetchImpl as unknown as GitHubFetch
    )
    expect(fetchImpl.mock.calls[0][0]).toBe(
      `https://api.github.com/repos/owner/repo/pulls?state=all&head=owner:exp%2FEXP-314&per_page=30`
    )
    expect(pulls).toEqual([
      { number: 240, state: `closed`, merged: true },
      { number: 199, state: `closed`, merged: false },
    ])
  })
})

describe(`listOpenPullsByBase`, () => {
  it(`lists open PRs based on the ref`, async () => {
    const fetchImpl = vi.fn(async (_url: string) =>
      jsonResponse(200, [
        {
          number: 241,
          html_url: `https://github.com/owner/repo/pull/241`,
          head: { ref: `exp/EXP-320` },
        },
      ])
    )
    const pulls = await listOpenPullsByBase(
      `owner/repo`,
      `exp/EXP-314`,
      `tok`,
      fetchImpl as unknown as GitHubFetch
    )
    expect(fetchImpl.mock.calls[0][0]).toBe(
      `https://api.github.com/repos/owner/repo/pulls?state=open&base=exp%2FEXP-314&per_page=100`
    )
    expect(pulls).toEqual([
      {
        number: 241,
        url: `https://github.com/owner/repo/pull/241`,
        headRef: `exp/EXP-320`,
      },
    ])
  })
})

describe(`branchExists`, () => {
  it(`maps 200 to true and 404 to false, throws otherwise`, async () => {
    const ok = (async () => jsonResponse(200, {})) as unknown as GitHubFetch
    const gone = (async () =>
      jsonResponse(404, { message: `Branch not found` })) as unknown as GitHubFetch
    const broken = (async () =>
      jsonResponse(500, {})) as unknown as GitHubFetch
    await expect(branchExists(`o/r`, `exp/EXP-314`, `tok`, ok)).resolves.toBe(
      true
    )
    await expect(branchExists(`o/r`, `exp/EXP-314`, `tok`, gone)).resolves.toBe(
      false
    )
    await expect(
      branchExists(`o/r`, `exp/EXP-314`, `tok`, broken)
    ).rejects.toThrow(`GitHub returned 500`)
  })
})

describe(`retargetPullRequest`, () => {
  it(`PATCHes the new base and resolves on success`, async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(200, {}))
    await retargetPullRequest({
      repo: `owner/repo`,
      prNumber: 241,
      base: `master`,
      token: `tok`,
      fetchImpl: fetchImpl as unknown as GitHubFetch,
    })
    const [url, init] = fetchImpl.mock.calls[0] as unknown as [
      string,
      { method?: string; body?: string },
    ]
    expect(url).toBe(`https://api.github.com/repos/owner/repo/pulls/241`)
    expect(init.method).toBe(`PATCH`)
    expect(JSON.parse(init.body as string)).toEqual({ base: `master` })
  })

  it(`throws GitHubMergeError with GitHub's message and status on failure`, async () => {
    const fetchImpl = (async () =>
      jsonResponse(422, {
        message: `Proposed base branch 'nope' was not found`,
      })) as unknown as GitHubFetch
    await expect(
      retargetPullRequest({
        repo: `owner/repo`,
        prNumber: 241,
        base: `nope`,
        token: `tok`,
        fetchImpl,
      })
    ).rejects.toMatchObject({
      status: 422,
      message: `Proposed base branch 'nope' was not found`,
    })
    await expect(
      retargetPullRequest({
        repo: `owner/repo`,
        prNumber: 241,
        base: `nope`,
        token: `tok`,
        fetchImpl,
      })
    ).rejects.toBeInstanceOf(GitHubMergeError)
  })
})

// Route the orchestrator's sub-fetches by URL — one injected fetch serves the
// PR read, the by-head listing, and the branch probe.
function routedFetch(
  routes: Array<{ match: string; status: number; body: unknown }>
): GitHubFetch {
  return (async (url: string) => {
    const route = routes.find((r) => url.includes(r.match))
    if (!route) throw new Error(`unexpected fetch: ${url}`)
    return jsonResponse(route.status, route.body)
  }) as unknown as GitHubFetch
}

describe(`resolvePrBaseState`, () => {
  it(`short-circuits a default-based PR without extra GitHub calls`, async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse(200, {
        state: `open`,
        merged: false,
        head: { ref: `exp/EXP-42` },
        base: { ref: `master` },
      })
    )
    const state = await resolvePrBaseState({
      repo: `o/r`,
      prNumber: 7,
      token: `tok`,
      defaultBranch: `master`,
      fetchImpl: fetchImpl as unknown as GitHubFetch,
    })
    expect(state.kind).toBe(`default`)
    expect(state.rebaseOnto).toBe(`master`)
    expect(fetchImpl).toHaveBeenCalledTimes(1)
  })

  it(`resolves the EXP-320 shape end-to-end: merged parent → retarget to default`, async () => {
    const fetchImpl = routedFetch([
      {
        match: `/pulls/241`,
        status: 200,
        body: {
          state: `open`,
          merged: false,
          head: { ref: `exp/EXP-320` },
          base: { ref: `exp/EXP-314` },
        },
      },
      {
        match: `state=all&head=`,
        status: 200,
        body: [{ number: 240, state: `closed`, merged_at: `2026-07-20T00:00:00Z` }],
      },
    ])
    const state = await resolvePrBaseState({
      repo: `owner/repo`,
      prNumber: 241,
      token: `tok`,
      defaultBranch: `master`,
      fetchImpl,
    })
    expect(state).toMatchObject({
      kind: `merged-parent`,
      rebaseOnto: `master`,
      retargetTo: `master`,
      parentPrNumber: 240,
      baseRef: `exp/EXP-314`,
      headRef: `exp/EXP-320`,
    })
  })

  it(`only probes branch existence when no PR ever had the base as head`, async () => {
    const fetchImpl = routedFetch([
      {
        match: `/pulls/9`,
        status: 200,
        body: {
          state: `open`,
          merged: false,
          head: { ref: `feat/x` },
          base: { ref: `release/1.x` },
        },
      },
      { match: `state=all&head=`, status: 200, body: [] },
      { match: `/branches/`, status: 200, body: {} },
    ])
    const state = await resolvePrBaseState({
      repo: `owner/repo`,
      prNumber: 9,
      token: `tok`,
      defaultBranch: `master`,
      fetchImpl,
    })
    expect(state.kind).toBe(`custom-branch`)
    expect(state.rebaseOnto).toBe(`release/1.x`)
  })
})

describe(`diagnoseUnmergeablePr`, () => {
  it(`names the merged parent and the retarget tool for the EXP-320 shape`, async () => {
    const fetchImpl = routedFetch([
      {
        match: `/pulls/241`,
        status: 200,
        body: {
          state: `open`,
          merged: false,
          head: { ref: `exp/EXP-320` },
          base: { ref: `exp/EXP-314` },
        },
      },
      {
        match: `state=all&head=`,
        status: 200,
        body: [{ number: 240, state: `closed`, merged_at: `2026-07-20T00:00:00Z` }],
      },
    ])
    const message = await diagnoseUnmergeablePr({
      repo: `owner/repo`,
      prNumber: 241,
      token: `tok`,
      defaultBranch: `master`,
      fetchImpl,
    })
    expect(message).toContain(`'exp/EXP-314'`)
    expect(message).toContain(`#240`)
    expect(message).toContain(`already-merged`)
    expect(message).toContain(`exponential_pr_retarget`)
    expect(message).toContain(`'master'`)
  })

  it(`reports real content conflicts for a default-based PR`, async () => {
    const fetchImpl = routedFetch([
      {
        match: `/pulls/7`,
        status: 200,
        body: {
          state: `open`,
          merged: false,
          head: { ref: `exp/EXP-42` },
          base: { ref: `master` },
        },
      },
    ])
    const message = await diagnoseUnmergeablePr({
      repo: `owner/repo`,
      prNumber: 7,
      token: `tok`,
      defaultBranch: `master`,
      fetchImpl,
    })
    expect(message).toContain(`merge conflicts with 'master'`)
    expect(message).toContain(`--force-with-lease`)
  })

  it(`returns null when the diagnosis itself fails (caller keeps GitHub's message)`, async () => {
    const fetchImpl = (async () =>
      jsonResponse(500, {})) as unknown as GitHubFetch
    await expect(
      diagnoseUnmergeablePr({
        repo: `owner/repo`,
        prNumber: 7,
        token: `tok`,
        defaultBranch: `master`,
        fetchImpl,
      })
    ).resolves.toBeNull()
  })
})
