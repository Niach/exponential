import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/ide-sidebar.json" with { type: "json" }
import { ideSidebar, ideTabPlacement, ideTabs, type IdeSidebarInput } from "./ide"

describe(`ide-sidebar.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      expect(ideSidebar({ ...fixture.base, ...c.input } as IdeSidebarInput)).toEqual(c.expected as ReturnType<typeof ideSidebar>)
    })
  }
})

describe(`ide-sidebar.json tabs`, () => {
  for (const c of fixture.tabs) {
    test(c.name, () => {
      expect(ideTabs(c.tabs, c.active)).toEqual(c.expected)
    })
  }
})

describe(`ide-sidebar.json placement`, () => {
  for (const c of fixture.placement) {
    test(c.name, () => {
      expect(ideTabPlacement(c.ede)).toEqual(c.expected)
    })
  }
})
