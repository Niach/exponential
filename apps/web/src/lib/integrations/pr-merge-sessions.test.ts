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
  returning: [] as { id: string; branch?: string | null }[],
  // SLOP-3: per-call select results, consumed before `selectRows`.
  selectQueue: [] as Record<string, string>[][],
  githubAppConfigured: vi.fn(() => false),
  // What the ONE db-level select of a sweep resolves to: the issues whose
  // team still ends sessions on merge (EXP-711) for endMergedPrSessions.
  selectRows: [] as Record<string, string>[],
  selectWheres: [] as unknown[],
  getSteerRelayConfig: vi.fn(),
  relayPostKill: vi.fn(async () => ({ delivered: true })),
  // EXP-700: the merge paths must also tell a live parent that its
  // agent-started child ended without a report.
  notifyParentOfChildEnd: vi.fn(async () => ({ delivered: false })),
}))

function selectResult(where: unknown) {
  h.selectWheres.push(where)
  const rows = h.selectQueue.shift() ?? h.selectRows
  return Object.assign(Promise.resolve(rows), {
    limit: async () => rows,
  })
}

function fakeTx() {
  return {
    // EXP-734: applySessionPrState reads the live rows to end INSIDE its
    // transaction — same recorder as the db-level select.
    select: () => {
      const chain = {
        innerJoin: () => chain,
        where: selectResult,
      }
      return { from: () => chain }
    },
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
// SLOP-3: the first step of `retargetChildrenOfMergedPr` — a spy that bails,
// so a call proves the retarget fired without reaching GitHub.
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: h.githubAppConfigured,
  resolveRepoDefaultBranchCached: vi.fn(),
  resolveRepoInstallationTokenInfo: vi.fn(),
}))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayPostKill: h.relayPostKill,
}))
vi.mock(`@/lib/steer-child-messages`, () => ({
  notifyParentOfChildEnd: h.notifyParentOfChildEnd,
}))

import {
  applySessionPrState,
  endLiveIssueSessionsInTx,
  endMergedPrSessions,
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
  h.selectQueue = []
  h.selectWheres.length = 0
  vi.clearAllMocks()
  h.githubAppConfigured.mockReturnValue(false)
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
      // An ended run asks nobody anything.
      pendingQuestion: null,
    })
    // EXP-637: the sweep spares the session that merged its own PR. EXP-888:
    // a stale-swept row is a target like any live one — the sweep's end is
    // not one the desktop acts on, a merge end is.
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:issue_id`,
      ISSUE,
      `col:status`,
      `running`,
      `in_review`,
      `col:ended_by`,
      `stale`,
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
      // An ended run asks nobody anything.
      pendingQuestion: null,
    })
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:issue_id`,
      ISSUE,
      ISSUE_2,
      `col:status`,
      `running`,
      `in_review`,
      `col:ended_by`,
      `stale`,
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
      `col:ended_by`,
      `stale`,
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

