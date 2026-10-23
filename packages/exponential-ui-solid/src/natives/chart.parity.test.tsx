// FEED-128: DOM parity of the Chart port with React's natives/chart.tsx:
// every catalog fixture case (the example × every kind × both booleans),
// the specimen, the dashboard's sparklines, more shapes (several series,
// negatives, gaps, labels, a full pie, a title), dark + rtl + forced states.

import { describe, expect, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import components from "@exponential_app/ui/fixtures/catalog-components.json"
import specimens from "@exponential_app/ui/fixtures/specimens.json"
import dashboard from "@exponential_app/ui/fixtures/dashboard.json"
import dashboardData from "@exponential_app/ui/fixtures/dashboard.data.json"
import { renderBoth, renderReact, renderSolidSurface, serializeDom, type ParityCase } from "../test/parity"

// src/test/parity.ts's GENERATED_ID misses React 19.2's `useId` form
// (`_r_1l_`, the browser check's parity-compare.ts knows it), so the title's
// `<useId>-title` stays raw on the React side while Solid's `cl-…-title` is
// renamed: both sides' generated id VALUES collapse to one token here.
const GEN = /="(_r_[0-9a-z]+_[^"]*|\$id\d+)"/g
async function expectDomParity(c: ParityCase): Promise<void> {
  const { react, solid } = await renderBoth(c)
  expect(solid.replace(GEN, `="$gen"`)).toBe(react.replace(GEN, `="$gen"`))
}

const cases = (components as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => c.node.component === `Chart`)
const specimen = (specimens as unknown as { specimens: { component: string; node: NestedNode }[] }).specimens.find((s) => s.component === `Chart`)!

/** Every node of a tree whose component is one of `names`. */
function find(node: unknown, names: readonly string[], out: NestedNode[] = []): NestedNode[] {
  if (Array.isArray(node)) for (const n of node) find(n, names, out)
  else if (node && typeof node === `object`) {
    const n = node as Record<string, unknown>
    if (typeof n.component === `string` && names.includes(n.component) && typeof n.id === `string`) out.push(n as unknown as NestedNode)
    for (const v of Object.values(n)) find(v, names, out)
  }
  return out
}
const sparklines = find(dashboard, [`Sparkline`, `Chart`])

const chart = (id: string, props: Record<string, unknown>): NestedNode => ({ id, component: `Chart`, props }) as NestedNode
const shapes: Record<string, NestedNode> = {
  "two series bar, title, labels": chart(`c`, { kind: `bar`, title: `Runs`, xLabel: `Day`, yLabel: `Count`, showValues: true, categories: [`Mon`, `Tue`, `Wed`], series: [{ name: `Ok`, values: [3, 5, 2] }, { name: `Failed`, values: [1, -2, 4], tone: `danger` }] }),
  "stacked bar with values": chart(`c`, { kind: `stackedBar`, showValues: true, categories: [`a`, `b`, `c`], series: [{ name: `x`, values: [3, 5, 2] }, { name: `y`, values: [1, 2, 4] }, { name: `z`, values: [0.5, 1.25, 3.75] }] }),
  "line with a gap, values, min/max": chart(`c`, { kind: `line`, showValues: true, min: -10, max: 20, categories: [`a`, `b`, `c`, `d`], series: [{ name: `x`, values: [3, null, 7.333, 2] }, { name: `y`, values: [1, 2, `n/a`, 4], tone: `success` }] }),
  "area, three series": chart(`c`, { kind: `area`, height: 260, categories: [`Q1`, `Q2`, `Q3`], series: [{ name: `a`, values: [1, 2, 3] }, { name: `b`, values: [3, 2, 1] }, { name: `c`, values: [2, 2, 2] }] }),
  "pie, values": chart(`c`, { kind: `pie`, showValues: true, categories: [`a`, `b`, `c`], series: [{ name: `share`, values: [1, 2, 3.5] }] }),
  "pie, one full slice": chart(`c`, { kind: `pie`, categories: [`all`], series: [{ name: `share`, values: [7] }] }),
  "donut, two rings, values": chart(`c`, { kind: `donut`, showValues: true, categories: [`a`, `b`, `c`], series: [{ name: `2025`, values: [1, 2, 3] }, { name: `2026`, values: [4, 0, 2] }] }),
  "donut, a full ring": chart(`c`, { kind: `donut`, categories: [`a`, `b`], series: [{ name: `s`, values: [5, 0] }] }),
  "sparkline, one point": chart(`c`, { kind: `sparkline`, series: [{ name: `s`, values: [4] }] }),
  "sparkline, many": chart(`c`, { kind: `sparkline`, height: 40, series: [{ name: `s`, values: [4, 8, 1, 9, 3, 3, 12] }] }),
  "empty chart": chart(`c`, { kind: `bar` }),
  "categories longer than values": chart(`c`, { kind: `line`, categories: [`a`, `b`, `c`, `d`, `e`], series: [{ name: `s`, values: [1, 2] }] }),
  "legend off with two series": chart(`c`, { kind: `bar`, showLegend: false, categories: [`a`], series: [{ name: `x`, values: [1] }, { name: `y`, values: [2] }] }),
  "axes off, grid on": chart(`c`, { kind: `line`, showAxes: false, categories: [`a`, `b`], series: [{ name: `x`, values: [1000, 25000] }] }),
}

