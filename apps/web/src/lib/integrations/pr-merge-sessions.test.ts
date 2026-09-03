import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-498: merge always closes. Two writers share the semantics — the in-tx
// sweep `endLiveIssueSessionsInTx` (run by applyPrMergeState's claim winner)
// and the standalone idempotent sweep `endMergedPrSessions` (mergePr's
// backstop for claim races). A structural fake `tx`
// (recording set values + where clause) plus mocked relay helpers is enough —
// the where clause is asserted by SHAPE, since a fake db cannot execute it
// (the coding-session-kill.test.ts pattern).
const h = vi.hoisted(() => ({
  updates: [] as { set: Record<string, unknown>; where: unknown }[],
  returning: [] as { id: string }[],
  // What the ONE db-level select of each sweep resolves to: the repo→teams
  // lookup endSessionsOnMergedBranch does first (EXP-637), or the issues
  // whose team still ends sessions on merge (EXP-711) for
  // endMergedPrSessions.
  selectRows: [] as Record<string, string>[],
  selectWheres: [] as unknown[],
  getSteerRelayConfig: vi.fn(),
  relayPostKill: vi.fn(async () => ({ delivered: true })),
  // EXP-700: the merge paths must also tell a live parent that its
  // agent-started child ended without a report.
  notifyParentOfChildEnd: vi.fn(async () => ({ delivered: false })),
}))

function fakeTx() {
  return {
    update: () => ({
      set: (set: Record<string, unknown>) => ({
        where: (where: unknown) => ({
          returning: async () => {
            h.updates.push({ set, where })
            return h.returning
          },
        }),
      }),
    }),
  }
}

vi.mock(`@/db/connection`, () => ({
  db: {
    transaction: async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeTx()),
    select: () => {
      const chain = {
        innerJoin: () => chain,
        where: (where: unknown) => {
          h.selectWheres.push(where)
          return Promise.resolve(h.selectRows)
        },
      }
      return { from: () => chain }
    },
  },
}))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: async () => `1` }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayPostKill: h.relayPostKill,
}))
vi.mock(`@/lib/steer-child-messages`, () => ({
  notifyParentOfChildEnd: h.notifyParentOfChildEnd,
}))

import {
  endLiveIssueSessionsInTx,
  endMergedPrSessions,
  endSessionsOnMergedBranch,
} from "@/lib/integrations/pr-sync"

const ISSUE = `11111111-1111-4111-8111-111111111111`
const ISSUE_2 = `22222222-2222-4222-8222-222222222222`

// Flatten a drizzle condition into its column names + bound values, in order:
// eq(a.b, `x`) ⇒ [`col:b`, `x`].
function whereShape(cond: unknown, out: unknown[] = []): unknown[] {
  if (!cond || typeof cond !== `object`) return out
  if (Array.isArray(cond)) {
    for (const child of cond) whereShape(child, out)
    return out
  }
  const rec = cond as Record<string, unknown>
  if (Array.isArray(rec.queryChunks)) return whereShape(rec.queryChunks, out)
  if (`value` in rec && `encoder` in rec) {
    out.push(rec.value)
    return out
  }
  if (typeof rec.name === `string` && rec.table) {
    out.push(`col:${rec.name}`)
    return out
  }
  return out
}

beforeEach(() => {
  h.updates.length = 0
  h.returning = []
  h.selectRows = []
  h.selectWheres.length = 0
  vi.clearAllMocks()
  h.getSteerRelayConfig.mockReturnValue({ url: `ws://relay`, secret: `s` })
  h.relayPostKill.mockResolvedValue({ delivered: true })
  h.notifyParentOfChildEnd.mockResolvedValue({ delivered: false })
})

