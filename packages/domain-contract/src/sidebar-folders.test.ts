import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/sidebar-folders.json" with { type: "json" }
import {
  SIDEBAR_FOLDER_DEFAULT_NAME,
  SIDEBAR_MAX_DEPTH,
  sidebarFolderName,
  sidebarFolderOpenKey,
  sidebarMove,
  sidebarMoveToNewFolder,
  sidebarRows,
  sidebarTree,
  sidebarUngroup,
  type SidebarEntry,
  type SidebarMove,
} from "./sidebar-folders"

const entries = (list: unknown) => list as SidebarEntry[]

describe(`sidebar folders (#EXP-1832)`, () => {
  test(`constants are the fixture's`, () => {
    expect(SIDEBAR_MAX_DEPTH).toBe(fixture.maxDepth)
    expect(SIDEBAR_FOLDER_DEFAULT_NAME).toBe(fixture.defaultFolderName)
  })

  for (const c of fixture.tree) {
    test(`tree: ${c.name}`, () => {
      expect(sidebarTree(c.items, entries(c.entries))).toEqual(c.expected as never)
    })
  }

  for (const c of fixture.move) {
    test(`move: ${c.name}`, () => {
      expect(
        sidebarMove({ items: c.items, entries: entries(c.entries), move: c.move as SidebarMove, newFolderId: c.newFolderId })
      ).toEqual(c.expected as never)
    })
  }

  for (const c of fixture.ungroup) {
    test(`ungroup: ${c.name}`, () => {
      expect(sidebarUngroup(c.items, entries(c.entries), c.folderId)).toEqual(c.expected as never)
    })
  }

  for (const c of fixture.rows) {
    test(`rows: ${c.name}`, () => {
      const closed = new Set(c.closed)
      expect(sidebarRows(sidebarTree(c.items, entries(c.entries)), (id) => !closed.has(id))).toEqual(c.expected as never)
    })
  }

  for (const c of fixture.folderNames) {
    test(`folder name: ${JSON.stringify(c.input).slice(0, 24)}`, () => {
      expect(sidebarFolderName(c.input)).toBe(c.expected)
    })
  }

  for (const c of fixture.openKeys) {
    test(`open key`, () => {
      expect(sidebarFolderOpenKey(c.teamId, c.folderId)).toBe(c.expected)
    })
  }
})

describe(`#EXP-2124 Move to folder › New folder`, () => {
  const e = (id: string, kind: `folder` | `item`, targetId: string | null, parentId: string | null, sortOrder: number, name: string | null = null): SidebarEntry => ({ id, kind, targetId, parentId, name, sortOrder })

  test(`a top-level item: the new folder takes its place, holding it`, () => {
    expect(sidebarMoveToNewFolder([`a`, `b`, `c`], [], `b`, `f`)).toEqual({
      upserts: [
        { id: null, kind: `item`, targetId: `a`, parentId: null, name: null, sortOrder: 1 },
        { id: `f`, kind: `folder`, targetId: null, parentId: null, name: SIDEBAR_FOLDER_DEFAULT_NAME, sortOrder: 2 },
        { id: null, kind: `item`, targetId: `b`, parentId: `f`, name: null, sortOrder: 1 },
        { id: null, kind: `item`, targetId: `c`, parentId: null, name: null, sortOrder: 3 },
      ],
      deletes: [],
      createdFolderId: `f`,
    })
  })

  test(`the last item of a folder: the new folder follows it and the emptied one goes`, () => {
    const writes = sidebarMoveToNewFolder([`a`, `b`], [e(`g`, `folder`, null, null, 1, `G`), e(`ea`, `item`, `a`, `g`, 1)], `a`, `f`)
    expect(writes?.deletes).toEqual([`g`])
    expect(writes?.upserts).toContainEqual({ id: `f`, kind: `folder`, targetId: null, parentId: null, name: SIDEBAR_FOLDER_DEFAULT_NAME, sortOrder: 1 })
    expect(writes?.upserts).toContainEqual({ id: `ea`, kind: `item`, targetId: `a`, parentId: `f`, name: null, sortOrder: 1 })
  })

  test(`an unknown item: nothing`, () => {
    expect(sidebarMoveToNewFolder([`a`], [], `x`, `f`)).toBeNull()
  })
})
