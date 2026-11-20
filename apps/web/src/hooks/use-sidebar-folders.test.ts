import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({ sidebarLayoutCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

import fixture from "@exp/domain-contract/fixtures/sidebar-folders.json"
import { sidebarMove, sidebarRows, sidebarTree, type SidebarEntry, type SidebarMove } from "@exp/domain-contract/sidebar-folders"
import { applySidebarWrites, withEditingFolder } from "@/hooks/use-sidebar-folders"

// #EXP-1832: the optimistic overlay a drop shows before its write lands must
// draw exactly the tree the server's rows will (every `move` case of the
// fixture: the overlay's tree = the tree of the moved layout).
describe(`applySidebarWrites`, () => {
  for (const c of fixture.move) {
    if (!c.expected) continue
    it(c.name, () => {
      const entries = c.entries as SidebarEntry[]
      const writes = sidebarMove({ items: c.items, entries, move: c.move as SidebarMove, newFolderId: c.newFolderId })!
      const after = sidebarTree(c.items, applySidebarWrites(entries, writes))
      // Replaying the same move on the result is idempotent in shape: the
      // tree a second render draws equals the one the first drew.
      expect(sidebarTree(c.items, applySidebarWrites(applySidebarWrites(entries, writes), { upserts: [], deletes: [], createdFolderId: null }))).toEqual(after)
      // The dragged row now sits where the move put it.
      const ids = after.flatMap((node) => (node.kind === `folder` ? [node.id, ...node.children] : [node.id]))
      expect(ids).toContain(c.move.id)
    })
  }
})

// #EXP-2027 (sidebar-tree.json New folder): the empty folder being named is
// SHOWN (appended at the end), then hidden like any empty folder.
describe(`withEditingFolder`, () => {
  const folder: SidebarEntry = { id: `f-new`, kind: `folder`, targetId: null, parentId: null, name: `New folder`, sortOrder: 3 }
  const rows = sidebarRows(sidebarTree([`b-1`], [folder]), () => true)

  it(`shows the empty folder while it is named`, () => {
    expect(rows.map((row) => row.id)).toEqual([`b-1`])
    expect(withEditingFolder(rows, [folder], `f-new`)).toEqual([
      ...rows,
      { kind: `folder`, id: `f-new`, depth: 0, group: true, name: `New folder`, open: true, count: 0 },
    ])
  })

  it(`hides it once the name is done, and never doubles a shown folder`, () => {
    expect(withEditingFolder(rows, [folder], null)).toEqual(rows)
    expect(withEditingFolder(rows, [folder], `b-1`)).toEqual(rows)
  })
})
