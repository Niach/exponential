import { describe, expect, it } from "vitest"
import { guideDiffPrUrl, loadGuideDiff } from "./session-guide-diff"

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
