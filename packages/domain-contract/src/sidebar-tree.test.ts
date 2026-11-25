import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/sidebar-tree.json" with { type: "json" }
import type { SidebarEntry, SidebarMove, SidebarRow } from "./sidebar-folders"
import {
  SIDEBAR_DEVICES_OPEN_KEY,
  SIDEBAR_TREE_MAX_DEPTH,
  sidebarDeviceRows,
  sidebarDropSession,
  sidebarRootRows,
  sidebarTeamItems,
  sidebarTeamMenu,
  sidebarTeamOpenKey,
  sidebarTeamRows,
  sidebarTeams,
  phoneTeamMenu,
  phoneTeamPage,
  type SidebarTeamItem,
} from "./sidebar-tree"

const entries = (list: unknown) => list as SidebarEntry[]

describe(`sidebar tree (#EXP-2027)`, () => {
  test(`constants are the fixture's`, () => {
    expect(SIDEBAR_TREE_MAX_DEPTH).toBe(3)
    expect(SIDEBAR_DEVICES_OPEN_KEY).toBe(fixture.openKeys.devices)
    expect(sidebarTeamOpenKey(`t-1`)).toBe(`sidebar-team-open:t-1`)
  })
  for (const c of fixture.root) test(`root: ${c.name}`, () => expect(sidebarRootRows(c)).toEqual(c.expected as never))
  for (const c of fixture.devices) test(`devices: ${c.name}`, () => expect(sidebarDeviceRows(c)).toEqual(c.expected))
  for (const c of fixture.teams) test(`teams: ${c.name}`, () => expect(sidebarTeams(c.teams)).toEqual(c.expected))
  for (const c of fixture.teamMenu) test(`team menu: ${c.name}`, () => expect(sidebarTeamMenu(c)).toEqual(c.expected as never))
  for (const c of fixture.teamItems) {
    test(`team items: ${c.name}`, () => expect(sidebarTeamItems({ ...c, entries: entries(c.entries) })).toEqual(c.expected as never))
  }
  for (const c of fixture.teamRows) {
    test(`team rows: ${c.name}`, () => expect(sidebarTeamRows({ ...c, rows: c.rows as SidebarRow[] })).toEqual(c.expected as never))
  }
  for (const c of fixture.phone.teamMenu) test(`phone team menu: ${c.name}`, () => expect(phoneTeamMenu(c)).toEqual(c.expected as never))
  for (const c of fixture.phone.page) {
    test(`phone page: ${c.name}`, () =>
      expect(phoneTeamPage({ items: c.items as SidebarTeamItem[], entries: entries(c.entries), folderId: c.folderId })).toEqual(c.expected as never))
  }
  for (const c of fixture.drop) {
    test(`drop: ${c.name}`, () =>
      expect(sidebarDropSession({ ...c, entries: entries(c.entries), move: c.move as SidebarMove })).toEqual(c.expected as never))
  }
})
