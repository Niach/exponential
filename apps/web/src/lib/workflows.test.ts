import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  foldCompoundNodes,
  loadWorkflowEdges,
  nodeEdges,
  isWorkflowReviewBranch,
  proposeNodesForRelation,
  unlinkedRootsToPropose,
  workflowIntegrationBranch,
  workflowReviewBranch,
} from "@/lib/workflows"

// EXP-981 — the pure half of the workflow graph: how picked issues fold into
// nodes and how `blocks` relations become edges between them.
const parents = (...pairs: Array<[child: string, parent: string]>) => new Map(pairs)
const open = (...ids: string[]) => new Set(ids)

describe(`foldCompoundNodes`, () => {
  it(`keeps unrelated picks as plain nodes`, () => {
    expect(foldCompoundNodes([`a`, `b`], parents(), open())).toEqual([
      { issueId: `a`, memberIssueIds: [] },
      { issueId: `b`, memberIssueIds: [] },
    ])
  })

  it(`folds a picked sub-issue into its picked parent`, () => {
    expect(
      foldCompoundNodes([`k`, `p`], parents([`k`, `p`]), open(`k`, `p`))
    ).toEqual([{ issueId: `p`, memberIssueIds: [`k`] }])
  })

  it(`folds into the TOP picked ancestor through an unpicked middle one`, () => {
    expect(
      foldCompoundNodes(
        [`g`, `k`],
        parents([`k`, `m`], [`m`, `g`]),
        open(`g`, `m`, `k`)
      )
    ).toEqual([{ issueId: `g`, memberIssueIds: [`k`, `m`] }])
  })

  it(`adopts open sub-issues nobody picked, never finished ones`, () => {
    expect(
      foldCompoundNodes(
        [`p`],
        parents([`new`, `p`], [`done`, `p`]),
        open(`p`, `new`)
      )
    ).toEqual([{ issueId: `p`, memberIssueIds: [`new`] }])
  })

  it(`leaves a sub-issue whose parent is not in the workflow a plain node`, () => {
    expect(foldCompoundNodes([`k`], parents([`k`, `elsewhere`]), open(`k`))).toEqual([
      { issueId: `k`, memberIssueIds: [] },
    ])
  })

  it(`survives a parent cycle`, () => {
    const folded = foldCompoundNodes(
      [`a`, `b`],
      parents([`a`, `b`], [`b`, `a`]),
      open(`a`, `b`)
    )
    expect(folded.flatMap((n) => [n.issueId, ...n.memberIssueIds]).sort()).toEqual([
      `a`,
      `b`,
    ])
  })
})

describe(`nodeEdges`, () => {
  const nodes = [
    { id: `n1`, issueId: `p`, memberIssueIds: [`k1`, `k2`] },
    { id: `n2`, issueId: `x`, memberIssueIds: [] },
  ]

  it(`lifts a relation between members to an edge between their nodes`, () => {
    expect(nodeEdges(nodes, [{ issueId: `k2`, relatedIssueId: `x` }])).toEqual([
      [`n1`, `n2`],
    ])
  })

  it(`drops relations inside one node and ones leaving the workflow`, () => {
    expect(
      nodeEdges(nodes, [
        { issueId: `k1`, relatedIssueId: `k2` },
        { issueId: `x`, relatedIssueId: `outside` },
      ])
    ).toEqual([])
  })
})

describe(`workflowIntegrationBranch`, () => {
  it(`is exp/wf-<id8>`, () => {
    expect(workflowIntegrationBranch(`8cef8d22-dafc-4fb7-8e4f-01483ab0b5d0`)).toBe(
      `exp/wf-8cef8d22`
    )
  })
})

// FEED-51: the review gate's branch evidence mirrors `crates/coding`
// `review_branch` + `live_reviews_on_branches`.
describe(`workflowReviewBranch`, () => {
  const WF = `abcdef12-3456-7890-abcd-ef1234567890`
  it(`names the workflow, the issue and the round`, () => {
    expect(workflowReviewBranch(WF, `EXP-42`, 2)).toBe(`exp/wf-abcdef12-review-EXP-42-r2`)
  })
  it.each([
    [`the same node, any round`, `exp/wf-abcdef12-review-EXP-10-r3`, true],
    [`EXP-103's branch against EXP-10`, `exp/wf-abcdef12-review-EXP-103-r1`, false],
    [`another workflow's branch`, `exp/wf-00000000-review-EXP-10-r1`, false],
    [`a prefix with no round`, `exp/wf-abcdef12-review-EXP-10-r`, false],
    [`the node's own PR branch`, `exp/EXP-10`, false],
  ])(`matches %s: %s → %s`, (_name, branch, expected) => {
    expect(isWorkflowReviewBranch(WF, `EXP-10`, branch)).toBe(expected)
  })
})

