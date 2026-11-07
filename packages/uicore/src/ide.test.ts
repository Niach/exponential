import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/ide-sidebar.json" with { type: "json" }
import { ideSidebar, type IdeSidebarInput } from "./ide"

describe(`ide-sidebar.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      expect(ideSidebar({ ...fixture.base, ...c.input } as IdeSidebarInput)).toEqual(c.expected as ReturnType<typeof ideSidebar>)
    })
  }
})
