import { describe, expect, it } from "vitest"
import {
  membersAtOrBelow,
  memberAbove,
  orderStack,
  prUrlPattern,
  stackPosition,
  stackTop,
  stackTopOpen,
  toStackEntries,
  type StackRow,
} from "@/lib/integrations/pr-stack"

// EXP-897: the SERVER's stack model — derived from `pr_base_branch` edges
// alone, so it works on a repo where GitHub's stack preview is unavailable.

const REPO = `owner/repo`
const url = (n: number) => `https://github.com/${REPO}/pull/${n}`

function row(
  n: number,
  opts: Partial<StackRow> & { identifier: string }
): StackRow {
  return {
    id: `issue-${opts.identifier}`,
    title: opts.identifier,
    status: `in_review`,
    branch: `exp/${opts.identifier}`,
    prUrl: url(n),
    prNumber: n,
    prState: `open`,
    prBaseBranch: null,
    prStackNumber: null,
    ...opts,
  }
}

// EXP-10 ← EXP-11 ← EXP-12, bottom first.
const CHAIN: StackRow[] = [
  row(242, { identifier: `EXP-12`, prBaseBranch: `exp/EXP-11` }),
  row(240, { identifier: `EXP-10`, prBaseBranch: `master` }),
  row(241, { identifier: `EXP-11`, prBaseBranch: `exp/EXP-10` }),
]

describe(`orderStack`, () => {
  it(`orders the whole chain bottom-up from any member`, () => {
    for (const from of [240, 241, 242]) {
      expect(
        orderStack(CHAIN, url(from)).map((entry) => entry.prNumber)
      ).toEqual([240, 241, 242])
    }
  })

  it(`stops at a base nobody in the set owns`, () => {
    const chain = orderStack(CHAIN, url(240))
    expect(chain[0]!.baseBranch).toBe(`master`)
    expect(chain).toHaveLength(3)
  })

  it(`groups a batch PR's issues into ONE entry`, () => {
    const batch = [
      row(250, { identifier: `EXP-20`, branch: `exp/batch-1`, prBaseBranch: `master` }),
      { ...row(250, { identifier: `EXP-21`, branch: `exp/batch-1`, prBaseBranch: `master` }), id: `issue-EXP-21` },
      row(251, { identifier: `EXP-22`, prBaseBranch: `exp/batch-1` }),
    ]
    const chain = orderStack(batch, url(251))
    expect(chain.map((entry) => entry.prNumber)).toEqual([250, 251])
    expect(chain[0]!.issues.map((issue) => issue.identifier)).toEqual([
      `EXP-20`,
      `EXP-21`,
    ])
  })

  // FEED-43 R1: a closed-without-merge PR keeps its rows (and, until its
  // edge is cleared, its base), so "first match" could hang the chain on a
  // dead member while the live one on the same edge stayed out of it.
  it(`prefers an open PR over a closed one on the same edge, on both walks`, () => {
    const rows: StackRow[] = [
      row(240, { identifier: `EXP-1`, prBaseBranch: `master` }),
      // EXP-2 stacked on EXP-1, closed unmerged; EXP-3 stacked on EXP-1, open.
      row(241, { identifier: `EXP-2`, prBaseBranch: `exp/EXP-1`, prState: `closed` }),
      row(242, { identifier: `EXP-3`, prBaseBranch: `exp/EXP-1` }),
    ]
    expect(orderStack(rows, url(240)).map((entry) => entry.prNumber)).toEqual([
      240, 242,
    ])
    expect(stackTopOpen(orderStack(rows, url(240)))?.prNumber).toBe(242)

    // Below: two PRs own the branch `exp/EXP-1` (an old closed one and the
    // live one); the open one is the foundation.
    const rebased: StackRow[] = [
      { ...row(239, { identifier: `EXP-1`, prBaseBranch: `master`, prState: `closed` }), id: `issue-EXP-1-old` },
      row(240, { identifier: `EXP-1`, prBaseBranch: `master` }),
      row(242, { identifier: `EXP-3`, prBaseBranch: `exp/EXP-1` }),
    ]
    expect(orderStack(rebased, url(242)).map((entry) => entry.prNumber)).toEqual([
      240, 242,
    ])
  })

  it(`still walks through a merged foundation when nothing open owns the edge`, () => {
    const rows: StackRow[] = [
      row(240, { identifier: `EXP-1`, prBaseBranch: `master`, prState: `merged` }),
      row(242, { identifier: `EXP-3`, prBaseBranch: `exp/EXP-1` }),
    ]
    expect(orderStack(rows, url(242)).map((entry) => entry.prNumber)).toEqual([
      240, 242,
    ])
  })

  it(`breaks a cycle where it first repeats`, () => {
    const cyclic: StackRow[] = [
      row(240, { identifier: `EXP-10`, prBaseBranch: `exp/EXP-11` }),
      row(241, { identifier: `EXP-11`, prBaseBranch: `exp/EXP-10` }),
    ]
    const chain = orderStack(cyclic, url(240))
    expect(chain).toHaveLength(2)
    expect(new Set(chain.map((entry) => entry.prNumber)).size).toBe(2)
  })

  it(`answers an unknown PR with an empty chain`, () => {
    expect(orderStack(CHAIN, url(999))).toEqual([])
  })

  it(`never treats an empty branch as everyone's foundation`, () => {
    const rows: StackRow[] = [
      row(240, { identifier: `EXP-10`, branch: ``, prBaseBranch: `master` }),
      row(241, { identifier: `EXP-11`, prBaseBranch: `` }),
    ]
    expect(orderStack(rows, url(241)).map((e) => e.prNumber)).toEqual([241])
  })
})

