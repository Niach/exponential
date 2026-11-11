import { describe, expect, it } from "bun:test"
import fixture from "../../fixtures/interactions-round1.json"
import { selectOpensWithoutCursor } from "./listbox"

describe(`interactions-round1.json › selectPointerOpen (#EXP-1629)`, () => {
  for (const c of fixture.selectPointerOpen) {
    it(c.name, () => {
      expect(selectOpensWithoutCursor({ triggerVariant: c.triggerVariant }, true)).toBe(!c.cursor)
      // The keyboard always keeps the cursor.
      expect(selectOpensWithoutCursor({ triggerVariant: c.triggerVariant }, false)).toBe(false)
    })
  }
})
