import { afterEach, describe, expect, it, vi } from "vitest"
import {
  branchExists,
  createPullRequest,
  deleteBranch,
  findOpenPullByHead,
  fetchPullState,
  getPullRequest,
  type GitHubFetch,
  listOpenPullsByBase,
  retargetPullRequest,
  updatePullRequest,
  GitHubMergeError,
  PullAlreadyExistsError,
} from "@/lib/integrations/github-pr"

function jsonResponse(status: number, body: unknown) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(body),
    json: async () => body,
  }
}

describe(`fetchPullState`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  // EXP-897: the poller folds the stack member's base into its ONE read.
  it(`maps state, merger and the live base ref`, async () => {
    vi.stubGlobal(
      `fetch`,
      vi.fn(async () =>
        jsonResponse(200, {
          state: `closed`,
          merged: true,
          merged_by: { login: `octocat`, id: 1 },
          merged_at: `2026-11-04T23:40:00Z`,
          base: { ref: `exp/EXP-314` },
          head: { sha: `abc123` },
        })
      )
    )
    await expect(fetchPullState(`o/r`, 241, `tok`)).resolves.toEqual({
      state: `closed`,
      merged: true,
      mergedBy: { login: `octocat`, id: 1 },
      baseRef: `exp/EXP-314`,
      mergeCommitSha: null,
      headSha: `abc123`,
      // EXP-1705: the real merge time, for the live-PR backfill.
      mergedAt: new Date(`2026-11-04T23:40:00Z`),
    })
  })

  it(`reports a null base when GitHub omits it`, async () => {
    vi.stubGlobal(
      `fetch`,
      vi.fn(async () => jsonResponse(200, { state: `open`, merged: false }))
    )
    await expect(fetchPullState(`o/r`, 241, `tok`)).resolves.toEqual({
      state: `open`,
      merged: false,
      mergedBy: null,
      baseRef: null,
      mergeCommitSha: null,
      headSha: null,
      mergedAt: null,
    })
  })
})

describe(`getPullRequest`, () => {
  it(`maps head/base refs and mergeable state`, async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse(200, {
        state: `open`,
        merged: false,
        head: { ref: `exp/EXP-320`, sha: `abc123` },
        base: { ref: `exp/EXP-314` },
        mergeable: false,
        mergeable_state: `dirty`,
      })
    ) as unknown as GitHubFetch
    const pull = await getPullRequest(`o/r`, 241, `tok`, fetchImpl)
    expect(pull).toEqual({
      state: `open`,
      merged: false,
      // EXP-897: the stack pre-flight refuses a draft member, so the read
      // carries it.
      draft: false,
      headRef: `exp/EXP-320`,
      headSha: `abc123`,
      baseRef: `exp/EXP-314`,
      mergeable: false,
      mergeableState: `dirty`,
      // EXP-1139: the description fields, normalised when GitHub omits them.
      title: ``,
      body: ``,
      url: `https://github.com/o/r/pull/241`,
    })
  })

  it(`carries the title, body and html url (EXP-1139)`, async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse(200, {
        state: `open`,
        merged: false,
        title: `EXP-320: Stacked child`,
        body: null,
        html_url: `https://github.com/o/r/pull/241`,
        head: { ref: `exp/EXP-320` },
        base: { ref: `master` },
      })
    ) as unknown as GitHubFetch
    const pull = await getPullRequest(`o/r`, 241, `tok`, fetchImpl)
    expect(pull.title).toBe(`EXP-320: Stacked child`)
    // GitHub sends `body: null` for an empty description.
    expect(pull.body).toBe(``)
    expect(pull.url).toBe(`https://github.com/o/r/pull/241`)
  })

  it(`throws on a non-2xx response`, async () => {
    const fetchImpl = (async () =>
      jsonResponse(404, { message: `Not Found` })) as unknown as GitHubFetch
    await expect(getPullRequest(`o/r`, 241, `tok`, fetchImpl)).rejects.toThrow(
      `GitHub returned 404`
    )
  })
})

