import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  GitHubAsyncMergePending,
  GitHubMergeError,
  mergedByPerson,
  mergePullRequestAsync,
  isFetchFailure,
  mergePullRequestSmart,
  pollAsyncMerge,
} from "@/lib/integrations/github-pr"

// FEED-43: the async merge is the ONLY way to land a PR GitHub holds in a
// stack: the legacy `PUT …/merge` answers 405 "Merging stacked PRs via this
// endpoint is not supported. Use the asynchronous merge endpoint instead."
// and every Exponential merge path died on it.

const STACKED_REFUSAL = `Merging stacked PRs via this endpoint is not supported. Use the asynchronous merge endpoint instead.`

interface Route {
  match: string
  method?: string
  status: number
  body?: unknown
}

function response(status: number, body: unknown) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(body ?? {}),
    json: async () => body ?? {},
  }
}

// Routes are consumed in order for the same (method, match) pair, so a retry
// can be given a different answer than the first attempt.
function routedFetch(routes: Route[]) {
  const queue = [...routes]
  const calls: Array<{ url: string; method: string; body?: string }> = []
  const impl = vi.fn(
    async (url: string, init?: { method?: string; body?: string }) => {
      const method = init?.method ?? `GET`
      calls.push({ url, method, body: init?.body })
      const index = queue.findIndex(
        (route) =>
          url.includes(route.match) && (route.method ?? `GET`) === method
      )
      if (index < 0) {
        throw new Error(`unrouted ${method} ${url}`)
      }
      const [route] = queue.splice(index, 1)
      return response(route!.status, route!.body)
    }
  )
  return { impl, calls }
}

const realFetch = globalThis.fetch

afterEach(() => {
  globalThis.fetch = realFetch
  vi.restoreAllMocks()
})

describe(`mergePullRequestAsync / pollAsyncMerge`, () => {
  it(`starts a job and reports its uuid`, async () => {
    const { impl, calls } = routedFetch([
      {
        match: `/pulls/242/merge-async`,
        method: `PUT`,
        status: 202,
        body: { status: `pending`, details: { uuid: `u-1`, message: `queued` } },
      },
    ])
    const started = await mergePullRequestAsync({
      repo: `o/r`,
      prNumber: 242,
      token: `tok`,
      commitTitle: `EXP-12: t (#242)`,
      fetchImpl: impl as never,
    })
    expect(started).toMatchObject({ status: `pending`, uuid: `u-1` })
    expect(JSON.parse(calls[0]!.body!)).toEqual({
      merge_method: `squash`,
      commit_title: `EXP-12: t (#242)`,
    })
  })

  it(`treats GitHub's 409 "already enqueued" as enqueued, not a failure`, async () => {
    const { impl } = routedFetch([
      {
        match: `/merge-async`,
        method: `PUT`,
        status: 409,
        body: { message: `A merge is already enqueued` },
      },
    ])
    await expect(
      mergePullRequestAsync({
        repo: `o/r`,
        prNumber: 242,
        token: `tok`,
        fetchImpl: impl as never,
      })
    ).resolves.toMatchObject({ status: `enqueued` })
  })

  it(`polls until the job leaves pending`, async () => {
    const { impl } = routedFetch([
      {
        match: `/merge-async/u-1`,
        status: 200,
        body: { status: `pending`, details: { uuid: `u-1` } },
      },
      {
        match: `/merge-async/u-1`,
        status: 200,
        body: { status: `merged`, sha: `deadbeef`, details: { uuid: `u-1` } },
      },
    ])
    const sleepImpl = vi.fn(async () => {})
    const state = await pollAsyncMerge({
      repo: `o/r`,
      prNumber: 242,
      uuid: `u-1`,
      token: `tok`,
      fetchImpl: impl as never,
      sleepImpl,
    })
    expect(state).toMatchObject({ status: `merged`, sha: `deadbeef` })
    expect(sleepImpl).toHaveBeenCalledTimes(1)
  })

  // FEED-64: only a dropped connection is "no answer yet". A bug (or a
  // caller's own throw) used to be swallowed into a silent 60 s of polling.
  it(`keeps polling across a dropped connection`, async () => {
    const { impl } = routedFetch([
      {
        match: `/merge-async/u-1`,
        status: 200,
        body: { status: `merged`, sha: `deadbeef`, details: { uuid: `u-1` } },
      },
    ])
    impl.mockImplementationOnce(async () => {
      throw new TypeError(`fetch failed`)
    })
    const sleepImpl = vi.fn(async () => {})
    const state = await pollAsyncMerge({
      repo: `o/r`,
      prNumber: 242,
      uuid: `u-1`,
      token: `tok`,
      fetchImpl: impl as never,
      sleepImpl,
    })
    expect(state).toMatchObject({ status: `merged`, sha: `deadbeef` })
    expect(sleepImpl).toHaveBeenCalledTimes(1)
  })

  it(`rethrows an error that is not a network failure instead of burning the deadline`, async () => {
    const impl = vi.fn(async () => {
      throw new RangeError(`programming error`)
    })
    const sleepImpl = vi.fn(async () => {})
    await expect(
      pollAsyncMerge({
        repo: `o/r`,
        prNumber: 242,
        uuid: `u-1`,
        token: `tok`,
        fetchImpl: impl as never,
        sleepImpl,
      })
    ).rejects.toThrow(`programming error`)
    expect(impl).toHaveBeenCalledTimes(1)
    expect(sleepImpl).not.toHaveBeenCalled()
  })

  it(`returns the last pending state at the injected deadline`, async () => {
    const { impl } = routedFetch([
      {
        match: `/merge-async/u-1`,
        status: 200,
        body: { status: `pending`, details: { uuid: `u-1` } },
      },
    ])
    const state = await pollAsyncMerge({
      repo: `o/r`,
      prNumber: 242,
      uuid: `u-1`,
      token: `tok`,
      fetchImpl: impl as never,
      sleepImpl: async () => {},
      timeoutMs: 1_000,
      stepMs: 2_000,
      nowImpl: () => 0,
    })
    expect(state.status).toBe(`pending`)
  })
})

