import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-897: the stack PLAN — the transitive `blocks` chain a stacked run is
// built on, and the ONE place a blocking cycle is refused.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  insertRelationInTx: vi.fn(async () => null),
}))

vi.mock(`@/db/connection`, () => ({
  db: { select: () => ({ from: () => ({ where: async () => [] }) }) },
}))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/issue-relations`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/issue-relations")>()
  return { ...actual, insertRelationInTx: h.insertRelationInTx }
})

import { orderBlockerChain, resolveStackChain, StackCycleError } from "@/lib/stack-plan"

// Each select() consumes the next queued result set, in call order.
const db = {
  select: vi.fn(() => {
    const rows = h.selectQueue.shift() ?? []
    const builder: Record<string, unknown> = {}
    Object.assign(builder, {
      from: () => builder,
      innerJoin: () => builder,
      where: () => builder,
      limit: async () => rows,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      then: (res: any, rej: any) => Promise.resolve(rows).then(res, rej),
    })
    return builder
  }),
} as never

const REPO = {
  repositoryId: `repo-1`,
  fullName: `owner/repo`,
  defaultBranch: `master`,
  defaultBranchOverride: null,
  boardDefaultBranch: null,
}

function issueRow(identifier: string, over: Record<string, unknown> = {}) {
  return {
    id: `issue-${identifier}`,
    identifier,
    title: identifier,
    status: `in_progress`,
    boardId: `board-1`,
    teamId: `ws-1`,
    branch: `exp/${identifier}`,
    prUrl: null,
    prNumber: null,
    prState: null,
    ...over,
  }
}

beforeEach(() => {
  h.selectQueue.length = 0
  vi.clearAllMocks()
})

describe(`orderBlockerChain`, () => {
  const open = () => true
  const identifierOf = (id: string) => id

  it(`orders transitive blockers bottom-up`, () => {
    // A blocks B, B blocks C → starting C stacks on B on A.
    expect(
      orderBlockerChain(
        `C`,
        [
          { issueId: `B`, relatedIssueId: `C` },
          { issueId: `A`, relatedIssueId: `B` },
        ],
        { open, identifierOf }
      )
    ).toEqual([`A`, `B`])
  })

  it(`drops a blocker that is done, cancelled or a duplicate — and its own blockers`, () => {
    expect(
      orderBlockerChain(
        `C`,
        [
          { issueId: `B`, relatedIssueId: `C` },
          { issueId: `A`, relatedIssueId: `B` },
        ],
        { open: (id) => id !== `B`, identifierOf }
      )
    ).toEqual([])
  })

  it(`refuses a cycle by name`, () => {
    expect(() =>
      orderBlockerChain(
        `A`,
        [
          { issueId: `B`, relatedIssueId: `A` },
          { issueId: `A`, relatedIssueId: `B` },
        ],
        { open, identifierOf }
      )
    ).toThrow(StackCycleError)
    try {
      orderBlockerChain(
        `A`,
        [
          { issueId: `B`, relatedIssueId: `A` },
          { issueId: `A`, relatedIssueId: `B` },
        ],
        { open, identifierOf }
      )
    } catch (err) {
      expect((err as Error).message).toBe(
        `Blocking cycle: A → B → A. Fix the relations before stacking.`
      )
    }
  })

  it(`keeps several blockers of one issue, ordered by identifier`, () => {
    expect(
      orderBlockerChain(
        `C`,
        [
          { issueId: `EXP-9`, relatedIssueId: `C` },
          { issueId: `EXP-2`, relatedIssueId: `C` },
        ],
        { open, identifierOf }
      )
    ).toEqual([`EXP-2`, `EXP-9`])
  })
})

describe(`resolveStackChain`, () => {
  it(`returns the chain bottom-up, excluding the target, with the lower's open PR as base`, async () => {
    h.selectQueue.push([issueRow(`EXP-12`)]) // target
    h.selectQueue.push([REPO]) // target board repo
    h.selectQueue.push([
      { issueId: `issue-EXP-11`, relatedIssueId: `issue-EXP-12` },
    ])
    h.selectQueue.push([
      issueRow(`EXP-11`, {
        prUrl: `https://github.com/owner/repo/pull/241`,
        prNumber: 241,
        prState: `open`,
      }),
    ])
    h.selectQueue.push([]) // no blockers of EXP-11
    h.selectQueue.push([REPO]) // EXP-11's board repo

    const plan = await resolveStackChain(db, `issue-EXP-12`)
    expect(plan.chain.map((link) => link.identifier)).toEqual([`EXP-11`])
    expect(plan.lower?.identifier).toBe(`EXP-11`)
    expect(plan.base).toBe(`exp/EXP-11`)
    expect(plan.repoFullName).toBe(`owner/repo`)
  })

  it(`falls back to the board's default branch when the foundation has no open PR`, async () => {
    h.selectQueue.push([issueRow(`EXP-12`)])
    h.selectQueue.push([REPO])
    h.selectQueue.push([
      { issueId: `issue-EXP-11`, relatedIssueId: `issue-EXP-12` },
    ])
    h.selectQueue.push([issueRow(`EXP-11`)])
    h.selectQueue.push([])
    h.selectQueue.push([REPO])

    const plan = await resolveStackChain(db, `issue-EXP-12`)
    expect(plan.base).toBe(`master`)
    expect(plan.lower?.prState).toBeNull()
  })

  it(`writes the missing blocks relation for an explicit stackOnIssueId, once`, async () => {
    h.selectQueue.push([issueRow(`EXP-12`)])
    h.selectQueue.push([
      issueRow(`EXP-11`, {
        prUrl: `https://github.com/owner/repo/pull/241`,
        prNumber: 241,
        prState: `open`,
      }),
    ]) // the pick, same team
    h.selectQueue.push([REPO])
    // The pick rides the walk as an in-memory edge: the relation table holds
    // nothing yet, and the pick's row is never re-read.
    h.selectQueue.push([]) // blockers of [EXP-12, EXP-11]
    h.selectQueue.push([REPO]) // EXP-11's board repo
    h.selectQueue.push([]) // findRelationCycle: nothing reachable from EXP-12

    const plan = await resolveStackChain(db, `issue-EXP-12`, {
      stackOnIssueId: `issue-EXP-11`,
      actorUserId: `actor`,
    })
    expect(plan.chain.map((link) => link.identifier)).toEqual([`EXP-11`])
    expect(plan.base).toBe(`exp/EXP-11`)
    expect(h.insertRelationInTx).toHaveBeenCalledTimes(1)
    expect(h.insertRelationInTx).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        issueId: `issue-EXP-11`,
        relatedIssueId: `issue-EXP-12`,
        type: `blocks`,
        source: `user`,
        teamId: `ws-1`,
      })
    )
  })

  it(`refuses a stackOnIssueId that would close a blocking cycle, writing nothing`, async () => {
    // EXP-12 already blocks EXP-11; picking EXP-11 as the foundation of
    // EXP-12 would make each block the other. The candidate edge is judged
    // in memory, so the graph never receives it.
    h.selectQueue.push([issueRow(`EXP-12`)])
    h.selectQueue.push([issueRow(`EXP-11`)]) // the pick
    h.selectQueue.push([REPO])
    h.selectQueue.push([
      { issueId: `issue-EXP-12`, relatedIssueId: `issue-EXP-11` },
    ]) // blockers of [EXP-12, EXP-11]

    await expect(
      resolveStackChain(db, `issue-EXP-12`, {
        stackOnIssueId: `issue-EXP-11`,
        actorUserId: `actor`,
      })
    ).rejects.toThrow(StackCycleError)
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
  })

  it(`refuses a stackOnIssueId whose cycle runs through a CLOSED issue, writing nothing`, async () => {
    // EXP-12 blocks EXP-11 (done), EXP-11 blocks EXP-10. The plan walk drops
    // the settled EXP-11 with its blockers, so it sees no cycle — yet picking
    // EXP-10 as EXP-12's foundation writes `EXP-10 blocks EXP-12` and closes
    // EXP-10 → EXP-12 → EXP-11 → EXP-10 in the relation graph itself.
    h.selectQueue.push([issueRow(`EXP-12`)]) // target
    h.selectQueue.push([issueRow(`EXP-10`)]) // the pick
    h.selectQueue.push([REPO])
    h.selectQueue.push([
      { issueId: `issue-EXP-11`, relatedIssueId: `issue-EXP-10` },
    ]) // blockers of [EXP-12, EXP-10]
    h.selectQueue.push([issueRow(`EXP-11`, { status: `done` })])
    h.selectQueue.push([
      { issueId: `issue-EXP-12`, relatedIssueId: `issue-EXP-11` },
    ]) // blockers of [EXP-11]
    h.selectQueue.push([REPO]) // EXP-10's board repo
    // findRelationCycle walks forward from EXP-12 over the REAL rows.
    h.selectQueue.push([{ from: `issue-EXP-12`, to: `issue-EXP-11` }])
    h.selectQueue.push([{ from: `issue-EXP-11`, to: `issue-EXP-10` }])

    await expect(
      resolveStackChain(db, `issue-EXP-12`, {
        stackOnIssueId: `issue-EXP-10`,
        actorUserId: `actor`,
      })
    ).rejects.toThrow(
      `Blocking cycle: EXP-10 → EXP-12 → EXP-11 → EXP-10. Fix the relations before stacking.`
    )
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
  })

  it(`refuses a stackOnIssueId on another repository, writing nothing`, async () => {
    h.selectQueue.push([issueRow(`EXP-12`)])
    h.selectQueue.push([issueRow(`EXP-11`, { boardId: `board-2` })]) // the pick
    h.selectQueue.push([REPO])
    h.selectQueue.push([]) // blockers of [EXP-12, EXP-11]
    h.selectQueue.push([{ ...REPO, repositoryId: `repo-2`, fullName: `o/other` }])

    await expect(
      resolveStackChain(db, `issue-EXP-12`, {
        stackOnIssueId: `issue-EXP-11`,
        actorUserId: `actor`,
      })
    ).rejects.toThrow(
      /^EXP-12 is blocked by EXP-11, which is on another repository/
    )
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
  })

  it(`refuses an explicit stackOnIssueId from another team before writing anything`, async () => {
    h.selectQueue.push([issueRow(`EXP-12`)])
    h.selectQueue.push([issueRow(`OTHER-1`, { teamId: `ws-2`, boardId: `board-9` })])

    await expect(
      resolveStackChain(db, `issue-EXP-12`, {
        stackOnIssueId: `issue-OTHER-1`,
        actorUserId: `actor`,
      })
    ).rejects.toThrow(`EXP-12 cannot stack on an issue in another team`)
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
  })

  it(`refuses a blocker on another repository`, async () => {
    h.selectQueue.push([issueRow(`EXP-12`)])
    h.selectQueue.push([REPO])
    h.selectQueue.push([
      { issueId: `issue-EXP-11`, relatedIssueId: `issue-EXP-12` },
    ])
    h.selectQueue.push([issueRow(`EXP-11`, { boardId: `board-2` })])
    h.selectQueue.push([])
    h.selectQueue.push([{ ...REPO, repositoryId: `repo-2`, fullName: `o/other` }])

    await expect(
      resolveStackChain(db, `issue-EXP-12`)
    ).rejects.toThrow(
      `EXP-12 is blocked by EXP-11, which is on another repository — start it without stacking.`
    )
  })

  it(`drops a finished blocker instead of stacking on it`, async () => {
    h.selectQueue.push([issueRow(`EXP-12`)])
    h.selectQueue.push([REPO])
    h.selectQueue.push([
      { issueId: `issue-EXP-11`, relatedIssueId: `issue-EXP-12` },
    ])
    h.selectQueue.push([issueRow(`EXP-11`, { status: `done` })])
    h.selectQueue.push([])

    const plan = await resolveStackChain(db, `issue-EXP-12`)
    expect(plan.chain).toEqual([])
    expect(plan.lower).toBeNull()
    expect(plan.base).toBe(`master`)
  })

  it(`refuses an issue whose board has no repository`, async () => {
    h.selectQueue.push([issueRow(`EXP-12`)])
    h.selectQueue.push([])
    await expect(resolveStackChain(db, `issue-EXP-12`)).rejects.toThrow(
      `No repository linked to this board. Link one in team settings.`
    )
  })
})
