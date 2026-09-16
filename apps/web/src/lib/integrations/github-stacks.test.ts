import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  addToStack,
  createStack,
  findStackForPull,
  GitHubAsyncMergePending,
  GitHubMergeError,
  membersAtOrBelowNumber,
  mergePullRequestAsync,
  mergePullRequestSmart,
  pollAsyncMerge,
  unstack,
} from "@/lib/integrations/github-pr"

// EXP-897 / FEED-43: GitHub's stacked-PR API and the async merge that is the
// ONLY way to land a stack member. The named regression is FEED-43: the legacy
// `PUT …/merge` answers 405 "Merging stacked PRs via this endpoint is not
// supported. Use the asynchronous merge endpoint instead." and every
// Exponential merge path died on it.

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

const STACK_BODY = {
  id: `S_1`,
  number: 7,
  base: { ref: `master` },
  open: true,
  pull_requests: [
    {
      number: 240,
      state: `open`,
      draft: false,
      merged_at: null,
      head: { ref: `exp/EXP-10`, sha: `aaa` },
    },
    {
      number: 241,
      state: `open`,
      draft: false,
      merged_at: null,
      head: { ref: `exp/EXP-11`, sha: `bbb` },
    },
    {
      number: 242,
      state: `open`,
      draft: true,
      merged_at: null,
      head: { ref: `exp/EXP-12`, sha: `ccc` },
    },
  ],
}

const realFetch = globalThis.fetch

afterEach(() => {
  globalThis.fetch = realFetch
  vi.restoreAllMocks()
})

describe(`findStackForPull`, () => {
  it(`parses GitHub's stack, bottom first`, async () => {
    const { impl, calls } = routedFetch([
      { match: `/stacks?pull_request=241`, status: 200, body: [STACK_BODY] },
    ])
    const stack = await findStackForPull(
      `o/r`,
      241,
      `tok`,
      impl as never
    )
    expect(stack).toMatchObject({ number: 7, baseRef: `master`, open: true })
    expect(stack!.members.map((m) => m.number)).toEqual([240, 241, 242])
    expect(stack!.members[2]).toMatchObject({ draft: true, headRef: `exp/EXP-12` })
    // The preview version pins ONLY these calls.
    expect(calls[0]!.url).toContain(`/stacks?pull_request=241`)
  })

  it(`degrades to null when the stack preview is unavailable (404)`, async () => {
    const { impl } = routedFetch([
      { match: `/stacks`, status: 404, body: { message: `Not Found` } },
    ])
    await expect(
      findStackForPull(`o/r`, 241, `tok`, impl as never)
    ).resolves.toBeNull()
  })

  it(`throws on any other failure`, async () => {
    const { impl } = routedFetch([
      { match: `/stacks`, status: 500, body: { message: `boom` } },
    ])
    await expect(
      findStackForPull(`o/r`, 241, `tok`, impl as never)
    ).rejects.toBeInstanceOf(GitHubMergeError)
  })
})