describe(`isFetchFailure (FEED-64)`, () => {
  it(`classifies the shapes a dropped connection takes`, () => {
    expect(isFetchFailure(new TypeError(`fetch failed`))).toBe(true)
    expect(
      isFetchFailure(Object.assign(new Error(`socket hang up`), { code: `ECONNRESET` }))
    ).toBe(true)
    expect(
      isFetchFailure(Object.assign(new Error(`timeout`), { code: `UND_ERR_CONNECT_TIMEOUT` }))
    ).toBe(true)
    expect(
      isFetchFailure(new Error(`wrapped`, { cause: Object.assign(new Error(), { code: `ETIMEDOUT` }) }))
    ).toBe(true)
    expect(isFetchFailure(Object.assign(new Error(`aborted`), { name: `AbortError` }))).toBe(true)
  })

  it(`leaves programming errors and GitHub's own answers alone`, () => {
    expect(isFetchFailure(new RangeError(`bad index`))).toBe(false)
    expect(isFetchFailure(new Error(`unrouted GET`))).toBe(false)
    expect(isFetchFailure(new GitHubMergeError(500, `Server Error`))).toBe(false)
    expect(isFetchFailure(`string`)).toBe(false)
  })
})

describe(`mergePullRequestSmart (FEED-43)`, () => {
  function install(routes: Route[]) {
    const { impl, calls } = routedFetch(routes)
    globalThis.fetch = impl as never
    return { impl, calls }
  }

  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it(`merges through the legacy endpoint when the PR is not stacked`, async () => {
    const { calls } = install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 200,
        body: { merged: true, sha: `abc` },
      },
    ])
    const result = await mergePullRequestSmart({
      repo: `o/r`,
      prNumber: 241,
      token: `tok`,
    })
    expect(result).toMatchObject({
      merged: true,
      queued: false,
    })
    expect(calls).toHaveLength(1)
  })

  it(`falls back to merge-async on GitHub's stacked-PR refusal`, async () => {
    install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 405,
        body: { message: STACKED_REFUSAL },
      },
      {
        match: `/pulls/241/merge-async`,
        method: `PUT`,
        status: 202,
        body: { status: `merged`, sha: `abc`, details: { uuid: `u-1` } },
      },
    ])
    const result = await mergePullRequestSmart({
      repo: `o/r`,
      prNumber: 241,
      token: `tok`,
    })
    expect(result).toMatchObject({ merged: true, queued: false, sha: `abc` })
  })

  it(`reports an enqueued merge as success + queued`, async () => {
    install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 405,
        body: { message: STACKED_REFUSAL },
      },
      {
        match: `/pulls/241/merge-async`,
        method: `PUT`,
        status: 202,
        body: { status: `enqueued`, details: { uuid: `u-1` } },
      },
    ])
    await expect(
      mergePullRequestSmart({ repo: `o/r`, prNumber: 241, token: `tok` })
    ).resolves.toMatchObject({ merged: true, queued: true })
  })

  it(`turns a failed job into a GitHubMergeError carrying GitHub's message`, async () => {
    install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 405,
        body: { message: STACKED_REFUSAL },
      },
      {
        match: `/pulls/241/merge-async`,
        method: `PUT`,
        status: 202,
        body: {
          status: `failed`,
          details: { uuid: `u-1`, message: `Merge conflict` },
        },
      },
    ])
    await expect(
      mergePullRequestSmart({ repo: `o/r`, prNumber: 241, token: `tok` })
    ).rejects.toMatchObject({ status: 405, message: `Merge conflict` })
  })

  it(`raises GitHubAsyncMergePending when the job is still running at the deadline`, async () => {
    install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 405,
        body: { message: STACKED_REFUSAL },
      },
      {
        match: `/pulls/241/merge-async`,
        method: `PUT`,
        status: 202,
        body: { status: `pending`, details: { uuid: `u-1` } },
      },
      {
        match: `/merge-async/u-1`,
        status: 200,
        body: { status: `pending`, details: { uuid: `u-1` } },
      },
    ])
    const error = await mergePullRequestSmart({
      repo: `o/r`,
      prNumber: 241,
      token: `tok`,
      sleepImpl: async () => {},
      timeoutMs: 0,
      stepMs: 1,
    }).catch((err) => err)
    expect(error).toBeInstanceOf(GitHubAsyncMergePending)
    expect(error).toMatchObject({ prNumber: 241, uuid: `u-1` })
  })

  // FEED-64: GitHub's `PUT …/merge` answered 500 "Server Error" AFTER it had
  // written the squash commit; every merge path reported `merged: false`, the
  // issue state never moved, the claims were dropped and the own-PR spare
  // reverted. The PR's own state decides now.
  describe(`verifies the PR before calling a merge failed (FEED-64)`, () => {
    const noSleep = async () => {}
    const mergedPull = {
      state: `closed`,
      merged: true,
      merged_by: { login: `exponential[bot]`, type: `Bot` },
      merge_commit_sha: `d6ef0be6e5`,
    }
    const openPull = { state: `open`, merged: false, merged_by: null }

    it(`answers merged:true with the squash sha when a 500 hid a landed merge`, async () => {
      const { calls } = install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 500,
          body: { message: `Server Error` },
        },
        { match: `/pulls/241`, status: 200, body: mergedPull },
      ])
      const result = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
      })
      expect(result).toMatchObject({
        merged: true,
        queued: false,
        sha: `d6ef0be6e5`,
        // Our own App pressed the button: not a person's merge.
        mergedBy: { login: `exponential[bot]` },
      })
      expect(calls.map((call) => call.method)).toEqual([`PUT`, `GET`])
    })

    it(`rethrows the ORIGINAL 500 when two reads still show the PR open`, async () => {
      const { calls } = install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 500,
          body: { message: `Server Error` },
        },
        { match: `/pulls/241`, status: 200, body: openPull },
        { match: `/pulls/241`, status: 200, body: openPull },
      ])
      const error = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
      }).catch((e: unknown) => e)
      expect(error).toBeInstanceOf(GitHubMergeError)
      expect((error as GitHubMergeError).status).toBe(500)
      expect((error as GitHubMergeError).message).toBe(`Server Error`)
      expect(calls.map((call) => call.method)).toEqual([`PUT`, `GET`, `GET`])
    })

    it(`rethrows the original error when the verify read itself fails`, async () => {
      install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 502,
          body: { message: `Bad Gateway` },
        },
        // No GET route: the read throws "unrouted", which must never replace
        // GitHub's own answer.
      ])
      const error = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
      }).catch((e: unknown) => e)
      expect(error).toBeInstanceOf(GitHubMergeError)
      expect((error as GitHubMergeError).status).toBe(502)
    })

    it(`treats a dropped connection like a 5xx: the PR decides`, async () => {
      const { impl } = install([
        { match: `/pulls/241`, status: 200, body: mergedPull },
      ])
      impl.mockImplementationOnce(async () => {
        throw new TypeError(`fetch failed`)
      })
      const result = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
      })
      expect(result).toMatchObject({ merged: true, sha: `d6ef0be6e5` })
    })

    it(`answers merged:true to the 405 a retry gets on an already-merged PR (the idempotency promise)`, async () => {
      install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 405,
          body: { message: `Pull Request is not mergeable` },
        },
        { match: `/pulls/241`, status: 200, body: mergedPull },
      ])
      const result = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
      })
      expect(result).toMatchObject({ merged: true, sha: `d6ef0be6e5` })
    })

    it(`passes a real 405 conflict through untouched once the PR reads open`, async () => {
      install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 405,
          body: { message: `Pull Request has merge conflicts` },
        },
        { match: `/pulls/241`, status: 200, body: openPull },
        { match: `/pulls/241`, status: 200, body: openPull },
      ])
      const error = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
      }).catch((e: unknown) => e)
      expect(error).toBeInstanceOf(GitHubMergeError)
      expect((error as GitHubMergeError).status).toBe(405)
      expect((error as GitHubMergeError).message).toBe(
        `Pull Request has merge conflicts`
      )
    })

    // FEED-64 follow-up: the 405 is GitHub's answer ABOUT the PR, so an open
    // first read is final — a genuine conflict costs one read, not two and a
    // second of sleep.
    it(`settles a real 405 conflict on the FIRST open read: no second read, no sleep`, async () => {
      const sleepImpl = vi.fn(async () => {})
      const { calls } = install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 405,
          body: { message: `Pull Request has merge conflicts` },
        },
        { match: `/pulls/241`, status: 200, body: openPull },
      ])
      const error = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl,
      }).catch((e: unknown) => e)
      expect(error).toBeInstanceOf(GitHubMergeError)
      expect((error as GitHubMergeError).status).toBe(405)
      expect(calls.map((call) => call.method)).toEqual([`PUT`, `GET`])
      expect(sleepImpl).not.toHaveBeenCalled()
    })

    it(`still reads twice after a 5xx: GitHub may still be writing the merge`, async () => {
      const sleepImpl = vi.fn(async () => {})
      const { calls } = install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 500,
          body: { message: `Server Error` },
        },
        { match: `/pulls/241`, status: 200, body: openPull },
        { match: `/pulls/241`, status: 200, body: mergedPull },
      ])
      const result = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl,
      })
      expect(result).toMatchObject({ merged: true, sha: `d6ef0be6e5` })
      expect(calls.map((call) => call.method)).toEqual([`PUT`, `GET`, `GET`])
      expect(sleepImpl).toHaveBeenCalledTimes(1)
    })

    it(`never verifies a 4xx GitHub decided (409 head moved: one call, no read)`, async () => {
      const { calls } = install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 409,
          body: { message: `Head branch was modified.` },
        },
      ])
      await expect(
        mergePullRequestSmart({ repo: `o/r`, prNumber: 241, token: `tok` })
      ).rejects.toMatchObject({ status: 409 })
      expect(calls).toHaveLength(1)
    })

    it(`names the PERSON who merged it when GitHub says so, for the caller to attribute`, async () => {
      install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 500,
          body: { message: `Server Error` },
        },
        {
          match: `/pulls/241`,
          status: 200,
          body: {
            ...mergedPull,
            merged_by: { login: `danny`, id: 7, type: `User` },
          },
        },
      ])
      const result = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
      })
      expect(result.mergedBy).toEqual({ login: `danny`, id: 7, type: `User` })
      expect(mergedByPerson(result.mergedBy)).toBe(true)
      expect(mergedByPerson({ login: `exponential[bot]` })).toBe(false)
      expect(mergedByPerson({ login: `danny`, type: `Bot` })).toBe(false)
      expect(mergedByPerson(null)).toBe(false)
    })

    it(`verifies a 502 from merge-async too`, async () => {
      install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 405,
          body: { message: STACKED_REFUSAL },
        },
          {
          match: `/pulls/241/merge-async`,
          method: `PUT`,
          status: 502,
          body: { message: `Bad Gateway` },
        },
        { match: `/pulls/241`, status: 200, body: mergedPull },
      ])
      const result = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
      })
      expect(result).toMatchObject({
        merged: true,
        queued: false,
        sha: `d6ef0be6e5`,
      })
    })

    it(`keeps polling merge-async through a 5xx on one poll (the job is still running)`, async () => {
      install([
        {
          match: `/pulls/241/merge`,
          method: `PUT`,
          status: 405,
          body: { message: STACKED_REFUSAL },
        },
          {
          match: `/pulls/241/merge-async`,
          method: `PUT`,
          status: 202,
          body: { status: `pending`, details: { uuid: `u-1` } },
        },
        {
          match: `/merge-async/u-1`,
          status: 503,
          body: { message: `Service Unavailable` },
        },
        {
          match: `/merge-async/u-1`,
          status: 200,
          body: { status: `merged`, sha: `abc` },
        },
      ])
      const result = await mergePullRequestSmart({
        repo: `o/r`,
        prNumber: 241,
        token: `tok`,
        sleepImpl: noSleep,
        stepMs: 1,
        timeoutMs: 10_000,
      })
      expect(result).toMatchObject({ merged: true, sha: `abc` })
    })
  })

  it(`rethrows a NON-stacked refusal untouched (no merge-async retry)`, async () => {
    const { calls } = install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 405,
        body: { message: `Squash merges are not allowed on this repository` },
      },
    ])
    await expect(
      mergePullRequestSmart({ repo: `o/r`, prNumber: 241, token: `tok` })
    ).rejects.toMatchObject({
      status: 405,
      message: `Squash merges are not allowed on this repository`,
    })
    expect(calls).toHaveLength(1)
  })
})
