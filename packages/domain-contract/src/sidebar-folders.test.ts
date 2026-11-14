import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/sidebar-folders.json" with { type: "json" }
import {
  SIDEBAR_FOLDER_DEFAULT_NAME,
  SIDEBAR_MAX_DEPTH,
  sidebarFolderName,
  sidebarFolderOpenKey,
  sidebarMove,
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
