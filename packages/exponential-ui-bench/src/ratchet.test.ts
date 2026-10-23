import { describe, expect, it } from "bun:test"
import { budgetsFor, check, failing, type Budgets } from "./ratchet"
import type { Fragment } from "./results"

const budgets: Budgets = { floor: 0.5, headroom: 1.5, machines: { m: { core: { "list-100k": { firstPaintMs: 100, scrollStepMs: 5, renderedItems: 30 } } } } }
const fragment = (scenarios: Fragment[`scenarios`]): Fragment => ({ renderer: `core`, platform: `linux`, machine: `m`, date: `2026-10-22`, scenarios })

describe(`ratchet`, () => {
  it(`worse fails, a much better timing warns, a better count fails, a missing budget is a notice`, () => {
    const v = check(fragment({ "list-100k": { firstPaintMs: 120, scrollStepMs: 1, renderedItems: 28, scrollToIndexMs: 7 } }), budgets)
    expect(v.map((x) => [x.metric, x.status])).toEqual([
      [`firstPaintMs`, `worse`],
      [`scrollStepMs`, `better`],
      [`renderedItems`, `better`],
      [`scrollToIndexMs`, `unbudgeted`],
    ])
    expect(failing(v).map((x) => x.metric)).toEqual([`firstPaintMs`, `renderedItems`])
  })

  it(`within budget is ok; a timing a little better is ok`, () => {
    const v = check(fragment({ "list-100k": { firstPaintMs: 90, scrollStepMs: 4, renderedItems: 30 } }), budgets)
    expect(v.every((x) => x.status === `ok`)).toBe(true)
  })

  it(`--write puts headroom on timings and keeps counts exact`, () => {
    const next = budgetsFor(fragment({ "list-100k": { firstPaintMs: 80, renderedItems: 31 }, "cold-start": { coldStartMs: 10.04 } }), budgets)
    expect(next.machines.m.core[`list-100k`]).toEqual({ firstPaintMs: 120, renderedItems: 31 })
    expect(next.machines.m.core[`cold-start`]).toEqual({ coldStartMs: 15.1 })
    expect(budgets.machines.m.core[`list-100k`].scrollStepMs).toBe(5)
  })
})
