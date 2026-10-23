// N-12: the Dock pile primitive's numbers (written in
// @exponential_app/ui/web/primitives, which ships no fixture) stay the app
// contract's (fixtures/dock.json `stack`).
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/dock.json"
import { DOCK_PILE_CONSTANTS } from "@exponential_app/ui-react/primitives/dock-card"

describe(`the Dock pile primitive`, () => {
  it(`draws dock.json's stack`, () => {
    expect(DOCK_PILE_CONSTANTS).toEqual(fixture.stack)
  })
})
