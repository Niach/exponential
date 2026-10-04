import { beforeEach, describe, expect, it, vi } from "vitest"

// Isolate the poll pass — never touch a real DB or GitHub. The select chain
// stub returns mockRows and records the where clause it was handed.
let mockRows: Array<{
  issueId: string
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  teamId: string
  // The recorded base the pass re-reads.
  prBaseBranch: string | null
}> = []
let capturedWhere: unknown = null
// The `pr_base_branch` rewrites the pass issued.
let baseWrites: Array<Record<string, unknown>> = []
// EXP-734: the second lane polls the chore PRs on coding_sessions rows
// (a join-less select) — the stub answers it from mockSessionRows.
let mockSessionRows: Array<{
  sessionId: string
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  teamId: string
}> = []
let capturedSessionWhere: unknown = null
vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        innerJoin: () => ({
          where: (clause: unknown) => {
            capturedWhere = clause
            return Promise.resolve(mockRows)
          },
        }),
        where: (clause: unknown) => {
          capturedSessionWhere = clause
          return Promise.resolve(mockSessionRows)
        },
      }),
    }),
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          baseWrites.push(values)
        },
      }),
    }),
  },
}))
vi.mock(`@/lib/integrations/github-pr`, () => ({
  fetchPullState: vi.fn(),
  resolveRepoToken: vi.fn(async () => `tok`),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrMergeState: vi.fn(),
  applyPrClosedState: vi.fn(),
  applyPrReopenedState: vi.fn(),
  applySessionPrState: vi.fn(),
}))

import { fetchPullState } from "@/lib/integrations/github-pr"
import {
  applyPrClosedState,
  applyPrMergeState,
  applyPrReopenedState,
  applySessionPrState,
} from "@/lib/integrations/pr-sync"
import {
  CLOSED_PR_RECHECK_WINDOW_MS,
  decidePrPollAction,
  parseRepoFromPrUrl,
  runPrPollPass,
} from "@/lib/bootstrap-self-hosted"

const PR_URL = `https://github.com/acme/app/pull/7`

function row(overrides: Partial<(typeof mockRows)[number]> = {}) {
  return {
    issueId: `i1`,
    prUrl: PR_URL,
    prNumber: 7,
    prState: `open` as string | null,
    teamId: `t1`,
    prBaseBranch: null as string | null,
    ...overrides,
  }
}

