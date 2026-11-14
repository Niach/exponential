import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/list-reorder.json" with { type: "json" }
import { listDropSlot, listMoveContext, type ListReorderRow } from "./list-reorder"

describe(`EXP-1832 fixtures/list-reorder.json`, () => {
  for (const c of fixture.dropSlot) {
    test(`dropSlot: ${c.name}`, () => {
      const drop = listDropSlot(c.rows as ListReorderRow[], c.drag, c.at, c.maxDepth)
      expect(drop).toEqual(c.expected as never)
      expect(drop ? listMoveContext(c.drag, drop) : null).toEqual(c.move as never)
    })
  }
})