// FEED-59: pr_open links to the PR already open on its head.
describe(`findOpenPullByHead`, () => {
  it(`returns the open PR on the head with its base`, async () => {
    const fetchImpl = vi.fn(async (_url: string) =>
      jsonResponse(200, [
        {
          number: 873,
          html_url: `https://github.com/owner/repo/pull/873`,
          base: { ref: `master` },
        },
      ])
    )
    const pull = await findOpenPullByHead(
      `owner/repo`,
      `exp/batch-a14a29c3`,
      `tok`,
      fetchImpl as unknown as GitHubFetch
    )
    expect(fetchImpl.mock.calls[0][0]).toBe(
      `https://api.github.com/repos/owner/repo/pulls?state=open&head=owner:exp%2Fbatch-a14a29c3&per_page=1`
    )
    expect(pull).toEqual({
      url: `https://github.com/owner/repo/pull/873`,
      number: 873,
      baseRef: `master`,
    })
  })

  it(`narrows to one base when asked (the PR behind a 422)`, async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse(200, []))
    await findOpenPullByHead(
      `owner/repo`,
      `exp/batch-a14a29c3`,
      `tok`,
      fetchImpl as unknown as GitHubFetch,
      `release/1.2`
    )
    expect(fetchImpl.mock.calls[0][0]).toBe(
      `https://api.github.com/repos/owner/repo/pulls?state=open&head=owner:exp%2Fbatch-a14a29c3&base=release%2F1.2&per_page=1`
    )
  })

  it(`returns null when the head has no open PR`, async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(200, []))
    expect(
      await findOpenPullByHead(
        `owner/repo`,
        `exp/x`,
        `tok`,
        fetchImpl as unknown as GitHubFetch
      )
    ).toBeNull()
  })
})

