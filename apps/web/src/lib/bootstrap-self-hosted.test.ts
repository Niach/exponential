import { beforeEach, describe, expect, it, vi } from "vitest"

// Isolate the poll pass — never touch a real DB or GitHub. The select chain
// stub returns mockRows (pull_requests rows) and records its where clause.
let mockRows: Array<{
  id: string
  url: string
  number: number
  state: string
  base: string
  teamId: string
}> = []
let capturedWhere: unknown = null
vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        where: (clause: unknown) => {
          capturedWhere = clause
          return Promise.resolve(mockRows)
        },
      }),
    }),
  },
}))
vi.mock(`@/lib/integrations/github-pr`, () => ({
  fetchPullState: vi.fn(),
  resolveRepoToken: vi.fn(async () => `tok`),
}))
vi.mock(`@/lib/integrations/github-identity`, () => ({
  resolveAppUserForGithubActor: vi.fn(async () => null),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPullRequestBase: vi.fn(),
  applyPullRequestClosed: vi.fn(),
  applyPullRequestMerged: vi.fn(),
  applyPullRequestReopened: vi.fn(),
}))

import { fetchPullState } from "@/lib/integrations/github-pr"
import {
  applyPullRequestBase,
  applyPullRequestClosed,
  applyPullRequestMerged,
  applyPullRequestReopened,
} from "@/lib/integrations/pr-sync"
import {
  CLOSED_PR_RECHECK_WINDOW_MS,
  decidePrPollAction,
  parseRepoFromPrUrl,
  runPrPollPass,
} from "@/lib/bootstrap-self-hosted"

const PR_URL = `https://github.com/acme/app/pull/7`

function row(overrides: Partial<(typeof mockRows)[number]> = {}) {
  return { id: `pr1`, url: PR_URL, number: 7, state: `open`, base: `master`, teamId: `t1`, ...overrides }
}

function onGitHub(over: { state?: `open` | `closed`; merged?: boolean; baseRef?: string | null } = {}) {
  return {
    state: over.state ?? (`open` as const),
    merged: over.merged ?? false,
    mergedBy: null,
    mergeCommitSha: null,
    baseRef: over.baseRef === undefined ? `master` : over.baseRef,
  }
}

// Flatten a drizzle SQL tree down to its bound parameter values.
function sqlParams(chunk: unknown, out: unknown[] = []): unknown[] {
  if (!chunk || typeof chunk !== `object`) return out
  const node = chunk as { queryChunks?: unknown[]; value?: unknown }
  if (Array.isArray(node.queryChunks)) {
    for (const sub of node.queryChunks) sqlParams(sub, out)
  } else if (`value` in node) {
    out.push(node.value)
  }
  return out
}

describe(`parseRepoFromPrUrl`, () => {
  it(`extracts owner/repo from a PR url`, () => {
    expect(parseRepoFromPrUrl(PR_URL)).toBe(`acme/app`)
  })

  it(`returns null for a non-PR url`, () => {
    expect(parseRepoFromPrUrl(`https://example.com/acme/app`)).toBe(null)
  })
})

describe(`decidePrPollAction`, () => {
  it(`merges an open PR GitHub reports as merged`, () => {
    expect(decidePrPollAction(`open`, { state: `closed`, merged: true })).toBe(
      `merge`
    )
  })

  it(`merges a locally-closed PR that was reopened and merged on GitHub`, () => {
    // REV2-74: the reopen may be missed entirely (no webhook on a polling
    // instance) — the merge must still complete the linked issues.
    expect(decidePrPollAction(`closed`, { state: `closed`, merged: true })).toBe(
      `merge`
    )
  })

  it(`heals a locally-closed PR that is open again on GitHub`, () => {
    expect(decidePrPollAction(`closed`, { state: `open`, merged: false })).toBe(
      `reopen`
    )
  })

  it(`closes an open PR closed without merging`, () => {
    expect(decidePrPollAction(`open`, { state: `closed`, merged: false })).toBe(
      `close`
    )
  })

  it(`closes a draft closed without merging`, () => {
    expect(decidePrPollAction(`draft`, { state: `closed`, merged: false })).toBe(`close`)
  })

  it(`does nothing when local and GitHub state already agree`, () => {
    expect(decidePrPollAction(`open`, { state: `open`, merged: false })).toBe(
      `none`
    )
    expect(
      decidePrPollAction(`closed`, { state: `closed`, merged: false })
    ).toBe(`none`)
    expect(decidePrPollAction(`merged`, { state: `closed`, merged: true })).toBe(
      `none`
    )
  })
})