describe(`Chart DOM parity with React`, () => {
  for (const c of cases) it(`fixture ${c.name}`, async () => expectDomParity({ tree: c.node }))
  it(`the Chart specimen`, async () => expectDomParity({ tree: specimen.node }))
  it(`the Chart specimen, exponential dark rtl`, async () => expectDomParity({ tree: specimen.node, theme: `exponential`, mode: `dark`, direction: `rtl` }))
  for (const s of sparklines) it(`dashboard ${s.id}`, async () => expectDomParity({ tree: s, data: dashboardData as Record<string, unknown>, theme: `exponential` }))
  for (const [name, tree] of Object.entries(shapes)) {
    it(name, async () => expectDomParity({ tree }))
    it(`${name}, orange dark rtl`, async () => expectDomParity({ tree, theme: `orange`, mode: `dark`, direction: `rtl`, locale: `de-AT` }))
  }
  for (const state of [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`]) it(`forced ${state}`, async () => expectDomParity({ tree: shapes[`two series bar, title, labels`], states: [state] }))
})

// The highlighted category (keyboard on the focused plot): the band, the
// dimmed bars/slices, the bigger dot, `data-active` and the tooltip part.
describe(`Chart DOM parity with React, a highlighted category`, () => {
  const keyed = async (c: ParityCase, keys: string[], highlighted = true) => {
    const { act } = await import(`react`)
    const r = await renderReact(c)
    const s = await renderSolidSurface(c)
    try {
      for (const key of keys) {
        await act(async () => void r.root.querySelector(`svg`)!.dispatchEvent(new KeyboardEvent(`keydown`, { key, bubbles: true })))
        s.root.querySelector(`svg`)!.dispatchEvent(new KeyboardEvent(`keydown`, { key, bubbles: true }))
      }
      const react = serializeDom(r.root)
      expect(react.includes(`Chart/tooltip`)).toBe(highlighted)
      expect(serializeDom(s.root).replace(GEN, `="$gen"`)).toBe(react.replace(GEN, `="$gen"`))
    } finally {
      r.cleanup()
      s.cleanup()
    }
  }
  for (const name of [`two series bar, title, labels`, `stacked bar with values`, `line with a gap, values, min/max`, `area, three series`, `pie, values`, `donut, two rings, values`, `empty chart`]) {
    it(`${name}: ArrowRight`, async () => keyed({ tree: shapes[name] }, [`ArrowRight`], name !== `empty chart`))
    it(`${name}: ArrowLeft ×2, rtl`, async () => keyed({ tree: shapes[name], direction: `rtl` }, [`ArrowLeft`, `ArrowLeft`], name !== `empty chart`))
    it(`${name}: End then Escape`, async () => keyed({ tree: shapes[name] }, [`End`, `Escape`], false))
  }
})
