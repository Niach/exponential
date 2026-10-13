import { describe, expect, it, vi } from "vitest"
import type { PrState } from "@exp/db-schema/domain"

vi.mock(`@/lib/collections`, () => ({ pullRequestCollection: {} }))

import { threadRowWord } from "@/components/thread/threads-list"

const pr = (id: string, state: PrState, basePullRequestId: string | null = null) => ({
  id,
  state,
  basePullRequestId,
})

// EXP-1293: ONE quiet word per Threads row, off the root's pull requests.
describe(`threadRowWord`, () => {
  const idle = { busy: false, needsYou: false }

  it(`an open PR while nothing works is ready to merge`, () => {
    expect(threadRowWord(idle, [pr(`a`, `open`)])).toBe(`readyToMerge`)
    expect(threadRowWord({ busy: true, needsYou: false }, [pr(`a`, `open`)])).toBeNull()
  })

  it(`an open PR on another open PR (either side) is a stack`, () => {
    const lower = pr(`a`, `open`)
    const upper = pr(`b`, `open`, `a`)
    expect(threadRowWord(idle, [upper], [lower, upper])).toBe(`stack`)
    expect(threadRowWord(idle, [lower], [lower, upper])).toBe(`stack`)
    expect(threadRowWord(idle, [upper], [pr(`a`, `merged`), upper])).toBe(`readyToMerge`)
  })

  it(`merged wins over a question; a question without a PR`, () => {
    expect(threadRowWord({ busy: false, needsYou: true }, [pr(`a`, `merged`)])).toBe(`merged`)
    expect(threadRowWord({ busy: false, needsYou: true }, [])).toBe(`question`)
    expect(threadRowWord(idle, [])).toBeNull()
  })
})
