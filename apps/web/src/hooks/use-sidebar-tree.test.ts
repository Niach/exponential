import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({}))

import type { SidebarEntry } from "@exp/domain-contract/sidebar-folders"
import { registerSidebarTeamDrop, sidebarTeamDrop, teamListMove } from "@/hooks/use-sidebar-tree"

// #EXP-2027 (sidebar-tree.json `teamRows`): the team row heads its List, so a
// drop can name it: into / after it = the list's end, before it = nothing.
describe(`teamListMove`, () => {
  const entries: SidebarEntry[] = [
    { id: `f-1`, kind: `folder`, targetId: null, parentId: null, name: `Mobile`, sortOrder: 1 },
    { id: `e-1`, kind: `item`, targetId: `b-1`, parentId: `f-1`, name: null, sortOrder: 1 },
  ]
  const items = [`b-1`, `a-1`, `s-1`]

  it(`passes every move that does not name the team`, () => {
    expect(teamListMove({ id: `s-1`, into: `f-1` }, `t-1`, items, entries)).toEqual({ move: { id: `s-1`, into: `f-1` } })
  })

  it(`lands a drop into or after the team row at the list's end`, () => {
    expect(teamListMove({ id: `a-1`, into: `t-1` }, `t-1`, items, entries)).toEqual({ move: { id: `a-1`, after: `s-1` } })
    expect(teamListMove({ id: `s-1`, after: `t-1` }, `t-1`, items, entries)).toEqual({ move: { id: `s-1`, after: `a-1` } })
  })

  it(`refuses a drop before the team row; an empty team appends`, () => {
    expect(teamListMove({ id: `s-1`, before: `t-1` }, `t-1`, items, entries)).toBeNull()
    expect(teamListMove({ id: `s-9`, into: `t-1` }, `t-1`, [], [])).toEqual({ append: true })
  })
})

describe(`the team drop registry`, () => {
  it(`hands an Active row's drop to the team's mounted list`, () => {
    const drop = vi.fn()
    const unregister = registerSidebarTeamDrop(`t-1`, drop)
    sidebarTeamDrop(`t-1`)?.({ id: `s-1`, teamId: `t-1`, parentId: null }, { id: `s-1`, after: `b-1` })
    expect(drop).toHaveBeenCalledWith({ id: `s-1`, teamId: `t-1`, parentId: null }, { id: `s-1`, after: `b-1` })
    expect(sidebarTeamDrop(`t-2`)).toBeNull()
    unregister()
    expect(sidebarTeamDrop(`t-1`)).toBeNull()
  })
})