// EXP-984: a `proposed` node was never admitted. The engine drops it; so must
// the server's graph, or a covered node blocked by a proposed outsider never
// lands (`landNode` waits on it, `retargetReleasedDependents` never releases).
describe(`loadWorkflowEdges`, () => {
  function fakeExecutor(results: unknown[][]) {
    const queue = [...results]
    const chain = (rows: unknown[]) => {
      const p = Promise.resolve(rows) as Promise<unknown[]> & Record<string, () => unknown>
      for (const m of [`from`, `where`, `innerJoin`]) p[m] = () => p
      return p
    }
    return { select: () => chain(queue.shift() ?? []) } as never
  }

  it(`leaves proposed nodes and their edges out`, async () => {
    const executor = fakeExecutor([
      [
        { id: `n1`, issueId: `a`, memberIssueIds: [], state: `blocked`, baseBranch: null },
        { id: `n2`, issueId: `b`, memberIssueIds: [], state: `proposed`, baseBranch: null },
        { id: `n3`, issueId: `c`, memberIssueIds: [`c1`], state: `landed`, baseBranch: null },
      ],
      [
        { issueId: `b`, relatedIssueId: `a` },
        { issueId: `c1`, relatedIssueId: `a` },
      ],
    ])
    const graph = await loadWorkflowEdges(executor, `wf-1`)
    expect(graph.nodes.map((node) => node.id)).toEqual([`n1`, `n3`])
    expect(graph.edges).toEqual([[`n3`, `n1`]])
  })
})

// FEED-57 (a): unlinking a node's last in-workflow blocker must not turn an
// unstarted node into a root the engine starts at once.
describe(`unlinkedRootsToPropose`, () => {
  const node = (id: string, over: Partial<{ state: string; attempt: number; sessionId: string | null }> = {}) => ({
    id,
    state: `blocked`,
    attempt: 0,
    sessionId: null,
    ...over,
  })

  it(`proposes an unstarted node that lost its last blocker`, () => {
    expect(
      unlinkedRootsToPropose([{ id: `b`, wave: 1 }], [node(`a`, { state: `running` }), node(`b`)], [])
    ).toEqual([`b`])
  })

  it(`keeps a node that still has an admitted blocker`, () => {
    expect(
      unlinkedRootsToPropose([{ id: `c`, wave: 2 }], [node(`a`), node(`c`)], [[`a`, `c`]])
    ).toEqual([])
  })

  it(`keeps a node that was a root already, or that started`, () => {
    expect(
      unlinkedRootsToPropose(
        [
          { id: `a`, wave: 0 },
          { id: `b`, wave: 1 },
          { id: `c`, wave: 1 },
          { id: `d`, wave: 1 },
        ],
        [
          node(`a`),
          node(`b`, { state: `running`, sessionId: `s-1` }),
          node(`c`, { attempt: 1 }),
          node(`d`, { state: `ready`, sessionId: `s-2` }),
        ],
        []
      )
    ).toEqual([])
  })

  it(`treats a proposal as no blocker and cascades down an unstarted chain`, () => {
    expect(
      unlinkedRootsToPropose(
        [{ id: `b`, wave: 1 }],
        [node(`p`, { state: `proposed` }), node(`b`), node(`c`), node(`d`), node(`x`, { state: `landed` })],
        [
          [`p`, `b`],
          [`b`, `c`],
          [`c`, `d`],
          [`x`, `d`],
        ]
      )
    ).toEqual([`b`, `c`])
  })
})

// A drizzle-ish fake: each awaited select pops the next result; inserts are
// recorded.
function fakeTx(results: unknown[][]) {
  const queue = [...results]
  const inserted: unknown[] = []
  const chain = (rows: unknown[]) => {
    const p = Promise.resolve(rows) as Promise<unknown[]> & Record<string, () => unknown>
    for (const m of [`from`, `where`, `innerJoin`, `limit`]) p[m] = () => p
    return p
  }
  const tx = {
    select: () => chain(queue.shift() ?? []),
    insert: () => ({
      values: (value: unknown) => {
        inserted.push(value)
        return { onConflictDoNothing: () => Promise.resolve() }
      },
    }),
  }
  return { tx: tx as never, inserted }
}

describe(`proposeNodesForRelation`, () => {
  const workflow = {
    workflowId: `wf-1`,
    teamId: `team-1`,
    repositoryId: `repo-1`,
    startedAt: new Date(`2026-09-01T00:00:00Z`),
  }
  const followUp = [{ status: `backlog`, createdAt: new Date(`2026-09-02T00:00:00Z`), repositoryId: `repo-1` }]

  it(`admits a follow-up downstream of an admitted node`, async () => {
    const { tx, inserted } = fakeTx([
      [{ ...workflow, issueId: `a`, members: [], state: `running` }],
      [{ issueId: `a`, members: [] }],
      followUp,
      [],
    ])
    await proposeNodesForRelation(tx, { issueId: `a`, relatedIssueId: `new` })
    expect(inserted).toMatchObject([{ issueId: `new`, state: `blocked`, note: null }])
  })

  // FEED-57 (b)
  it(`proposes a follow-up whose only blocker is itself a proposal`, async () => {
    const { tx, inserted } = fakeTx([
      [{ ...workflow, issueId: `p`, members: [], state: `proposed` }],
      [{ issueId: `p`, members: [] }],
      followUp,
      [],
    ])
    await proposeNodesForRelation(tx, { issueId: `p`, relatedIssueId: `new` })
    expect(inserted).toMatchObject([{ issueId: `new`, state: `proposed` }])
  })
})
