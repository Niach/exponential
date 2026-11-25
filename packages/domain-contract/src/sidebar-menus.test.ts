import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/sidebar-menus.json" with { type: "json" }
import {
  SIDEBAR_MENU_KINDS,
  newTabMenu,
  sidebarItemMenu,
  sidebarMenuDestructive,
  sidebarNewKinds,
  type SidebarMenuKind,
} from "./sidebar-menus"

describe(`sidebar menus (#EXP-2124)`, () => {
  for (const c of fixture.menuCases) {
    test(`menu: ${c.name}`, () => expect(sidebarItemMenu(c.kind as SidebarMenuKind, c.input)).toEqual(c.expected as never))
  }
  for (const c of fixture.newCases) test(`new: ${c.name}`, () => expect(sidebarNewKinds(c)).toEqual(c.expected as never))
  for (const c of fixture.newTabCases) test(`new tab: ${c.name}`, () => expect(newTabMenu(c)).toEqual(c.expected as never))
  test(`every kind has a case, every verb a label and an icon`, () => {
    expect(new Set(fixture.menuCases.map((c) => c.kind))).toEqual(new Set<string>(SIDEBAR_MENU_KINDS))
    for (const kind of SIDEBAR_MENU_KINDS) {
      for (const id of sidebarItemMenu(kind, { owner: true, mine: true, canDelete: true })) {
        if (id === `separator`) continue
        expect(fixture.copy[id]).toBeTruthy()
        expect(fixture.icons[id]).toBeTruthy()
      }
    }
  })
  test(`the only destructive rows: a board's Archive, an Active row's Delete`, () => {
    expect(sidebarMenuDestructive(`board`, `archive`)).toBe(true)
    expect(sidebarMenuDestructive(`active-session`, `delete`)).toBe(true)
    expect(SIDEBAR_MENU_KINDS.flatMap((kind) => fixture.kinds[kind].danger).length).toBe(2)
  })
})
