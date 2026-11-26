import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/list-reorder.json" with { type: "json" }
import board from "../fixtures/board-drop.json" with { type: "json" }
import { boardDropSlot, boardMoveContext, listDropSlot, listMoveContext, type BoardColumn, type ListReorderRow } from "./list-reorder"

describe(`EXP-1832 fixtures/list-reorder.json`, () => {
  for (const c of fixture.dropSlot) {
    test(`dropSlot: ${c.name}`, () => {
      const drop = listDropSlot(c.rows as ListReorderRow[], c.drag, c.at, c.maxDepth)
      expect(drop).toEqual(c.expected as never)
      expect(drop ? listMoveContext(c.drag, drop) : null).toEqual(c.move as never)
    })
  }
})

describe(`genUI round 4 fixtures/board-drop.json`, () => {
  for (const c of board.boardDropSlot) {
    test(`boardDropSlot: ${c.name}`, () => {
      const drop = boardDropSlot(board.columns as BoardColumn[], c.drag, c.x, c.y)
      expect(drop).toEqual(c.expected as never)
      expect(drop ? boardMoveContext(c.drag, drop) : null).toEqual(c.move as never)
    })
  }
})
