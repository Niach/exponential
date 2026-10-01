import { describe, expect, it } from "vitest"
import marks from "@exp/domain-contract/fixtures/session-tree-marks.json"
import {
  flattenSessionTree,
  sessionDescendantIds,
  sessionNeedsYou,
  sessionTree,
  sessionTreeNodeKey,
  visibleSessionTreeRows,
  type SessionNode,
  type SessionTreeNode,
  type SessionTreeRow,
} from "./session-tree"

// EXP-1029 contract — the session tree (`lib/sessions/session-tree.ts`).
// Its Rust, Swift and Kotlin mirrors carry the same case names.

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
  nodes.map((node) => ({ id: node.session.id, children: ids(node.children) }))

describe(`flattenSessionTree (EXP-1029 contract)`, () => {
  it(`walks children depth-first`, () => {
    const leaf: SessionNode = {
      kind: `session`,
      session: row(`b`),
      chain: [row(`b`)],
      children: [],
      lastActivityAt: 0,
    }
    const tree: SessionTreeNode[] = [
      {
        kind: `session`,
        session: row(`p`),
        chain: [row(`p`)],
        children: [leaf],
        lastActivityAt: 0,
      },
      {
        kind: `session`,
        session: row(`a`),
        chain: [row(`a`)],
        children: [],
        lastActivityAt: 0,
      },
    ]
    expect(flattenSessionTree(tree).map((node) => node.session.id)).toEqual([
      `p`,
      `b`,
      `a`,
    ])
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
    expect(tree[0]!.chain.map((r) => r.id)).toEqual([`r1`, `r2`])
  })

  it(`follows a child to its parent's resume succession`, () => {
    const p1 = row(`p1`)
    const p2 = row(`p2`, { resumedFromId: `p1`, ...at(`2026-09-01T11:00:00Z`) })
    const child = row(`c`, { parentSessionId: `p1` })
    expect(ids(sessionTree([p1, p2, child]))).toEqual([
      { id: `p2`, children: [{ id: `c`, children: [] }] },
    ])
  })

  it(`sorts a parent by its subtree's last activity`, () => {
    const lone = row(`lone`, at(`2026-09-01T11:30:00Z`))
    const parent = row(`p`, at(`2026-09-01T10:00:00Z`))
    const child = row(`c`, { parentSessionId: `p`, ...at(`2026-09-01T12:00:00Z`) })
    const tree = sessionTree([lone, parent, child])
    expect(tree.map((node) => node.session.id)).toEqual([`p`, `lone`])
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
  const tree = () => {
    const parent = row(`p`, at(`2026-09-01T11:00:00Z`))
    const child = row(`c`, { parentSessionId: `p`, ...at(`2026-09-01T10:30:00Z`) })
    const grandchild = row(`g`, { parentSessionId: `c`, ...at(`2026-09-01T10:40:00Z`) })
    const other = row(`o`, at(`2026-09-01T10:00:00Z`))
    return sessionTree([parent, child, grandchild, other])
  }

  it(`flattens children with their depths`, () => {
    expect(
      visibleSessionTreeRows(tree()).map((flat) => [flat.key, flat.depth])
    ).toEqual([
      [`p`, 0],
      [`c`, 1],
      [`g`, 2],
      [`o`, 0],
    ])
  })

  it(`hides everything under a collapsed node`, () => {
    const rows = visibleSessionTreeRows(tree(), new Set([`c`]))
    expect(rows.map((flat) => flat.key)).toEqual([`p`, `c`, `o`])
    expect(rows.find((flat) => flat.key === `c`)!.hasChildren).toBe(true)
  })

  it(`keys a session by its id`, () => {
    expect(sessionTreeNodeKey(tree()[0]!)).toBe(`p`)
  })
})

describe(`sessionDescendantIds`, () => {
  it(`lists every nested row, resumes included`, () => {
    const parent = row(`p`)
    const child = row(`c`, { parentSessionId: `p` })
    const resumed = row(`c2`, { resumedFromId: `c`, ...at(`2026-09-01T11:00:00Z`) })
    const grandchild = row(`g`, { parentSessionId: `c2` })
    const nodes = sessionTree([parent, child, resumed, grandchild])
    expect(sessionDescendantIds(nodes, `p`).sort()).toEqual([`c`, `c2`, `g`])
    expect(sessionDescendantIds(nodes, `g`)).toEqual([])
    expect(sessionDescendantIds(nodes, `missing`)).toEqual([])
  })
})

// EXP-1108: the row marks, replayed off the ONE contract fixture ×4.
describe(`session row marks (contract fixture)`, () => {
  for (const entry of marks.needsYou) {
    it(`needs you: ${entry.name}`, () => {
      expect(sessionNeedsYou(entry)).toBe(entry.needsYou)
    })
  }
})