// EXP-734: a run's own PR lives on its session row. The writer flips
// `pr_state` along the PR lifecycle and, on a merge, ends the live rows on
// that PR the way every other merge path does.
describe(`applySessionPrState`, () => {
  const PR_URL = `https://github.com/org/repo/pull/12`

  it(`flips the row to merged, ends the live runs on the PR and relays one kill each`, async () => {
    h.selectRows = [{ id: `sess-1` }]
    h.returning = [{ id: `sess-1` }]

    const result = await applySessionPrState({ prUrl: PR_URL, state: `merged` })

    expect(result).toEqual({ endedSessionIds: [`sess-1`] })
    expect(h.updates).toHaveLength(2)
    // The state flip addresses every run row on the url (every run owns the
    // PR it opened) and never re-applies a terminal merge.
    expect(h.updates[0]!.set).toMatchObject({ prState: `merged` })
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:pr_url`,
      PR_URL,
      `col:pr_state`,
      `col:pr_state`,
      `merged`,
    ])
    // The end candidates: live, not the self-merge spare, team still ends
    // sessions on merge (no override).
    expect(whereShape(h.selectWheres[0])).toEqual([
      `col:pr_url`,
      PR_URL,
      `col:status`,
      `running`,
      `in_review`,
      `col:ended_by`,
      `stale`,
      `col:merged_own_pr`,
      false,
      `col:end_sessions_on_merge`,
      true,
    ])
    expect(h.updates[1]!.set).toMatchObject({
      status: `ended`,
      endedAt: expect.any(Date),
      endedBy: `merge`,
      updatedAt: expect.any(Date),
    })
    expect(whereShape(h.updates[1]!.where)).toEqual([
      `col:id`,
      `sess-1`,
      `col:status`,
      `running`,
      `in_review`,
      `col:ended_by`,
      `stale`,
    ])
    expect(h.relayPostKill).toHaveBeenCalledTimes(1)
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-1`)
    expect(h.notifyParentOfChildEnd).toHaveBeenCalledWith(
      expect.anything(),
      `sess-1`,
      { summary: null, endedBy: `merge` }
    )
  })

  it(`ends nothing when no live row sits on the PR (idempotent re-run)`, async () => {
    h.selectRows = []
    const result = await applySessionPrState({ prUrl: PR_URL, state: `merged` })
    expect(result).toEqual({ endedSessionIds: [] })
    expect(h.updates).toHaveLength(1)
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })

  // EXP-711: the merger's per-call override.
  it(`honours the endSessions override either way`, async () => {
    await applySessionPrState({ prUrl: PR_URL, state: `merged`, endSessions: false })
    expect(h.updates).toHaveLength(1)
    expect(h.selectWheres).toHaveLength(0)

    h.selectRows = [{ id: `sess-1` }]
    h.returning = [{ id: `sess-1` }]
    await applySessionPrState({ prUrl: PR_URL, state: `merged`, endSessions: true })
    expect(whereShape(h.selectWheres[0])).toEqual([
      `col:pr_url`,
      PR_URL,
      `col:status`,
      `running`,
      `in_review`,
      `col:ended_by`,
      `stale`,
      `col:merged_own_pr`,
      false,
    ])
    expect(h.updates).toHaveLength(3)
  })

  it(`closed and open flip the state only, along the open⇄closed lifecycle`, async () => {
    await applySessionPrState({ prUrl: PR_URL, state: `closed` })
    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set).toMatchObject({ prState: `closed` })
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:pr_url`,
      PR_URL,
      `col:pr_state`,
      `open`,
    ])

    await applySessionPrState({ prUrl: PR_URL, state: `open` })
    expect(h.updates).toHaveLength(2)
    expect(whereShape(h.updates[1]!.where)).toEqual([
      `col:pr_url`,
      PR_URL,
      `col:pr_state`,
      `closed`,
    ])
    expect(h.selectWheres).toHaveLength(0)
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })

  // SLOP-3: an issue-less PR has no applyPrMergeState to retarget its
  // children, so the run row's merge flip does — once.
  it(`retargets an issue-less PR's children from the row that flipped`, async () => {
    h.returning = [{ id: `sess-1`, branch: `exp/chat-1a2b3c4d` }]
    h.selectQueue = [[]] // no issue carries the PR
    await applySessionPrState({ prUrl: PR_URL, state: `merged`, endSessions: false })
    expect(whereShape(h.selectWheres[0])).toEqual([`col:pr_url`, PR_URL])
    expect(h.githubAppConfigured).toHaveBeenCalledTimes(1)
  })

  it(`leaves the retarget to applyPrMergeState when an issue carries the PR`, async () => {
    h.returning = [{ id: `sess-1`, branch: `exp/APP-1` }]
    h.selectQueue = [[{ id: ISSUE }]]
    await applySessionPrState({ prUrl: PR_URL, state: `merged`, endSessions: false })
    expect(h.githubAppConfigured).not.toHaveBeenCalled()
  })

  it(`never retargets when nothing flipped (a racing duplicate) or not merged`, async () => {
    h.returning = []
    await applySessionPrState({ prUrl: PR_URL, state: `merged`, endSessions: false })
    h.returning = [{ id: `sess-1`, branch: `exp/chat-1a2b3c4d` }]
    await applySessionPrState({ prUrl: PR_URL, state: `closed` })
    expect(h.selectWheres).toHaveLength(0)
    expect(h.githubAppConfigured).not.toHaveBeenCalled()
  })

  it(`is a no-op for a blank url`, async () => {
    await applySessionPrState({ prUrl: ``, state: `merged` })
    expect(h.updates).toHaveLength(0)
  })
})
