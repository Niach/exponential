import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/expressions.json" with { type: "json" }
import catalog from "../catalog/expressions.json" with { type: "json" }
import host from "../catalog/host.json" with { type: "json" }
import core from "../catalog/core.catalog.json" with { type: "json" }
import {
  EXPRESSION_CALL_NAMES,
  EXPRESSION_COMPREHENSIONS,
  EXPRESSION_FUNCTIONS,
  EXPRESSION_LIMITS,
  EXPRESSION_METHODS,
  EXPRESSION_RESERVED,
  EXPRESSION_ROW_KEYS,
  callExpressionFunction,
  checkExpression,
  dayFromNumber,
  dayNumber,
  evalExpression,
  type CheckKind,
  type ExpressionClock,
} from "./expressions"
import { fixedOffset } from "./format"
import { BIND_FUNCTION_NAMES, resolveDynamic } from "./dynamic"
import { RECORDS_ROW_KEYS } from "./host/query"
import { formatDay } from "./dates"

// VAPP-191: the expression language against fixtures/expressions.json (the
// Rust core, the Swift and the Compose SDK replay the same cases).

type Clock = { now: string; offsetMinutes: number }
const clockOf = (c: Clock): ExpressionClock => ({ now: Date.parse(c.now), zone: fixedOffset(c.offsetMinutes) })
type EvalCase = { name: string; source: string; vars?: Record<string, unknown>; clock?: Clock; expected: unknown }

describe(`expressions (VAPP-191)`, () => {
  test(`the tables are the catalog's`, () => {
    expect({ limits: EXPRESSION_LIMITS, functions: EXPRESSION_FUNCTIONS, methods: EXPRESSION_METHODS, comprehensions: EXPRESSION_COMPREHENSIONS, reserved: EXPRESSION_RESERVED, rowKeys: EXPRESSION_ROW_KEYS } as unknown).toEqual({
      limits: fixture.limits,
      functions: fixture.functions,
      methods: fixture.methods,
      comprehensions: fixture.comprehensions,
      reserved: fixture.reserved,
      rowKeys: fixture.rowKeys,
    })
    expect(fixture.limits).toEqual(catalog.limits)
    expect([...EXPRESSION_ROW_KEYS]).toEqual([...RECORDS_ROW_KEYS])
    for (const key of host.package.records.reserved) expect(EXPRESSION_ROW_KEYS).toContain(key)
    for (const name of EXPRESSION_CALL_NAMES) {
      expect(BIND_FUNCTION_NAMES).toContain(name)
      expect(core.functions.names).toContain(name)
      expect(Object.keys((core.functions.core as Record<string, { args: Record<string, string> }>)[name]!.args)).toEqual(name === `expr` ? [`source`, `vars`, `types`] : EXPRESSION_FUNCTIONS[name]!.call!)
    }
  })
  test(`day numbers round-trip`, () => {
    for (const n of [-719468, -1, 0, 1, 20_759, 2_932_896]) expect(dayNumber(dayFromNumber(n))).toBe(n)
    expect(formatDay(dayFromNumber(0))).toBe(`1970-01-01`)
    expect(dayNumber({ y: 2026, m: 11, d: 2 })).toBe(20_759)
  })
  const run = (c: EvalCase) => expect(evalExpression(c.source, c.vars ?? fixture.vars, clockOf(c.clock ?? fixture.clock))).toEqual(c.expected as never)
  test.each(fixture.evalCases as EvalCase[])(`eval: $name`, run)
  test.each(fixture.costCases as EvalCase[])(`cost: $name`, run)
  test.each(fixture.checkCases)(`check: $name`, (c) => {
    expect(checkExpression(c.source, c.types as Record<string, string>, fixture.kinds as Record<string, CheckKind>)).toEqual(c.expected as never)
  })
  test.each(fixture.callCases as { name: string; call: string; args: Record<string, unknown>; clock?: Clock; expected: { value?: unknown } }[])(`call: $name`, (c) => {
    const v = callExpressionFunction(c.call, c.args, clockOf(c.clock ?? fixture.clock))
    expect(v === undefined ? {} : { value: v }).toEqual(c.expected as never)
  })
  test(`the bind pass runs the call form on the surface clock and zone`, () => {
    const now = () => Date.parse(fixture.clock.now)
    const call = { call: `expr`, args: { source: `dateAdd(today(), n, 'day')`, vars: { n: { path: `/n` } } } }
    expect(resolveDynamic(call, { n: 2 }, { now, zone: fixedOffset(60) })).toBe(`2026-11-05`)
    expect(resolveDynamic(call, { n: 2 }, { now })).toBe(`2026-11-04`)
    expect(resolveDynamic({ call: `mul`, args: { a: { path: `/h` }, b: 60 } }, { h: `7.5` })).toBe(450)
    expect(resolveDynamic({ call: `formatDuration`, args: { hours: { path: `/h` } } }, { h: 7.25 })).toBe(`7:15`)
  })
})
