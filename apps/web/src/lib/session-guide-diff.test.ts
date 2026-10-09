import { describe, expect, it, vi } from "vitest"
import { guideDiffPrUrl, loadGuideDiff } from "./session-guide-diff"

const h = vi.hoisted(() => ({
  fetchPullFiles: vi.fn(
    async (..._args: unknown[]) =>
      [{ filename: `src/a.ts`, previous_filename: undefined }] as Array<{
        filename: string
        previous_filename?: string
      }>
  ),
}))
vi.mock(`@/lib/integrations/github-pr`, () => ({ fetchPullFiles: h.fetchPullFiles }))
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoInstallationTokenInfo: vi.fn(async () => ({ token: `tok` })),
  fetchBranchDiff: vi.fn(),
}))

// EXP-1251: the diff a Guide write checks against never reads a repository
// the run did not open its PR in.
describe(`guideDiffPrUrl`, () => {
  const run = { prUrl: `https://github.com/o/r/pull/7`, branch: `exp/EXP-1`, boardId: `b1` }

  it(`reads the topic's PR in the run's repository`, () => {
    expect(guideDiffPrUrl(run, `https://github.com/o/r/pull/6`)).toBe(
      `https://github.com/o/r/pull/6`
    )
  })

  it(`falls back to the run's own PR for another repository or no tag`, () => {
    expect(guideDiffPrUrl(run, `https://github.com/evil/other/pull/1`)).toBe(run.prUrl)
    expect(guideDiffPrUrl(run, null)).toBe(run.prUrl)
  })

  it(`has no PR to read without a run PR`, () => {
    expect(guideDiffPrUrl({ ...run, prUrl: null }, `https://github.com/o/r/pull/6`)).toBeNull()
  })

  it(`reads nothing without a PR, a branch or a board`, async () => {
    expect(await loadGuideDiff({ prUrl: null, branch: null, boardId: `b1` }, null)).toBeNull()
    expect(await loadGuideDiff({ prUrl: null, branch: `exp/x`, boardId: null }, null)).toBeNull()
  })
})

// M19: the Guide check reads the PR's files FRESH, so a fix pushed seconds
// ago is seen (the 60s PR-files cache would answer the old list).
describe(`loadGuideDiff on a PR`, () => {
  it(`reads the PR files bypassing the cache, every call`, async () => {
    const run = { prUrl: `https://github.com/o/r/pull/7`, branch: `exp/EXP-1`, boardId: `b1` }
    expect(await loadGuideDiff(run, null)).toEqual([{ path: `src/a.ts`, previousPath: null }])
    h.fetchPullFiles.mockResolvedValueOnce([
      { filename: `src/b.ts`, previous_filename: `src/a.ts` },
    ])
    expect(await loadGuideDiff(run, null)).toEqual([
      { path: `src/b.ts`, previousPath: `src/a.ts` },
    ])
    expect(h.fetchPullFiles).toHaveBeenCalledTimes(2)
    for (const call of h.fetchPullFiles.mock.calls) {
      expect(call).toEqual([`o/r`, 7, `tok`, undefined, { fresh: true }])
    }
  })
})