// An open PR as the poller's one read reports it; `baseRef` = its live base.
function openOnGitHub(baseRef: string | null = null) {
  return { state: `open` as const, merged: false, mergedBy: null,
      mergeCommitSha: null, baseRef }
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
    mockSessionRows = []
    capturedWhere = null
    capturedSessionWhere = null
    baseWrites = []
  })

  // EXP-734: the chore PR of an action/chat run lives on its session row.
  it(`polls session-owned PRs with the same transitions and window`, async () => {
    const now = new Date(`2026-09-04T12:00:00Z`)
    mockSessionRows = [
      { sessionId: `s1`, prUrl: PR_URL, prNumber: 7, prState: `open`, teamId: `t1` },
    ]
    vi.mocked(fetchPullState).mockResolvedValue({
      state: `closed`,
      merged: true,
      mergedBy: null,
      mergeCommitSha: null,
    baseRef: null,
    })
    await runPrPollPass(now)
    const params = sqlParams(capturedSessionWhere)
    expect(params).toContain(`open`)
    expect(params).toContain(`closed`)
    expect(params).toContainEqual(
      new Date(now.getTime() - CLOSED_PR_RECHECK_WINDOW_MS)
    )
    expect(applySessionPrState).toHaveBeenCalledWith({
      prUrl: PR_URL,
      state: `merged`,
    })
    expect(applyPrMergeState).not.toHaveBeenCalled()

    vi.mocked(applySessionPrState).mockClear()
    vi.mocked(fetchPullState).mockResolvedValue({
      state: `closed`,
      merged: false,
      mergedBy: null,
      mergeCommitSha: null,
    baseRef: null,
    })
    await runPrPollPass(now)
    expect(applySessionPrState).toHaveBeenCalledWith({
      prUrl: PR_URL,
      state: `closed`,
    })
  })

  it(`keeps recently-closed PRs in the fetch set within a bounded window`, async () => {
    const now = new Date(`2026-07-20T12:00:00Z`)
    await runPrPollPass(now)
    const params = sqlParams(capturedWhere)
    expect(params).toContain(`open`)
    expect(params).toContain(`closed`)
    expect(params).toContainEqual(
      new Date(now.getTime() - CLOSED_PR_RECHECK_WINDOW_MS)
    )
  })

  it(`heals a closed PR that is open again on GitHub`, async () => {
    mockRows = [row({ prState: `closed` })]
    vi.mocked(fetchPullState).mockResolvedValue({
      state: `open`,
      merged: false,
      mergedBy: null,
      mergeCommitSha: null,
    baseRef: null,
    })
    await runPrPollPass()
    expect(applyPrReopenedState).toHaveBeenCalledWith({
      issueId: `i1`,
      prUrl: PR_URL,
      baseBranch: null,
    })
    expect(applyPrMergeState).not.toHaveBeenCalled()
    expect(applyPrClosedState).not.toHaveBeenCalled()
  })

  it(`merges a closed-then-reopened PR that GitHub reports as merged`, async () => {
    mockRows = [row({ prState: `closed` })]
    vi.mocked(fetchPullState).mockResolvedValue({
      state: `closed`,
      merged: true,
      mergedBy: null,
      mergeCommitSha: null,
    baseRef: null,
    })
    await runPrPollPass()
    expect(applyPrMergeState).toHaveBeenCalledWith(
      expect.objectContaining({ issueId: `i1`, prUrl: PR_URL })
    )
    expect(applyPrReopenedState).not.toHaveBeenCalled()
  })

  // The close cleared the base on both rows; the reopen restores the live one
  // so a stacked PR is guarded again.
  it(`reopens with the live base on the issue and the session rows`, async () => {
    mockRows = [row({ prState: `closed` })]
    mockSessionRows = [
      { sessionId: `s1`, prUrl: PR_URL, prNumber: 7, prState: `closed`, teamId: `t1` },
    ]
    vi.mocked(fetchPullState).mockResolvedValue(openOnGitHub(`exp/EXP-1`))
    await runPrPollPass()
    expect(applyPrReopenedState).toHaveBeenCalledWith({
      issueId: `i1`,
      prUrl: PR_URL,
      baseBranch: `exp/EXP-1`,
    })
    expect(applySessionPrState).toHaveBeenCalledWith({
      prUrl: PR_URL,
      state: `open`,
      baseBranch: `exp/EXP-1`,
    })
  })

  it(`flips an open PR closed without merging`, async () => {
    mockRows = [row()]
    vi.mocked(fetchPullState).mockResolvedValue({
      state: `closed`,
      merged: false,
      mergedBy: null,
      mergeCommitSha: null,
    baseRef: null,
    })
    await runPrPollPass()
    expect(applyPrClosedState).toHaveBeenCalledWith({
      issueId: `i1`,
      prUrl: PR_URL,
    })
  })

  it(`writes nothing for a PR still open on both sides`, async () => {
    mockRows = [row()]
    vi.mocked(fetchPullState).mockResolvedValue({
      state: `open`,
      merged: false,
      mergedBy: null,
      mergeCommitSha: null,
    baseRef: null,
    })
    await runPrPollPass()
    expect(applyPrMergeState).not.toHaveBeenCalled()
    expect(applyPrClosedState).not.toHaveBeenCalled()
    expect(applyPrReopenedState).not.toHaveBeenCalled()
    // A PR without a recorded base gets no base write.
    expect(baseWrites).toEqual([])
  })

  // The `edited` webhook that mirrors a retarget never reaches a polling
  // instance, so the pass takes the base of every open PR with a recorded one
  // off its ONE state read and writes it back when it moved.
  it(`rewrites a PR's base when GitHub moved it, off the one state read`, async () => {
    mockRows = [
      row({ issueId: `i1`, prBaseBranch: `exp/EXP-1` }),
      // A batch sibling on the same PR shares the read.
      row({ issueId: `i2`, prBaseBranch: `exp/EXP-1` }),
    ]
    vi.mocked(fetchPullState).mockResolvedValue(openOnGitHub(`master`))
    await runPrPollPass()
    // The base rides the state read: no second GitHub call per PR.
    expect(fetchPullState).toHaveBeenCalledTimes(1)
    expect(fetchPullState).toHaveBeenCalledWith(`acme/app`, 7, `tok`)
    expect(baseWrites.map((write) => write.prBaseBranch)).toEqual([
      `master`,
      `master`,
    ])
    expect(applyPrMergeState).not.toHaveBeenCalled()
  })

  it(`leaves a PR's base alone while GitHub still agrees, and skips closed PRs`, async () => {
    mockRows = [
      row({ issueId: `i1`, prBaseBranch: `exp/EXP-1` }),
      row({
        issueId: `i2`,
        prUrl: `https://github.com/acme/app/pull/8`,
        prNumber: 8,
        prBaseBranch: `exp/EXP-1`,
      }),
    ]
    vi.mocked(fetchPullState)
      .mockResolvedValueOnce(openOnGitHub(`exp/EXP-1`))
      .mockResolvedValueOnce({
        state: `closed`,
        merged: false,
        mergedBy: null,
        mergeCommitSha: null,
        // A closed PR's base is never mirrored, whatever it says.
        baseRef: `master`,
      })
    await runPrPollPass()
    expect(baseWrites).toEqual([])
    expect(applyPrClosedState).toHaveBeenCalledWith({
      issueId: `i2`,
      prUrl: `https://github.com/acme/app/pull/8`,
    })
  })

  it(`fetches a batch PR's state once and applies it to every linked issue`, async () => {
    mockRows = [row({ issueId: `i1` }), row({ issueId: `i2` })]
    vi.mocked(fetchPullState).mockResolvedValue({
      state: `closed`,
      merged: true,
      mergedBy: null,
      mergeCommitSha: null,
    baseRef: null,
    })
    await runPrPollPass()
    expect(fetchPullState).toHaveBeenCalledTimes(1)
    expect(applyPrMergeState).toHaveBeenCalledTimes(2)
  })

  it(`keeps polling the remaining rows when one PR fetch throws`, async () => {
    mockRows = [
      row({ issueId: `i1`, prUrl: `https://github.com/acme/app/pull/1` }),
      row({ issueId: `i2`, prUrl: `https://github.com/acme/app/pull/2` }),
    ]
    vi.mocked(fetchPullState)
      .mockRejectedValueOnce(new Error(`boom`))
      .mockResolvedValueOnce({
        state: `closed`,
        merged: true,
        mergedBy: null,
      mergeCommitSha: null,
        baseRef: null,
      })
    const spy = vi.spyOn(console, `error`).mockImplementation(() => {})
    await runPrPollPass()
    expect(applyPrMergeState).toHaveBeenCalledTimes(1)
    spy.mockRestore()
  })
})
