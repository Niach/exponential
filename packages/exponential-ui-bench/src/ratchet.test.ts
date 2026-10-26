import { describe, expect, it } from "bun:test"
import { budgetsFor, check, describe as line, failing, headroomOf, timingMode, worstOf, type Budgets } from "./ratchet"
import type { Fragment } from "./results"

const budgets: Budgets = { floor: 0.5, headroom: 1.5, machines: { m: { core: { "list-100k": { firstPaintMs: 100, scrollStepMs: 5, renderedItems: 30 } } } } }
const fragment = (scenarios: Fragment[`scenarios`]): Fragment => ({ renderer: `core`, platform: `linux`, machine: `m`, date: `2026-10-22`, scenarios })

describe(`ratchet`, () => {
  it(`a worse timing warns, a much better timing warns, a better count fails, a missing budget is a notice`, () => {
    const v = check(fragment({ "list-100k": { firstPaintMs: 120, scrollStepMs: 1, renderedItems: 28, scrollToIndexMs: 7 } }), budgets)
    expect(v.map((x) => [x.metric, x.status])).toEqual([
      [`firstPaintMs`, `worse`],
      [`scrollStepMs`, `better`],
      [`renderedItems`, `better`],
      [`scrollToIndexMs`, `unbudgeted`],
    ])
    expect(failing(v).map((x) => x.metric)).toEqual([`renderedItems`])
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

  it(`a machine's own headroom wins over the file's`, () => {
    const hosted: Budgets = { ...budgets, machineHeadroom: { hosted: 2.5 } }
    expect(headroomOf(hosted, `hosted`)).toBe(2.5)
    expect(headroomOf(hosted, `m`)).toBe(1.5)
    const next = budgetsFor({ ...fragment({ "list-100k": { firstPaintMs: 80, renderedItems: 31 } }), machine: `hosted` }, hosted)
    expect(next.machines.hosted.core[`list-100k`]).toEqual({ firstPaintMs: 200, renderedItems: 31 })
    expect(budgetsFor(fragment({ "list-100k": { firstPaintMs: 80 } }), hosted).machines.m.core[`list-100k`]).toEqual({ firstPaintMs: 120 })
  })

  it(`several runs: every timing at its worst, the counts of the last run`, () => {
    const a = fragment({ "list-100k": { firstPaintMs: 80, scrollStepMs: 6, renderedItems: 31 } })
    const b = fragment({ "list-100k": { firstPaintMs: 95, scrollStepMs: 4, renderedItems: 30 } })
    expect(worstOf([a, b]).scenarios[`list-100k`]).toEqual({ firstPaintMs: 95, scrollStepMs: 6, renderedItems: 30 })
    expect(worstOf([b]).scenarios).toEqual(b.scenarios)
  })

  it(`a warn machine: a timing over budget warns, a count over budget still fails`, () => {
    const shared: Budgets = { ...budgets, machineTimings: { m: `warn` } }
    const v = check(fragment({ "list-100k": { firstPaintMs: 300, scrollStepMs: 4, renderedItems: 31 } }), shared)
    expect(v.map((x) => [x.metric, x.status, x.warnOnly ?? false])).toEqual([
      [`firstPaintMs`, `worse`, true],
      [`scrollStepMs`, `ok`, true],
      [`renderedItems`, `worse`, false],
    ])
    expect(failing(v).map((x) => x.metric)).toEqual([`renderedItems`])
    expect(line(v[0])).toStartWith(`::warning`)
    expect(line(v[2])).toStartWith(`::error`)
    // Every other machine only warns on its timings too.
    expect(failing(check({ ...fragment({ "list-100k": { firstPaintMs: 300 } }), machine: `other` }, { ...shared, machines: { other: budgets.machines.m } })).map((x) => x.metric)).toEqual([])
  })

  it(`a warn renderer: its timing over budget warns on every machine, its count fails, other renderers warn too`, () => {
    const emu: Budgets = { ...budgets, rendererTimings: { "compose-android": `warn` }, machines: { m: { core: budgets.machines.m.core, "compose-android": budgets.machines.m.core }, n: { "compose-android": budgets.machines.m.core } } }
    for (const machine of [`m`, `n`]) {
      const v = check({ ...fragment({ "list-100k": { firstPaintMs: 300, renderedItems: 31 } }), renderer: `compose-android`, machine }, emu)
      expect(v.map((x) => [x.metric, x.status, x.warnOnly ?? false])).toEqual([
        [`firstPaintMs`, `worse`, true],
        [`renderedItems`, `worse`, false],
      ])
      expect(failing(v).map((x) => x.metric)).toEqual([`renderedItems`])
    }
    expect(failing(check(fragment({ "list-100k": { firstPaintMs: 300 } }), emu)).map((x) => x.metric)).toEqual([])
    expect(timingMode(emu, `m`, `compose-android`)).toBe(`warn`)
    expect(timingMode({ ...emu, machineTimings: { m: `warn` } }, `m`, `core`)).toBe(`warn`)
    expect(timingMode(emu, `m`, `core`)).toBe(`warn`)
  })
})
