import { describe, expect, it } from "vitest"
import {
  flattenSessionTree,
  sessionTree,
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

describe.skip(`sessionTree (EXP-1029 → EXP-996)`, () => {
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
