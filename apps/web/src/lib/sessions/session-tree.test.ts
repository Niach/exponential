import { describe, expect, it } from "vitest"
import marks from "@exp/domain-contract/fixtures/session-tree-marks.json"
import {
  sessionNeedsYou,
  workflowRunAccountCaption,
  type SessionMarkDevice,
  type SessionMarkRow,
  flattenSessionTree,
  reviewBranchRound,
  reviewRoundVerdict,
  reviewRowCaption,
  sessionTree,
  sessionTreeNodeKey,
  visibleSessionTreeRows,
  workflowGroupCaption,
  type SessionNode,
  type SessionTreeNode,
  type SessionTreeRow,
  type WorkflowGroupNode,
} from "./session-tree"

// EXP-1029 contract — the session tree (`lib/sessions/session-tree.ts`).
// EXP-996 implements `sessionTree` and un-skips this table; its Rust, Swift
// and Kotlin mirrors carry the same case names.

const row = (
  id: string,
  over: Partial<SessionTreeRow> = {}
): SessionTreeRow => ({
  id,
  resumedFromId: null,
  parentSessionId: null,
  issueId: null,
  batchIssueIds: null,
  startedReason: null,
  status: `running`,
  createdAt: new Date(`2026-09-01T10:00:00Z`),
  updatedAt: new Date(`2026-09-01T10:00:00Z`),
  ...over,
})

const at = (iso: string) => ({ createdAt: new Date(iso), updatedAt: new Date(iso) })

/** EXP-1082 §1: the server-stamped membership of workflow `w`. */
const member = (nodeId: string | null, role = `author`, workflowId = `w`) => ({
  workflowId,
  workflowNodeId: nodeId,
  workflowRole: role,
})

const ids = (nodes: readonly SessionTreeNode[]): unknown[] =>
  nodes.map((node) =>
    node.kind === `session`
      ? { id: node.session.id, children: ids(node.children) }
      : { [node.kind]: node.kind === `workflow` ? node.workflowId : node.rootIssueId, children: ids(node.children) }
  )

describe(`flattenSessionTree (EXP-1029 contract)`, () => {
  it(`walks groups and children depth-first, sessions only`, () => {
    const leaf: SessionNode = {
      kind: `session`,
      session: row(`b`),
      chain: [row(`b`)],
      children: [],
      lastActivityAt: 0,
      reviewRound: null,
      duplicateLive: false,
    }
    const tree: SessionTreeNode[] = [
      {
        kind: `workflow`,
        workflowId: `w`,
        name: `W`,
        status: `running`,
        liveRuns: 1,
        nodesDone: 0,
        nodesTotal: 0,
        children: [leaf],
        lastActivityAt: 0,
      },
      {
        kind: `session`,
        session: row(`a`),
        chain: [row(`a`)],
        children: [],
        lastActivityAt: 0,
        reviewRound: null,
        duplicateLive: false,
      },
    ]
    expect(flattenSessionTree(tree).map((node) => node.session.id)).toEqual([`b`, `a`])
  })
})