describe(`chain navigation`, () => {
  const chain = orderStack(CHAIN, url(241))

  it(`returns the members at or below a PR`, () => {
    expect(
      membersAtOrBelow(chain, url(241)).map((entry) => entry.prNumber)
    ).toEqual([240, 241])
  })

  it(`names the member above, and null at the top`, () => {
    expect(memberAbove(chain, url(241))?.prNumber).toBe(242)
    expect(memberAbove(chain, url(242))).toBeNull()
  })

  it(`numbers a member from the bottom of the chain`, () => {
    expect(stackPosition(chain, url(241))).toEqual({ position: 2, size: 3 })
    expect(stackPosition(chain, url(999))).toBeNull()
  })

  it(`takes the topmost OPEN member as the merge target`, () => {
    expect(stackTop(chain)?.prNumber).toBe(242)
    const merged = orderStack(
      CHAIN.map((each) =>
        each.prNumber === 242 ? { ...each, prState: `merged` } : each
      ),
      url(241)
    )
    expect(stackTopOpen(merged)?.prNumber).toBe(241)
    const allMerged = orderStack(
      CHAIN.map((each) => ({ ...each, prState: `merged` })),
      url(241)
    )
    expect(stackTopOpen(allMerged)).toBeNull()
  })
})

describe(`toStackEntries`, () => {
  it(`skips rows with no PR and keeps the batch's stack number`, () => {
    const entries = toStackEntries([
      { ...row(250, { identifier: `EXP-20` }), prStackNumber: null },
      { ...row(250, { identifier: `EXP-21` }), prStackNumber: 7 },
      { ...row(0, { identifier: `EXP-22` }), prUrl: null },
    ])
    expect(entries).toHaveLength(1)
    expect(entries[0]!.stackNumber).toBe(7)
  })
})

describe(`prUrlPattern`, () => {
  it(`matches one repository's PR urls and escapes LIKE metacharacters`, () => {
    expect(prUrlPattern(REPO)).toBe(`https://github.com/owner/repo/pull/%`)
    expect(prUrlPattern(`o/re_po`)).toBe(
      `https://github.com/o/re\\_po/pull/%`
    )
  })
})
