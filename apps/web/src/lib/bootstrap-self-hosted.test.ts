import { beforeEach, describe, expect, it, vi } from "vitest"

// Isolate the poll pass — never touch a real DB or GitHub. The select chain
// stub returns mockRows and records the where clause it was handed.
let mockRows: Array<{
  issueId: string
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  teamId: string
  // EXP-897: the stack columns the pass re-reads the base for.
  prBaseBranch: string | null
  prStackNumber: number | null
}> = []
let capturedWhere: unknown = null
// EXP-897: the `pr_base_branch` rewrites the pass issued.
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
// EXP-982: the third lane's predicate (the workflows' final PRs).
let capturedWorkflowWhere: unknown = null
vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: (table: Record<symbol, unknown>) => ({
        innerJoin: () => ({
          where: (clause: unknown) => {
            capturedWhere = clause
            return Promise.resolve(mockRows)
          },
        }),
        where: (clause: unknown) => {
          // EXP-982: the THIRD lane (workflows' final PRs) is join-less too;
          // told apart by the table the select reads. No workflow rows here.
          if (table[Symbol.for(`drizzle:Name`)] === `workflows`) {
            capturedWorkflowWhere = clause
            return Promise.resolve([])
          }
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
vi.mock(`@/lib/workflow-final-pr`, () => ({
  applyWorkflowFinalPrState: vi.fn(),
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
    prStackNumber: null as number | null,
    ...overrides,
  }
}

// An open PR as the poller's one read reports it; `baseRef` = its live base.
function openOnGitHub(baseRef: string | null = null) {
  return { state: `open` as const, merged: false, mergedBy: null, baseRef }
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
    capturedWorkflowWhere = null
    baseWrites = []
  })

  // REV2-74 for the final PR lane too: a closed final PR is re-checked only
  // within the window, never forever.
  it(`polls workflow final PRs: open ones, and closed ones within the window`, async () => {
    const now = new Date(`2026-09-04T12:00:00Z`)
    await runPrPollPass(now)
    const params = sqlParams(capturedWorkflowWhere)
    expect(params).toContain(`open`)
    expect(params).toContain(`closed`)
    expect(params).toContainEqual(new Date(now.getTime() - CLOSED_PR_RECHECK_WINDOW_MS))
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
    baseRef: null,
    })
    await runPrPollPass()
    expect(applyPrReopenedState).toHaveBeenCalledWith({
      issueId: `i1`,
      prUrl: PR_URL,
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
    baseRef: null,
    })
    await runPrPollPass()
    expect(applyPrMergeState).toHaveBeenCalledWith(
      expect.objectContaining({ issueId: `i1`, prUrl: PR_URL })
    )
    expect(applyPrReopenedState).not.toHaveBeenCalled()
  })

  it(`flips an open PR closed without merging`, async () => {
    mockRows = [row()]
    vi.mocked(fetchPullState).mockResolvedValue({
      state: `closed`,
      merged: false,
      mergedBy: null,
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
    baseRef: null,
    })
    await runPrPollPass()
    expect(applyPrMergeState).not.toHaveBeenCalled()
    expect(applyPrClosedState).not.toHaveBeenCalled()
    expect(applyPrReopenedState).not.toHaveBeenCalled()
    // An unstacked PR gets no base write.
    expect(baseWrites).toEqual([])
  })

  // EXP-897 (FEED-43 R1): the `edited` webhook that mirrors GitHub's retarget
  // of a stack member never reaches a polling instance, so the pass takes the
  // base of every open member off its ONE state read and writes it back when
  // it moved.
  it(`rewrites a stack member's base when GitHub moved it, once per PR`, async () => {
    mockRows = [
      row({ issueId: `i1`, prBaseBranch: `exp/EXP-1`, prStackNumber: 3 }),
      // A batch sibling on the same PR shares the read.
      row({ issueId: `i2`, prBaseBranch: `exp/EXP-1`, prStackNumber: 3 }),
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

  it(`leaves a stack member's base alone while GitHub still agrees, and skips closed PRs`, async () => {
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
        baseRef: null,
      })
    const spy = vi.spyOn(console, `error`).mockImplementation(() => {})
    await runPrPollPass()
    expect(applyPrMergeState).toHaveBeenCalledTimes(1)
    spy.mockRestore()
  })
})
