import { describe, expect, it } from "vitest"
import {
  buildPrOptions,
  findPrOptionForIssue,
  type PrOptionIssue,
} from "./pr-options"

// EXP-259: the `pr` input's options are the team's OPEN issue-linked pull
// requests deduped by prUrl — a batch PR (several issues, one PR) appears
// ONCE, carrying a representative issue id and listing every linked
// identifier. Parity with the native builders (`buildPullRequestOptions` on
// Android, `StartPullRequestOption.build` on iOS).

function issue(over: Partial<PrOptionIssue> & { id: string }): PrOptionIssue {
  return {
    boardId: `b-1`,
    identifier: `EXP-1`,
    prUrl: `https://github.com/acme/web/pull/1`,
    prNumber: 1,
    prState: `open`,
    ...over,
  }
}

const BATCH_URL = `https://github.com/acme/web/pull/7`

const BATCH_ROWS = [
  issue({ id: `i-2`, identifier: `EXP-2`, prUrl: BATCH_URL, prNumber: 7 }),
  issue({ id: `i-1`, identifier: `EXP-1`, prUrl: BATCH_URL, prNumber: 7 }),
  issue({
    id: `i-3`,
    identifier: `EXP-3`,
    prUrl: `https://github.com/acme/web/pull/9`,
    prNumber: 9,
  }),
]

describe(`buildPrOptions`, () => {
  it(`dedupes a batch pull request into one option`, () => {
    const options = buildPrOptions(BATCH_ROWS, new Set([`b-1`]))

    expect(options).toHaveLength(2)
    const batch = options.find((option) => option.identifiers.length === 2)!
    expect(batch.identifiers).toEqual([`EXP-1`, `EXP-2`])
    // Representative id is deterministic (lowest id), not query-order bound.
    expect(batch.issueId).toBe(`i-1`)
    expect(batch.label).toBe(`#7 · EXP-1, EXP-2`)
    expect([...batch.linkedIssueIds].sort()).toEqual([`i-1`, `i-2`])
  })

  it(`skips other teams, non-open PRs and rows without a url`, () => {
    const options = buildPrOptions(
      [
        issue({ id: `mine` }),
        issue({ id: `other-team`, boardId: `b-9` }),
        issue({ id: `merged`, prState: `merged` }),
        issue({ id: `no-url`, prUrl: null, prNumber: null }),
      ],
      new Set([`b-1`])
    )

    expect(options.map((option) => option.issueId)).toEqual([`mine`])
  })

  it(`falls back to the identifiers when the pr number is missing`, () => {
    const options = buildPrOptions(
      [issue({ id: `i-1`, prNumber: null })],
      new Set([`b-1`])
    )

    expect(options[0].label).toBe(`EXP-1`)
  })
})

describe(`findPrOptionForIssue`, () => {
  // EXP-323: a Reviews row hands over its OWN representative (the newest
  // linked issue), which is not this builder's (lowest id) — both must land on
  // the same option, whose canonical id is what the picker renders.
  it(`resolves any linked issue id to the representative option`, () => {
    const options = buildPrOptions(BATCH_ROWS, new Set([`b-1`]))

    expect(findPrOptionForIssue(options, `i-2`)?.issueId).toBe(`i-1`)
    expect(findPrOptionForIssue(options, `i-1`)?.issueId).toBe(`i-1`)
    expect(findPrOptionForIssue(options, `i-3`)?.issueId).toBe(`i-3`)
    expect(findPrOptionForIssue(options, `unknown`)).toBeNull()
  })
})