describe(`createStack / addToStack / unstack`, () => {
  it(`creates a stack from an ordered pair`, async () => {
    const { impl, calls } = routedFetch([
      { match: `/stacks`, method: `POST`, status: 201, body: STACK_BODY },
    ])
    const stack = await createStack(`o/r`, [240, 241], `tok`, impl as never)
    expect(stack?.number).toBe(7)
    expect(JSON.parse(calls[0]!.body!)).toEqual({ pull_requests: [240, 241] })
  })

  it(`returns null when GitHub rejects the chain (422) or has no preview (404)`, async () => {
    const rejected = routedFetch([
      { match: `/stacks`, method: `POST`, status: 422, body: { message: `no` } },
    ])
    await expect(
      createStack(`o/r`, [240, 241], `tok`, rejected.impl as never)
    ).resolves.toBeNull()
    const missing = routedFetch([
      { match: `/stacks`, method: `POST`, status: 404, body: {} },
    ])
    await expect(
      createStack(`o/r`, [240, 241], `tok`, missing.impl as never)
    ).resolves.toBeNull()
  })

  it(`retries an add ONCE on a 409 race`, async () => {
    const { impl, calls } = routedFetch([
      { match: `/stacks/7/add`, method: `POST`, status: 409, body: {} },
      { match: `/stacks/7/add`, method: `POST`, status: 200, body: STACK_BODY },
    ])
    const stack = await addToStack(`o/r`, 7, [242], `tok`, impl as never)
    expect(stack?.number).toBe(7)
    expect(calls).toHaveLength(2)
  })

  it(`gives up after a second 409`, async () => {
    const { impl, calls } = routedFetch([
      { match: `/stacks/7/add`, method: `POST`, status: 409, body: {} },
      { match: `/stacks/7/add`, method: `POST`, status: 409, body: {} },
    ])
    await expect(
      addToStack(`o/r`, 7, [242], `tok`, impl as never)
    ).resolves.toBeNull()
    expect(calls).toHaveLength(2)
  })

  it(`unstacks on 200 and 204, and reports a refusal as false`, async () => {
    const ok200 = routedFetch([
      { match: `/stacks/7/unstack`, method: `POST`, status: 200, body: {} },
    ])
    await expect(
      unstack(`o/r`, 7, `tok`, ok200.impl as never)
    ).resolves.toBe(true)
    const ok204 = routedFetch([
      { match: `/stacks/7/unstack`, method: `POST`, status: 204, body: {} },
    ])
    await expect(
      unstack(`o/r`, 7, `tok`, ok204.impl as never)
    ).resolves.toBe(true)
    const refused = routedFetch([
      { match: `/stacks/7/unstack`, method: `POST`, status: 422, body: {} },
    ])
    await expect(
      unstack(`o/r`, 7, `tok`, refused.impl as never)
    ).resolves.toBe(false)
  })
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

describe(`membersAtOrBelowNumber`, () => {
  it(`returns the merged member and everything below it`, () => {
    const stack = {
      id: null,
      number: 7,
      baseRef: `master`,
      open: true,
      members: [240, 241, 242].map((number) => ({
        number,
        state: `open` as const,
        draft: false,
        merged: false,
        headRef: `exp/${number}`,
        headSha: null,
      })),
    }
    expect(membersAtOrBelowNumber(stack, 241)).toEqual([240, 241])
    expect(membersAtOrBelowNumber(stack, 999)).toEqual([999])
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
      viaStack: false,
      stackMemberNumbers: [241],
    })
    expect(calls).toHaveLength(1)
  })

  it(`falls back to merge-async on GitHub's stacked-PR refusal and lands everything below`, async () => {
    install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 405,
        body: { message: STACKED_REFUSAL },
      },
      { match: `/stacks?pull_request=241`, status: 200, body: [STACK_BODY] },
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
    expect(result).toMatchObject({
      merged: true,
      queued: false,
      viaStack: true,
      stackNumber: 7,
      // Merging 241 merges 240 with it — both issues complete.
      stackMemberNumbers: [240, 241],
    })
  })

  it(`skips the legacy attempt entirely when the row already knows its stack`, async () => {
    const { calls } = install([
      { match: `/stacks?pull_request=241`, status: 200, body: [STACK_BODY] },
      {
        match: `/pulls/241/merge-async`,
        method: `PUT`,
        status: 202,
        body: { status: `merged`, sha: `abc`, details: { uuid: `u-1` } },
      },
    ])
    await mergePullRequestSmart({
      repo: `o/r`,
      prNumber: 241,
      token: `tok`,
      knownStackNumber: 7,
    })
    expect(calls.some((call) => call.url.endsWith(`/pulls/241/merge`))).toBe(
      false
    )
  })

  it(`still merges when the stack read degrades (404) — the member list is just this PR`, async () => {
    install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 405,
        body: { message: STACKED_REFUSAL },
      },
      { match: `/stacks?pull_request=241`, status: 404, body: {} },
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
    expect(result).toMatchObject({
      merged: true,
      stackNumber: null,
      stackMemberNumbers: [241],
    })
  })

  it(`reports an enqueued merge as success + queued`, async () => {
    install([
      {
        match: `/pulls/241/merge`,
        method: `PUT`,
        status: 405,
        body: { message: STACKED_REFUSAL },
      },
      { match: `/stacks?pull_request=241`, status: 200, body: [STACK_BODY] },
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
      { match: `/stacks?pull_request=241`, status: 200, body: [STACK_BODY] },
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
      { match: `/stacks?pull_request=241`, status: 200, body: [STACK_BODY] },
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
    expect(error).toMatchObject({ prNumber: 241, stackNumber: 7, uuid: `u-1` })
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
