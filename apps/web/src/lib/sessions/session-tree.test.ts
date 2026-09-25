import { describe, expect, it } from "vitest"
import {
  flattenSessionTree,
  sessionTree,
  sessionTreeNodeKey,
  visibleSessionTreeRows,
  type SessionNode,
  type SessionTreeNode,
  type SessionTreeRow,
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
    }
    const tree: SessionTreeNode[] = [
      { kind: `workflow`, workflowId: `w`, name: `W`, children: [leaf], lastActivityAt: 0 },
      { kind: `session`, session: row(`a`), chain: [row(`a`)], children: [], lastActivityAt: 0 },
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
    const n1 = row(`n1`, { issueId: `i1`, startedReason: `workflow` })
    const n2 = row(`n2`, { issueId: `i2`, startedReason: `workflow`, ...at(`2026-09-01T11:00:00Z`) })
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
    const n1 = row(`n1`, { issueId: `i1`, ...at(`2026-09-01T10:00:00Z`) })
    const n2 = row(`n2`, { issueId: `i2`, ...at(`2026-09-01T12:00:00Z`) })
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
    const parent = row(`p`, { issueId: `i1`, ...at(`2026-09-01T11:00:00Z`) })
    const child = row(`c`, { parentSessionId: `p`, ...at(`2026-09-01T10:30:00Z`) })
    const other = row(`n2`, { issueId: `i2`, ...at(`2026-09-01T10:00:00Z`) })
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

// EXP-1082 §2 — membership-first grouping, DECLARED for EXP-1068. Each case
// states the expectation; EXP-1068 fills the rows and un-skips.
describe(`sessionTree — workflow membership (EXP-1082 → EXP-1068)`, () => {
  it.skip(`groups by workflow id before any heuristic`, () => {
    // A row with `workflowId` lands in that workflow's group even when no
    // `workflow_nodes` row names its session or issue.
  })
  it.skip(`nests a review run under its node's author row`, () => {
    // `workflowRole: review` + `workflowNodeId` = N sits as a child of N's
    // `author` run inside the group.
  })
  it.skip(`keeps a switched reviewer under its node`, () => {
    // An account-switch resume of the reviewer (a new row, same membership)
    // collapses into the same chain under N's author row.
  })
  it.skip(`lists a base merge as a child of the group`, () => {
    // `workflowRole: base_merge` (no node) is a direct child of the group.
  })
  it.skip(`names a plan-only group after the plan`, () => {
    // A workflow whose only row is its `plan` run still draws a group,
    // named after the workflow.
  })
  it.skip(`keeps a foreign chat that resumed a workflow run inside the group`, () => {
    // A resume performed from a chat (parent = the chat, startedReason kept
    // `workflow`) stays in the workflow group, not under the chat.
  })
  it.skip(`groups a person's run on a compound node's sub-issue`, () => {
    // A person's fresh run on a member issue (stamped author of the compound
    // node by the server) joins the node's group.
  })
  it.skip(`flags a node with two live author runs`, () => {
    // Two live `author` rows on one node: both listed, the node flagged.
  })
})
