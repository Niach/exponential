import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/workflow-view.json"
import {
  pruneSelection,
  selectNode,
  stepSelection,
  type StripSelection,
} from "./workflow-selection"

// EXP-1084: the picker model, locked ×4 by the fixture's `selection` cases.
describe(`workflow selection (contract fixture)`, () => {
  for (const c of fixture.selection) {
    it(c.name, () => {
      const current = c.current as StripSelection
      const op = c.op as { kind: string; id?: string | null; delta?: 1 | -1 }
      const got =
        op.kind === `click`
          ? selectNode(current, op.id ?? null, c.order)
          : op.kind === `toggle`
            ? selectNode(current, op.id ?? null, c.order, { toggle: true })
            : op.kind === `extend`
              ? selectNode(current, op.id ?? null, c.order, { extend: true })
              : op.kind === `step`
                ? stepSelection(current, op.delta!, c.order)
                : pruneSelection(current, c.order)
      expect(got).toEqual(c.expected)
    })
  }
})