describe(`endLiveIssueSessionsInTx`, () => {
  it(`flips every live status to ended and returns the ids`, async () => {
    h.returning = [{ id: `sess-1` }, { id: `sess-2` }]

    const ids = await endLiveIssueSessionsInTx(
      fakeTx() as never,
      ISSUE
    )

    expect(ids).toEqual([`sess-1`, `sess-2`])
    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set).toMatchObject({
      status: `ended`,
      endedAt: expect.any(Date),
      endedBy: `merge`,
      updatedAt: expect.any(Date),
    })
    // EXP-637: the sweep spares the session that merged its own PR.
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:issue_id`,
      ISSUE,
      `col:status`,
      `running`,
      `in_review`,
      `col:merged_own_pr`,
      false,
    ])
  })
})

describe(`endMergedPrSessions`, () => {
  it(`ends every live session across the linked issues and relays one kill each`, async () => {
    h.selectRows = [{ id: ISSUE }, { id: ISSUE_2 }]
    h.returning = [{ id: `sess-1` }, { id: `sess-2` }]

    await endMergedPrSessions([ISSUE, ISSUE_2])

    // EXP-711: with no override, only issues whose team still ends sessions
    // on merge are swept.
    expect(h.selectWheres).toHaveLength(1)
    expect(whereShape(h.selectWheres[0])).toEqual([
      `col:id`,
      ISSUE,
      ISSUE_2,
      `col:end_sessions_on_merge`,
      true,
    ])
    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set).toMatchObject({
      status: `ended`,
      endedAt: expect.any(Date),
      endedBy: `merge`,
      updatedAt: expect.any(Date),
    })
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:issue_id`,
      ISSUE,
      ISSUE_2,
      `col:status`,
      `running`,
      `in_review`,
      `col:merged_own_pr`,
      false,
    ])
    expect(h.relayPostKill).toHaveBeenCalledTimes(2)
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-1`)
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-2`)
    // EXP-700: each ended run is reported to its parent (the helper no-ops
    // for the ones that have none).
    expect(h.notifyParentOfChildEnd).toHaveBeenCalledTimes(2)
    expect(h.notifyParentOfChildEnd).toHaveBeenCalledWith(
      expect.anything(),
      `sess-1`,
      { summary: null, endedBy: `merge` }
    )
    expect(h.notifyParentOfChildEnd).toHaveBeenCalledWith(
      expect.anything(),
      `sess-2`,
      { summary: null, endedBy: `merge` }
    )
  })

  it(`relays nothing when no row matched (idempotent re-run)`, async () => {
    h.selectRows = [{ id: ISSUE }]
    await endMergedPrSessions([ISSUE])

    expect(h.updates).toHaveLength(1)
    expect(h.relayPostKill).not.toHaveBeenCalled()
    expect(h.notifyParentOfChildEnd).not.toHaveBeenCalled()
  })

  // EXP-711: the team switched merge-ends-sessions off.
  it(`sweeps nothing when every issue's team keeps sessions on merge`, async () => {
    h.selectRows = []
    await endMergedPrSessions([ISSUE, ISSUE_2])

    expect(h.selectWheres).toHaveLength(1)
    expect(h.updates).toHaveLength(0)
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })

  it(`endSessions=false skips the sweep without even reading the team`, async () => {
    await endMergedPrSessions([ISSUE], false)

    expect(h.selectWheres).toHaveLength(0)
    expect(h.updates).toHaveLength(0)
  })

  it(`endSessions=true sweeps every issue regardless of the team setting`, async () => {
    h.returning = [{ id: `sess-1` }]
    await endMergedPrSessions([ISSUE, ISSUE_2], true)

    expect(h.selectWheres).toHaveLength(0)
    expect(h.updates).toHaveLength(1)
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:issue_id`,
      ISSUE,
      ISSUE_2,
      `col:status`,
      `running`,
      `in_review`,
      `col:merged_own_pr`,
      false,
    ])
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-1`)
  })

  it(`is a no-op for an empty issue list`, async () => {
    await endMergedPrSessions([])

    expect(h.updates).toHaveLength(0)
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })
})

// EXP-637/EXP-626: an issue-LESS chore PR (opened with
// `exponential_pr_open({ repositoryId, head })`) resolves to no issue at all,
// so the branch recorded on the coding_sessions row is the only handle on the
// run that opened it.
describe(`endSessionsOnMergedBranch`, () => {
  const BRANCH = `exp/refresh-screenshots-1a2b3c4d`

  it(`ends issue-less rows on the merged branch in the repo's teams`, async () => {
    h.selectRows = [{ teamId: `team-1` }, { teamId: `team-2` }, { teamId: `team-1` }]
    h.returning = [{ id: `sess-1` }]

    await endSessionsOnMergedBranch(`org/repo`, BRANCH)

    // EXP-711: the repo→teams lookup drops teams that keep sessions on merge.
    expect(whereShape(h.selectWheres[0])).toEqual([
      `col:full_name`,
      `org/repo`,
      `col:end_sessions_on_merge`,
      true,
    ])
    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set).toMatchObject({
      status: `ended`,
      endedAt: expect.any(Date),
      endedBy: `merge`,
      updatedAt: expect.any(Date),
    })
    // Duplicate team ids collapse; the spare and the issue-less shape are
    // both part of the clause.
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:issue_id`,
      `col:branch`,
      BRANCH,
      `col:team_id`,
      `team-1`,
      `team-2`,
      `col:status`,
      `running`,
      `in_review`,
      `col:merged_own_pr`,
      false,
    ])
    expect(h.relayPostKill).toHaveBeenCalledTimes(1)
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-1`)
    // EXP-700: a chore-PR run can be an agent-started child too.
    expect(h.notifyParentOfChildEnd).toHaveBeenCalledWith(
      expect.anything(),
      `sess-1`,
      { summary: null, endedBy: `merge` }
    )
  })

  it(`is a no-op when nothing registered the repo, or the inputs are blank`, async () => {
    h.selectRows = []
    await endSessionsOnMergedBranch(`org/repo`, BRANCH)
    expect(h.updates).toHaveLength(0)

    await endSessionsOnMergedBranch(``, BRANCH)
    await endSessionsOnMergedBranch(`org/repo`, ``)
    expect(h.selectWheres).toHaveLength(1)
    expect(h.updates).toHaveLength(0)
  })

  // EXP-711: the merge claim's per-call override.
  it(`honours the endSessions override either way`, async () => {
    await endSessionsOnMergedBranch(`org/repo`, BRANCH, false)
    expect(h.selectWheres).toHaveLength(0)
    expect(h.updates).toHaveLength(0)

    h.selectRows = [{ teamId: `team-1` }]
    await endSessionsOnMergedBranch(`org/repo`, BRANCH, true)
    expect(whereShape(h.selectWheres[0])).toEqual([`col:full_name`, `org/repo`])
    expect(h.updates).toHaveLength(1)
  })
})
