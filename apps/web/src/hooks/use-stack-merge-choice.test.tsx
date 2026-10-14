import { describe, expect, it, vi } from "vitest"
import type { PrState } from "@exp/db-schema/domain"

vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  pullRequestCollection: {},
  pullRequestIssueCollection: {},
}))

import { pullRequestStackConfirm, pullRequestStackView } from "@/hooks/use-stack-merge-choice"

const pr = (id: string, number: number, head: string, base: string, state: PrState = `open`) => ({
  id,
  number,
  head,
  base,
  state,
  url: `https://github.com/acme/app/pull/${number}`,
  title: `PR ${number}`,
})

// EXP-1293: the stack confirm over `pull_requests` rows (head/base edges).
describe(`pullRequestStackConfirm`, () => {
  const lower = pr(`p1`, 11, `exp/APP-1`, `main`)
  const upper = pr(`p2`, 12, `exp/APP-2`, `exp/APP-1`)

  it(`a lone PR merges plainly`, () => {
    expect(pullRequestStackConfirm(lower, [lower])).toBeNull()
  })

  it(`a stack member lands the chain through its top`, () => {
    const confirm = pullRequestStackConfirm(lower, [lower, upper])
    expect(confirm?.landing).toEqual([`#11`, `#12`])
    expect(confirm?.input).toEqual({ issueId: `p2`, mergeStack: true })
  })

  it(`a merged lower PR leaves the upper one alone`, () => {
    expect(pullRequestStackConfirm(upper, [pr(`p1`, 11, `exp/APP-1`, `main`, `merged`), upper])).toBeNull()
  })
})

// EXP-1306: the thread Guide's Stack card rows, named like the desktop's.
describe(`pullRequestStackView`, () => {
  const lower = pr(`p1`, 11, `exp/APP-14`, `main`)
  const upper = pr(`p2`, 12, `exp/APP-15`, `exp/APP-14`)
  const issues: Record<string, { identifier: string; title: string }[]> = {
    p1: [{ identifier: `APP-14`, title: `Group board issues by assignee` }],
    p2: [
      { identifier: `APP-16`, title: `Later` },
      { identifier: `APP-15`, title: `Batch-edit labels from the board` },
    ],
  }

  it(`top first, the subject current, the lowest identifier names a row`, () => {
    expect(pullRequestStackView(upper, [lower, upper], (id) => issues[id] ?? [])).toEqual({
      rows: [
        { issueId: `p2`, identifier: `APP-15`, title: `Batch-edit labels from the board`, prNumber: 12, isCurrent: true },
        { issueId: `p1`, identifier: `APP-14`, title: `Group board issues by assignee`, prNumber: 11, isCurrent: false },
      ],
      baseBranch: `main`,
    })
  })

  it(`an issue-less PR reads #number and its title; a lone PR has no rail`, () => {
    expect(pullRequestStackView(upper, [lower, upper], () => [])?.rows[0]).toMatchObject({
      identifier: `#12`,
      title: `PR 12`,
    })
    expect(pullRequestStackView(lower, [lower], () => [])).toBeNull()
  })
})