describe(`sessionTree (EXP-1029 → EXP-996)`, () => {
  it(`lists unrelated sessions at top level, newest activity first`, () => {
    const a = row(`a`, at(`2026-09-01T10:00:00Z`))
    const b = row(`b`, at(`2026-09-01T11:00:00Z`))
    expect(ids(sessionTree([a, b]))).toEqual([
      { id: `b`, children: [] },
      { id: `a`, children: [] },
    ])
  })

  it(`nests a child under its parentSessionId`, () => {
    const parent = row(`p`)
    const child = row(`c`, { parentSessionId: `p`, ...at(`2026-09-01T10:30:00Z`) })
    expect(ids(sessionTree([child, parent]))).toEqual([
      { id: `p`, children: [{ id: `c`, children: [] }] },
    ])
  })

  it(`collapses a resume succession into one node keyed by its newest row`, () => {
    const first = row(`r1`, at(`2026-09-01T10:00:00Z`))
    const second = row(`r2`, { resumedFromId: `r1`, ...at(`2026-09-01T12:00:00Z`) })
    const tree = sessionTree([first, second])
    expect(ids(tree)).toEqual([{ id: `r2`, children: [] }])
    expect((tree[0] as SessionNode).chain.map((r) => r.id)).toEqual([`r1`, `r2`])
  })

  it(`follows a child to its parent's resume succession`, () => {
    const p1 = row(`p1`)
    const p2 = row(`p2`, { resumedFromId: `p1`, ...at(`2026-09-01T11:00:00Z`) })
    const child = row(`c`, { parentSessionId: `p1` })
    expect(ids(sessionTree([p1, p2, child]))).toEqual([
      { id: `p2`, children: [{ id: `c`, children: [] }] },
    ])
  })

  it(`groups the sessions of one workflow under a workflow node`, () => {
    // EXP-1068: the rows' own `workflowId` groups them; the `workflow_nodes`
    // rows only feed the caption.
    const n1 = row(`n1`, { issueId: `i1`, startedReason: `workflow`, ...member(`n1`) })
    const n2 = row(`n2`, {
      issueId: `i2`,
      startedReason: `workflow`,
      ...member(`n2`),
      ...at(`2026-09-01T11:00:00Z`),
    })
    const tree = sessionTree([n1, n2], {
      workflows: [{ id: `w`, name: `EXP-996 +5`, status: `running` }],
      workflowNodes: [
        { workflowId: `w`, issueId: `i1` },
        { workflowId: `w`, issueId: `i2` },
      ],
    })
    expect(ids(tree)).toEqual([
      { workflow: `w`, children: [{ id: `n2`, children: [] }, { id: `n1`, children: [] }] },
    ])
  })

  it(`groups a stack under its lowest issue, in linear order`, () => {
    const low = row(`s-low`, { issueId: `i-low` })
    const mid = row(`s-mid`, { issueId: `i-mid`, ...at(`2026-09-01T11:00:00Z`) })
    const top = row(`s-top`, { issueId: `i-top`, ...at(`2026-09-01T12:00:00Z`) })
    const tree = sessionTree([top, low, mid], {
      issues: [
        { id: `i-low`, identifier: `APP-1`, branch: `exp/APP-1`, prBaseBranch: null },
        { id: `i-mid`, identifier: `APP-2`, branch: `exp/APP-2`, prBaseBranch: `exp/APP-1` },
        { id: `i-top`, identifier: `APP-3`, branch: `exp/APP-3`, prBaseBranch: `exp/APP-2` },
      ],
    })
    expect(ids(tree)).toEqual([
      {
        stack: `i-low`,
        children: [
          { id: `s-low`, children: [] },
          { id: `s-mid`, children: [] },
          { id: `s-top`, children: [] },
        ],
      },
    ])
  })

  it(`keeps an issue-less run beside a stack group`, () => {
    const chat = row(`chat`, at(`2026-09-01T09:00:00Z`))
    const low = row(`s-low`, { issueId: `i-low` })
    const top = row(`s-top`, { issueId: `i-top`, ...at(`2026-09-01T11:00:00Z`) })
    expect(
      ids(
        sessionTree([chat, low, top], {
          issues: [
            { id: `i-low`, identifier: `APP-1`, branch: `exp/APP-1`, prBaseBranch: null },
            { id: `i-top`, identifier: `APP-2`, branch: `exp/APP-2`, prBaseBranch: `exp/APP-1` },
          ],
        })
      )
    ).toEqual([
      {
        stack: `i-low`,
        children: [
          { id: `s-low`, children: [] },
          { id: `s-top`, children: [] },
        ],
      },
      { id: `chat`, children: [] },
    ])
  })

  it(`sorts groups by their last activity among the top-level nodes`, () => {
    const lone = row(`lone`, at(`2026-09-01T11:30:00Z`))
    const n1 = row(`n1`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const n2 = row(`n2`, { issueId: `i2`, ...member(`n2`), ...at(`2026-09-01T12:00:00Z`) })
    const tree = sessionTree([lone, n1, n2], {
      workflows: [{ id: `w`, name: `W`, status: `running` }],
      workflowNodes: [
        { workflowId: `w`, issueId: `i1` },
        { workflowId: `w`, issueId: `i2` },
      ],
    })
    expect(tree.map((node) => node.kind)).toEqual([`workflow`, `session`])
    expect(tree[0]!.lastActivityAt).toBe(new Date(`2026-09-01T12:00:00Z`).getTime())
  })

  it(`puts an orphan child whose parent is gone at top level`, () => {
    const orphan = row(`o`, { parentSessionId: `gone` })
    expect(ids(sessionTree([orphan]))).toEqual([{ id: `o`, children: [] }])
  })

  it(`keeps children in creation order under their parent`, () => {
    const parent = row(`p`)
    const c1 = row(`c1`, { parentSessionId: `p`, ...at(`2026-09-01T10:10:00Z`) })
    const c2 = row(`c2`, { parentSessionId: `p`, ...at(`2026-09-01T10:20:00Z`) })
    expect(ids(sessionTree([c2, parent, c1]))).toEqual([
      { id: `p`, children: [{ id: `c1`, children: [] }, { id: `c2`, children: [] }] },
    ])
  })
})

// EXP-996: the flattening every client paints the EXP-965 connector over.
// Mirrored ×4 (desktop `visible_session_tree_rows`, iOS/Android
// `SessionTree.visibleRows`) with these same case names.
describe(`visibleSessionTreeRows (EXP-996)`, () => {
  const workflowTree = () => {
    const parent = row(`p`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T11:00:00Z`) })
    const child = row(`c`, { parentSessionId: `p`, ...at(`2026-09-01T10:30:00Z`) })
    const other = row(`n2`, { issueId: `i2`, ...member(`n2`), ...at(`2026-09-01T10:00:00Z`) })
    return sessionTree([parent, child, other], {
      workflows: [{ id: `w`, name: `W`, status: `running` }],
      workflowNodes: [
        { workflowId: `w`, issueId: `i1` },
        { workflowId: `w`, issueId: `i2` },
      ],
    })
  }

  it(`flattens groups and children with their depths`, () => {
    expect(
      visibleSessionTreeRows(workflowTree()).map((flat) => [flat.key, flat.depth])
    ).toEqual([
      [`workflow:w`, 0],
      [`p`, 1],
      [`c`, 2],
      [`n2`, 1],
    ])
  })

  it(`hides everything under a collapsed node`, () => {
    const rows = visibleSessionTreeRows(workflowTree(), new Set([`p`]))
    expect(rows.map((flat) => flat.key)).toEqual([`workflow:w`, `p`, `n2`])
    expect(rows.find((flat) => flat.key === `p`)!.hasChildren).toBe(true)
  })

  it(`folds a whole group away`, () => {
    expect(
      visibleSessionTreeRows(workflowTree(), new Set([`workflow:w`])).map(
        (flat) => flat.key
      )
    ).toEqual([`workflow:w`])
  })

  it(`keys a session by its id and a group by its kind`, () => {
    const tree = workflowTree()
    expect(sessionTreeNodeKey(tree[0]!)).toBe(`workflow:w`)
    expect(
      sessionTreeNodeKey({
        kind: `stack`,
        rootIssueId: `i-low`,
        children: [],
        lastActivityAt: 0,
      })
    ).toBe(`stack:i-low`)
  })
})

// EXP-1082 §2 → EXP-1068: membership-first grouping. Mirrored ×4 with these
// case names (desktop `workflow_membership`, `SessionTreeTests.swift`,
// `SessionTreeTest.kt`); the shapes are the same rows there.
describe(`sessionTree — workflow membership (EXP-1082 → EXP-1068)`, () => {
  const workflows = [{ id: `w`, name: `Checkout rewrite`, status: `running` }]
  const group = (tree: readonly SessionTreeNode[]): WorkflowGroupNode => {
    const first = tree[0]
    if (first?.kind !== `workflow`) throw new Error(`expected a workflow group first`)
    return first
  }
  const sessionAt = (tree: readonly SessionTreeNode[], path: number[]): SessionNode => {
    let cursor: SessionTreeNode | undefined = tree[path[0]!]
    for (const index of path.slice(1)) cursor = cursor?.children[index]
    if (cursor?.kind !== `session`) throw new Error(`expected a session at ${path.join(`/`)}`)
    return cursor
  }

  it(`groups by workflow id before any heuristic`, () => {
    // No `workflow_nodes` row names the run or its issue: the row's own
    // `workflowId` is what folds it under the group.
    const a = row(`a`, { issueId: `i9`, ...member(`n1`), ...at(`2026-09-01T11:00:00Z`) })
    const x = row(`x`, { issueId: `i-other` })
    expect(ids(sessionTree([a, x], { workflows }))).toEqual([
      { workflow: `w`, children: [{ id: `a`, children: [] }] },
      { id: `x`, children: [] },
    ])
  })

  it(`never groups an unstamped row, whatever workflow_nodes say`, () => {
    // The heuristics are gone: a `workflow_nodes` row naming the issue (or
    // the session) does not group a row that carries no `workflowId`.
    const n1 = row(`n1`, { issueId: `i1`, startedReason: `workflow` })
    const tree = sessionTree([n1], {
      workflows,
      workflowNodes: [{ workflowId: `w`, issueId: `i1`, sessionId: `n1` }],
    })
    expect(ids(tree)).toEqual([{ id: `n1`, children: [] }])
  })

  it(`leaves a stamped run ungrouped when the workflow is not listed`, () => {
    // The group's NAME comes from the `workflows` row; without it nothing is
    // invented (rule 4: no hardcoded label).
    const a = row(`a`, { issueId: `i1`, ...member(`n1`) })
    expect(ids(sessionTree([a], { workflows: [] }))).toEqual([{ id: `a`, children: [] }])
  })

  it(`nests a review run under its node's author row`, () => {
    // `review` on node n1 sits as a child of n1's `author` run — no
    // `parentSessionId` needed — captioned by its branch's round.
    const a = row(`a`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const r = row(`r`, {
      ...member(`n1`, `review`),
      branch: `exp/wf-2f353e88-review-EXP-1068-r1`,
      ...at(`2026-09-01T11:00:00Z`),
    })
    const b = row(`b`, { issueId: `i2`, ...member(`n2`), ...at(`2026-09-01T09:00:00Z`) })
    const tree = sessionTree([a, r, b], { workflows })
    expect(ids(tree)).toEqual([
      {
        workflow: `w`,
        children: [
          { id: `a`, children: [{ id: `r`, children: [] }] },
          { id: `b`, children: [] },
        ],
      },
    ])
    expect(sessionAt(tree, [0, 0, 0]).reviewRound).toBe(1)
    expect(sessionAt(tree, [0, 0]).reviewRound).toBeNull()
    expect(sessionAt(tree, [0, 0]).duplicateLive).toBe(false)
  })

  it(`keeps a switched reviewer under its node`, () => {
    // An account-switch resume of the reviewer (a new row, same membership)
    // collapses into the same chain under n1's author.
    const a = row(`a`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const r1 = row(`r1`, {
      ...member(`n1`, `review`),
      status: `ended`,
      branch: `exp/wf-2f353e88-review-EXP-1068-r2`,
      ...at(`2026-09-01T11:00:00Z`),
    })
    const r2 = row(`r2`, {
      resumedFromId: `r1`,
      ...member(`n1`, `review`),
      branch: `exp/wf-2f353e88-review-EXP-1068-r2`,
      ...at(`2026-09-01T12:00:00Z`),
    })
    const tree = sessionTree([a, r1, r2], { workflows })
    expect(ids(tree)).toEqual([
      { workflow: `w`, children: [{ id: `a`, children: [{ id: `r2`, children: [] }] }] },
    ])
    const reviewer = sessionAt(tree, [0, 0, 0])
    expect(reviewer.chain.map((entry) => entry.id)).toEqual([`r1`, `r2`])
    expect(reviewer.reviewRound).toBe(2)
    // ONE live reviewer: no duplicate flag on the node.
    expect(sessionAt(tree, [0, 0]).duplicateLive).toBe(false)
  })

  it(`nests a review under the live author, not an ended one`, () => {
    const dead = row(`a0`, { issueId: `i1`, status: `ended`, ...member(`n1`), ...at(`2026-09-01T12:00:00Z`) })
    const live = row(`a1`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const r = row(`r`, { ...member(`n1`, `review`), ...at(`2026-09-01T11:00:00Z`) })
    expect(ids(sessionTree([dead, live, r], { workflows }))).toEqual([
      {
        workflow: `w`,
        children: [
          { id: `a0`, children: [] },
          { id: `a1`, children: [{ id: `r`, children: [] }] },
        ],
      },
    ])
  })

  it(`lists a review whose node has no author as a child of the group`, () => {
    const r = row(`r`, { ...member(`n1`, `review`) })
    expect(ids(sessionTree([r], { workflows }))).toEqual([
      { workflow: `w`, children: [{ id: `r`, children: [] }] },
    ])
  })

  it(`lists a base merge as a child of the group`, () => {
    // `base_merge`, `plan` and `replan` name no node: each is a plain child
    // of the group, never under a node's author run. The group's children
    // sort newest activity first.
    const a = row(`a`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const m = row(`m`, { ...member(null, `base_merge`), ...at(`2026-09-01T11:00:00Z`) })
    const p = row(`p`, { ...member(null, `plan`), ...at(`2026-09-01T08:00:00Z`) })
    const rp = row(`rp`, { ...member(null, `replan`), ...at(`2026-09-01T12:00:00Z`) })
    expect(ids(sessionTree([a, m, p, rp], { workflows }))).toEqual([
      {
        workflow: `w`,
        children: [
          { id: `rp`, children: [] },
          { id: `m`, children: [] },
          { id: `a`, children: [] },
          { id: `p`, children: [] },
        ],
      },
    ])
  })

  it(`names a plan-only group after the plan`, () => {
    // A draft whose only row is its `plan` run still draws a group, named
    // after the plan's working name (the workflow row's name).
    const tree = sessionTree([row(`p`, { ...member(null, `plan`) })], {
      workflows: [{ id: `w`, name: `Checkout rewrite`, status: `draft` }],
    })
    expect(ids(tree)).toEqual([{ workflow: `w`, children: [{ id: `p`, children: [] }] }])
    expect(group(tree).name).toBe(`Checkout rewrite`)
    expect(group(tree).status).toBe(`draft`)
  })

  it(`keeps a foreign chat that resumed a workflow run inside the group`, () => {
    // A resume performed from a chat: its row names the chat as its parent
    // but keeps the workflow membership (EXP-906 inherits it), so the
    // succession stays in the group, not under the chat.
    const c = row(`c`, at(`2026-09-01T11:00:00Z`))
    const a = row(`a`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const a2 = row(`a2`, {
      resumedFromId: `a`,
      parentSessionId: `c`,
      issueId: `i1`,
      ...member(`n1`),
      ...at(`2026-09-01T12:00:00Z`),
    })
    expect(ids(sessionTree([c, a, a2], { workflows }))).toEqual([
      { workflow: `w`, children: [{ id: `a2`, children: [] }] },
      { id: `c`, children: [] },
    ])
  })

  it(`nests a child of a node run under it inside the group`, () => {
    // A `sessions_start` child inherits the membership (§1 c) and nests under
    // its parent as before; a child with NO workflow of its own nests too.
    const a = row(`a`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const kid = row(`kid`, { parentSessionId: `a`, ...member(`n1`, null as unknown as string), ...at(`2026-09-01T10:30:00Z`) })
    const plain = row(`plain`, { parentSessionId: `a`, ...at(`2026-09-01T10:40:00Z`) })
    expect(ids(sessionTree([a, kid, plain], { workflows }))).toEqual([
      {
        workflow: `w`,
        children: [
          {
            id: `a`,
            children: [
              { id: `kid`, children: [] },
              { id: `plain`, children: [] },
            ],
          },
        ],
      },
    ])
  })

  it(`groups a person's run on a compound node's sub-issue`, () => {
    // Compound node n1 = parent i1 + sub-issue i2; only the parent has a
    // `workflow_nodes` row. A person's fresh run on i2, stamped `author` of
    // n1 by the server (EXP-1062), joins the node's group.
    const mine = row(`mine`, { issueId: `i2`, ...member(`n1`), ...at(`2026-09-01T11:00:00Z`) })
    const b = row(`b`, { issueId: `i3`, ...member(`n2`), ...at(`2026-09-01T10:00:00Z`) })
    const tree = sessionTree([mine, b], {
      workflows,
      workflowNodes: [{ id: `n1`, workflowId: `w`, issueId: `i1`, state: `running` }],
    })
    expect(ids(tree)).toEqual([
      { workflow: `w`, children: [{ id: `mine`, children: [] }, { id: `b`, children: [] }] },
    ])
  })

  it(`flags a node with two live author runs`, () => {
    // Two live `author` rows on one node (a double start): BOTH are listed,
    // neither nested under the other, and both carry the warning.
    const a1 = row(`a1`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const a2 = row(`a2`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T11:00:00Z`) })
    const tree = sessionTree([a1, a2], { workflows })
    expect(ids(tree)).toEqual([
      { workflow: `w`, children: [{ id: `a2`, children: [] }, { id: `a1`, children: [] }] },
    ])
    expect(sessionAt(tree, [0, 0]).duplicateLive).toBe(true)
    expect(sessionAt(tree, [0, 1]).duplicateLive).toBe(true)
  })

  it(`flags a node with two live reviewers, on its author row`, () => {
    const a = row(`a`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const r1 = row(`r1`, { ...member(`n1`, `review`), ...at(`2026-09-01T11:00:00Z`) })
    const r2 = row(`r2`, { ...member(`n1`, `review`), ...at(`2026-09-01T11:30:00Z`) })
    const tree = sessionTree([a, r1, r2], { workflows })
    expect(ids(tree)).toEqual([
      {
        workflow: `w`,
        children: [
          { id: `a`, children: [{ id: `r1`, children: [] }, { id: `r2`, children: [] }] },
        ],
      },
    ])
    expect(sessionAt(tree, [0, 0]).duplicateLive).toBe(true)
    expect(sessionAt(tree, [0, 0, 0]).duplicateLive).toBe(false)
  })

  it(`does not flag a node whose second author run has ended`, () => {
    const a1 = row(`a1`, { issueId: `i1`, status: `ended`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const a2 = row(`a2`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T11:00:00Z`) })
    const tree = sessionTree([a1, a2], { workflows })
    expect(sessionAt(tree, [0, 0]).duplicateLive).toBe(false)
  })

  it(`counts the group's live runs and landed nodes`, () => {
    const a = row(`a`, { issueId: `i1`, ...member(`n1`), ...at(`2026-09-01T10:00:00Z`) })
    const r = row(`r`, { ...member(`n1`, `review`), ...at(`2026-09-01T11:00:00Z`) })
    const done = row(`d`, { issueId: `i2`, status: `ended`, ...member(`n2`), ...at(`2026-09-01T09:00:00Z`) })
    const tree = sessionTree([a, r, done], {
      workflows,
      workflowNodes: [
        { id: `n1`, workflowId: `w`, issueId: `i1`, state: `running` },
        { id: `n2`, workflowId: `w`, issueId: `i2`, state: `landed` },
        { id: `n3`, workflowId: `w`, issueId: `i3`, state: `blocked` },
        { id: `other`, workflowId: `w2`, issueId: `i4`, state: `landed` },
      ],
    })
    const node = group(tree)
    expect(node.liveRuns).toBe(2)
    expect(node.nodesDone).toBe(1)
    expect(node.nodesTotal).toBe(3)
    expect(workflowGroupCaption(node)).toBe(`2 running · 1 of 3 done`)
  })

  it(`still groups a stack beside a workflow, from the leftover top level`, () => {
    const a = row(`a`, { issueId: `i1`, ...member(`n1`) })
    const low = row(`s-low`, { issueId: `i-low` })
    const top = row(`s-top`, { issueId: `i-top`, ...at(`2026-09-01T11:00:00Z`) })
    expect(
      ids(
        sessionTree([a, low, top], {
          workflows,
          issues: [
            { id: `i-low`, identifier: `APP-1`, branch: `exp/APP-1`, prBaseBranch: null },
            { id: `i-top`, identifier: `APP-2`, branch: `exp/APP-2`, prBaseBranch: `exp/APP-1` },
          ],
        })
      )
    ).toEqual([
      { stack: `i-low`, children: [{ id: `s-low`, children: [] }, { id: `s-top`, children: [] }] },
      { workflow: `w`, children: [{ id: `a`, children: [] }] },
    ])
  })
})

// EXP-1068: the strings every client draws off the tree, byte-identical ×4
// (desktop `session_tree::{workflow_group_caption, review_row_caption,
// review_branch_round, review_round_verdict}`, iOS/Android the same names).
describe(`workflowGroupCaption (EXP-1068)`, () => {
  it(`says running and done`, () => {
    expect(workflowGroupCaption({ liveRuns: 3, nodesDone: 5, nodesTotal: 8 })).toBe(
      `3 running · 5 of 8 done`
    )
  })
  it(`drops the running part with nothing live`, () => {
    expect(workflowGroupCaption({ liveRuns: 0, nodesDone: 5, nodesTotal: 8 })).toBe(`5 of 8 done`)
  })
  it(`drops the done part before the nodes synced`, () => {
    expect(workflowGroupCaption({ liveRuns: 1, nodesDone: 0, nodesTotal: 0 })).toBe(`1 running`)
  })
  it(`is empty with neither`, () => {
    expect(workflowGroupCaption({ liveRuns: 0, nodesDone: 0, nodesTotal: 0 })).toBe(``)
  })
})

describe(`reviewBranchRound (EXP-1068)`, () => {
  it(`reads the round off a review branch`, () => {
    expect(reviewBranchRound(`exp/wf-2f353e88-review-EXP-1068-r3`)).toBe(3)
    expect(reviewBranchRound(`exp/wf-2f353e88-review-EXP-10-r12`)).toBe(12)
  })
  it(`is null for every other branch`, () => {
    expect(reviewBranchRound(`exp/EXP-1068`)).toBeNull()
    expect(reviewBranchRound(`exp/wf-2f353e88-review-EXP-1068-r`)).toBeNull()
    expect(reviewBranchRound(`exp/wf-2f353e88-review-EXP-1068-r0`)).toBeNull()
    expect(reviewBranchRound(null)).toBeNull()
    expect(reviewBranchRound(``)).toBeNull()
  })
})

describe(`reviewRoundVerdict (EXP-1068)`, () => {
  const node = { reviewRound: 2, review: { round: 2, verdict: `request_changes` } }
  it(`reads the latest verdict for its round`, () => {
    expect(reviewRoundVerdict(2, node)).toBe(`changes_requested`)
    expect(reviewRoundVerdict(2, { ...node, review: { round: 2, verdict: `approve` } })).toBe(`approved`)
  })
  it(`calls an older submitted round submitted`, () => {
    expect(reviewRoundVerdict(1, node)).toBe(`submitted`)
  })
  it(`has no verdict for the pending round or without a node`, () => {
    expect(reviewRoundVerdict(3, node)).toBe(`none`)
    expect(reviewRoundVerdict(null, node)).toBe(`none`)
    expect(reviewRoundVerdict(1, null)).toBe(`none`)
    expect(reviewRoundVerdict(1, { reviewRound: 0, review: null })).toBe(`none`)
  })
})

describe(`reviewRowCaption (EXP-1068)`, () => {
  it(`names the round and the verdict`, () => {
    expect(reviewRowCaption(2, `approved`, false)).toBe(`Review r2 · approved`)
    expect(reviewRowCaption(2, `changes_requested`, false)).toBe(`Review r2 · changes requested`)
    expect(reviewRowCaption(1, `submitted`, false)).toBe(`Review r1 · submitted`)
  })
  it(`says no verdict only once the run ended`, () => {
    expect(reviewRowCaption(3, `none`, true)).toBe(`Review r3`)
    expect(reviewRowCaption(3, `none`, false)).toBe(`Review r3 · no verdict`)
  })
  it(`falls back to a bare Review without a round`, () => {
    expect(reviewRowCaption(null, `none`, true)).toBe(`Review`)
    expect(reviewRowCaption(null, `none`, false)).toBe(`Review · no verdict`)
  })
})

// EXP-1108: the row marks, replayed off the ONE contract fixture ×4.
describe(`session row marks (contract fixture)`, () => {
  for (const entry of marks.accountCaptions) {
    it(`account caption: ${entry.name}`, () => {
      expect(
        workflowRunAccountCaption(
          entry.session as SessionMarkRow,
          entry.devices as SessionMarkDevice[]
        )
      ).toBe(entry.caption)
    })
  }
  for (const entry of marks.needsYou) {
    it(`needs you: ${entry.name}`, () => {
      expect(sessionNeedsYou(entry)).toBe(entry.needsYou)
    })
  }
})