describe(`createPullRequest`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it(`throws PullAlreadyExistsError only for GitHub's "already exists" 422`, async () => {
    const exists = {
      message: `Validation Failed`,
      errors: [
        {
          message: `A pull request already exists for owner:exp/batch-a14a29c3.`,
        },
      ],
    }
    const opts = {
      repo: `owner/repo`,
      head: `exp/batch-a14a29c3`,
      base: `master`,
      title: `t`,
      body: ``,
      token: `tok`,
    }
    vi.stubGlobal(`fetch`, vi.fn(async () => jsonResponse(422, exists)))
    await expect(createPullRequest(opts)).rejects.toBeInstanceOf(
      PullAlreadyExistsError
    )

    vi.stubGlobal(
      `fetch`,
      vi.fn(async () =>
        jsonResponse(422, { errors: [{ message: `No commits between` }] })
      )
    )
    const other = await createPullRequest(opts).catch((e: unknown) => e)
    expect(other).toBeInstanceOf(Error)
    expect(other).not.toBeInstanceOf(PullAlreadyExistsError)
    expect((other as Error).message).toBe(
      `GitHub PR create failed (422): No commits between`
    )
  })

  // FEED-74: a 500 with an EMPTY body used to read `GitHub PR create failed
  // (500): ` — the message names the empty body and GitHub's request id.
  it(`names an empty body and the request id on a 5xx`, async () => {
    vi.stubGlobal(
      `fetch`,
      vi.fn(async () => ({
        ok: false,
        status: 500,
        headers: new Headers({ "x-github-request-id": `1234:ABCD` }),
        text: async () => ``,
        json: async () => null,
      }))
    )
    await expect(
      createPullRequest({
        repo: `owner/repo`,
        head: `exp/native-ci`,
        base: `master`,
        title: `t`,
        body: ``,
        token: `tok`,
      })
    ).rejects.toThrow(
      `GitHub PR create failed (500): empty response body (request 1234:ABCD)`
    )
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

describe(`updatePullRequest (EXP-1139)`, () => {
  it(`PATCHes only the fields given`, async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(200, {}))
    await updatePullRequest({
      repo: `owner/repo`,
      prNumber: 241,
      body: `Closes #EXP-1\n\nNow with tests.`,
      token: `tok`,
      fetchImpl: fetchImpl as unknown as GitHubFetch,
    })
    const [url, init] = fetchImpl.mock.calls[0] as unknown as [
      string,
      { method?: string; body?: string },
    ]
    expect(url).toBe(`https://api.github.com/repos/owner/repo/pulls/241`)
    expect(init.method).toBe(`PATCH`)
    // An omitted title is ABSENT from the PATCH — GitHub keeps it.
    expect(JSON.parse(init.body as string)).toEqual({
      body: `Closes #EXP-1\n\nNow with tests.`,
    })
  })

  it(`sends both when both are given`, async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(200, {}))
    await updatePullRequest({
      repo: `owner/repo`,
      prNumber: 241,
      title: `New title`,
      body: ``,
      token: `tok`,
      fetchImpl: fetchImpl as unknown as GitHubFetch,
    })
    const [, init] = fetchImpl.mock.calls[0] as unknown as [
      string,
      { body?: string },
    ]
    // An EMPTY body is a real value (clears the description), never dropped.
    expect(JSON.parse(init.body as string)).toEqual({
      title: `New title`,
      body: ``,
    })
  })

  it(`throws GitHubMergeError with GitHub's message and status on failure`, async () => {
    const fetchImpl = (async () =>
      jsonResponse(404, { message: `Not Found` })) as unknown as GitHubFetch
    await expect(
      updatePullRequest({
        repo: `owner/repo`,
        prNumber: 241,
        title: `x`,
        token: `tok`,
        fetchImpl,
      })
    ).rejects.toMatchObject({ status: 404, message: `Not Found` })
  })

  // FEED-74: the empty 500 behind "GitHub update failed:" — never an empty
  // message again, and the request id rides along.
  it(`names an empty body and the request id on a 5xx`, async () => {
    const fetchImpl = (async () => ({
      ok: false,
      status: 500,
      headers: new Headers({ "x-github-request-id": `1234:ABCD` }),
      text: async () => ``,
      json: async () => null,
    })) as unknown as GitHubFetch
    await expect(
      updatePullRequest({
        repo: `owner/repo`,
        prNumber: 977,
        body: `report`,
        token: `tok`,
        fetchImpl,
      })
    ).rejects.toMatchObject({
      status: 500,
      message: `empty response body (request 1234:ABCD)`,
    })
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

describe(`deleteBranch (NEW-5)`, () => {
  it(`DELETEs the head ref, keeping the branch's slashes as path segments`, async () => {
    const fetchImpl = vi.fn(async (_url: string, _init: unknown) => jsonResponse(204, null))
    await expect(
      deleteBranch({ repo: `o/r`, branch: `exp/EXP-42`, token: `tok`, fetchImpl: fetchImpl as unknown as GitHubFetch })
    ).resolves.toBe(`deleted`)
    expect(fetchImpl.mock.calls[0][0]).toBe(`https://api.github.com/repos/o/r/git/refs/heads/exp/EXP-42`)
    expect(fetchImpl.mock.calls[0][1]).toMatchObject({
      method: `DELETE`,
      headers: expect.objectContaining({ authorization: `Bearer tok` }),
    })
  })

  it(`an already deleted branch is gone, not an error`, async () => {
    for (const status of [404, 422]) {
      const fetchImpl = (async () => jsonResponse(status, { message: `Reference does not exist` })) as unknown as GitHubFetch
      await expect(deleteBranch({ repo: `o/r`, branch: `exp/EXP-42`, token: `tok`, fetchImpl })).resolves.toBe(`gone`)
    }
  })

  it(`throws on anything else`, async () => {
    const fetchImpl = (async () => jsonResponse(403, { message: `Resource not accessible by integration` })) as unknown as GitHubFetch
    await expect(deleteBranch({ repo: `o/r`, branch: `exp/EXP-42`, token: `tok`, fetchImpl })).rejects.toThrow(/403/)
  })
})