describe(`runPrPollPass`, () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockRows = []
    capturedWhere = null
  })

  it(`polls live and recently-closed PRs within a bounded window`, async () => {
    const now = new Date(`2026-07-20T12:00:00Z`)
    await runPrPollPass(now)
    const params = sqlParams(capturedWhere)
    expect(params).toContain(`closed`)
    expect(params).toContainEqual(new Date(now.getTime() - CLOSED_PR_RECHECK_WINDOW_MS))
  })

  it(`merges a PR GitHub reports as merged`, async () => {
    mockRows = [row()]
    vi.mocked(fetchPullState).mockResolvedValue(onGitHub({ state: `closed`, merged: true }))
    await runPrPollPass()
    expect(applyPullRequestMerged).toHaveBeenCalledWith(
      expect.objectContaining({ pullRequestId: `pr1`, actorUserId: null, githubActorUserId: null })
    )
  })

  it(`closes a live (draft too) PR closed without merging`, async () => {
    mockRows = [row({ state: `draft` })]
    vi.mocked(fetchPullState).mockResolvedValue(onGitHub({ state: `closed` }))
    await runPrPollPass()
    expect(applyPullRequestClosed).toHaveBeenCalledWith({ pullRequestId: `pr1` })
  })

  it(`heals a closed PR that is open again on GitHub, with its live base`, async () => {
    mockRows = [row({ state: `closed` })]
    vi.mocked(fetchPullState).mockResolvedValue(onGitHub({ baseRef: `exp/EXP-3` }))
    await runPrPollPass()
    expect(applyPullRequestReopened).toHaveBeenCalledWith({ pullRequestId: `pr1`, base: `exp/EXP-3` })
    expect(applyPullRequestMerged).not.toHaveBeenCalled()
  })

  it(`stores a base GitHub moved, off the one state read`, async () => {
    mockRows = [row({ base: `exp/EXP-1` })]
    vi.mocked(fetchPullState).mockResolvedValue(onGitHub({ baseRef: `master` }))
    await runPrPollPass()
    expect(applyPullRequestBase).toHaveBeenCalledWith({ pullRequestId: `pr1`, base: `master` })
    expect(fetchPullState).toHaveBeenCalledTimes(1)
  })

  it(`writes nothing for a PR still open on both sides`, async () => {
    mockRows = [row()]
    vi.mocked(fetchPullState).mockResolvedValue(onGitHub())
    await runPrPollPass()
    expect(applyPullRequestBase).not.toHaveBeenCalled()
    expect(applyPullRequestMerged).not.toHaveBeenCalled()
    expect(applyPullRequestClosed).not.toHaveBeenCalled()
    expect(applyPullRequestReopened).not.toHaveBeenCalled()
  })

  it(`keeps polling the remaining rows when one PR fetch throws`, async () => {
    mockRows = [row({ id: `bad`, url: `https://github.com/acme/app/pull/1`, number: 1 }), row()]
    vi.mocked(fetchPullState)
      .mockRejectedValueOnce(new Error(`boom`))
      .mockResolvedValueOnce(onGitHub({ state: `closed`, merged: true }))
    const error = vi.spyOn(console, `error`).mockImplementation(() => {})
    await runPrPollPass()
    error.mockRestore()
    expect(applyPullRequestMerged).toHaveBeenCalledWith(expect.objectContaining({ pullRequestId: `pr1` }))
  })
})
