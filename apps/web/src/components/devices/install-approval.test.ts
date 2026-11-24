// #EXP-1950: the web row replays install-trust.json `row` (the tree ×4).
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/install-trust.json" with { type: "json" }
import { INSTALL_TRUST_COPY, type TrustRow } from "@exp/domain-contract/install-trust"
import { pendingRow, WAITING_CLASS } from "./install-approval"

type Node = { component: string; props?: Record<string, unknown>; children?: Node[]; slots?: Record<string, Node>; id?: string; on?: unknown }

/** The tree without ids and handlers, values swapped for the fixture's names. */
function shape(node: Node, names: Map<unknown, string>): unknown {
  const props = node.props ? Object.fromEntries(Object.entries(node.props).map(([k, v]) => [k, names.get(v) ?? v])) : undefined
  return {
    ...(node.id ? { id: node.id.replace(/^install-i(?=$|-)/, `install-<id>`) } : {}),
    component: node.component,
    ...(props ? { props } : {}),
    ...(node.children ? { children: node.children.map((child) => shape(child, names)) } : {}),
    ...(node.slots ? { slots: Object.fromEntries(Object.entries(node.slots).map(([k, v]) => [k, shape(v, names)])) } : {}),
  }
}

describe(`install-trust.json row`, () => {
  it(`each install is the fixture's Stack > Row (label, code, Badge) + the Buttons on their own line`, () => {
    for (const kind of [`newInstall`, `reset`] as const) {
      const row: TrustRow = { kind, installId: `i`, endpointId: `e`.repeat(64), userId: `u`, label: `Pixel 9`, code: `HNMBN A0319` }
      const names = new Map<unknown, string>([
        [`Pixel 9`, `label`],
        [`HNMBN A0319`, `code`],
        [INSTALL_TRUST_COPY[kind], `kind`],
        [INSTALL_TRUST_COPY.approve, `approve`],
        [INSTALL_TRUST_COPY.remove, `remove`],
      ])
      expect(shape(pendingRow(row, `Pixel 9`) as Node, names)).toEqual(fixture.row.tree.children[0])
    }
    expect(fixture.row.neverTruncated).toBe(`code`)
    expect(fixture.row.minWidthPt).toBe(320)
  })
})

describe(`install-trust.json waiting`, () => {
  it(`the waiting state starts the fixture's share of the window down, in its column`, () => {
    // Tailwind: `max-w-md` = 28rem, the page's `gap-3` = 12.
    expect(WAITING_CLASS.split(` `)).toEqual([`mx-auto`, `w-full`, `max-w-md`, `pt-[${fixture.waiting.topOffset * 100}vh]`])
    expect(fixture.waiting.maxWidth).toBe(28 * 16)
    expect(fixture.waiting.listGap).toBe(3 * 4)
  })
})
