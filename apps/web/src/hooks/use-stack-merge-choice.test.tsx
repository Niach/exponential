import { describe, expect, it, vi } from "vitest"
import type { PrState } from "@exp/db-schema/domain"

vi.mock(`@/lib/collections`, () => ({ pullRequestCollection: {} }))

import { pullRequestStackConfirm } from "@/hooks/use-stack-merge-choice"

const pr = (id: string, number: number, head: string, base: string, state: PrState = `open`) => ({
  id,
  number,
  head,
  base,
  state,
  url: `https://github.com/acme/app/pull/${number}`,
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
