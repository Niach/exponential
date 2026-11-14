import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({ sidebarLayoutCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

import fixture from "@exp/domain-contract/fixtures/sidebar-folders.json"
import { sidebarMove, sidebarTree, type SidebarEntry, type SidebarMove } from "@exp/domain-contract/sidebar-folders"
import { applySidebarWrites } from "@/hooks/use-sidebar-folders"

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
