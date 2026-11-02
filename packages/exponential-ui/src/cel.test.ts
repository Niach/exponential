// VAPP-191: fixtures/expressions.json replayed through the TS reference
// (the Rust core, the FFI facade and the Swift / Compose SDKs replay the
// same file).

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/expressions.json"
import { EXPRESSION_CALL_NAMES, EXPRESSION_FUNCTIONS, EXPRESSION_LIMITS, checkExpression, evaluateExpression, parseTimestamp } from "./cel"
import type { ExprType } from "./cel"
import { BIND_FUNCTIONS, resolveDynamic } from "./dynamic"
import { intlZoneOffset } from "./format"
import { validatePackage } from "./host/package"
import coreCatalog from "../catalog/core.catalog.json"

type Zone = { offset: number; transitions: [string, number][]; name?: string }

/** A fixture zone: `offset` until the first transition, then each one's. */
export function zoneOffset(zone: Zone): (ms: number) => number {
  const steps = zone.transitions.map(([at, offset]) => [parseTimestamp(at)!, offset] as const)
  return (ms) => {
    let offset = zone.offset
    for (const [at, o] of steps) if (ms >= at) offset = o
    return offset
  }
}

const zones = fixture.zones as unknown as Record<string, Zone>
const env = (c: { now?: string; zone?: string; maxSteps?: number }) => ({
  now: parseTimestamp(c.now ?? fixture.env.now)!,
  offset: zoneOffset(zones[c.zone ?? fixture.env.zone]!),
  ...(c.maxSteps !== undefined ? { maxSteps: c.maxSteps } : {}),
})

describe(`expressions.json`, () => {
  test(`limits`, () => {
    expect(fixture.limits).toEqual({ ...EXPRESSION_LIMITS })
  })

  test(`the fixture zones are the IANA zones (Intl)`, () => {
    for (const zone of Object.values(zones)) {
      if (!zone.name) continue
      const ours = zoneOffset(zone)
      const intl = intlZoneOffset(zone.name)
      const instants = [`2026-01-15T12:00:00Z`, `2026-07-15T12:00:00Z`, ...zone.transitions.flatMap(([at]) => [at])].map((t) => parseTimestamp(t)!)
      for (const ms of instants.flatMap((ms) => [ms - 1, ms, ms + 1])) expect(ours(ms), `${zone.name} at ${ms}`).toBe(intl(ms))
    }
  })

  for (const c of fixture.eval as { name: string; expr: string; value?: unknown; error?: string; now?: string; zone?: string; maxSteps?: number }[]) {
    test(`eval: ${c.name}`, () => {
      const result = evaluateExpression(c.expr, fixture.vars as Record<string, unknown>, env(c))
      expect(result).toEqual(c.error !== undefined ? { error: c.error } : { value: c.value })
    })
  }

  for (const c of fixture.check as { name: string; expr: string; type?: ExprType; error?: string }[]) {
    test(`check: ${c.name}`, () => {
      expect(checkExpression(c.expr, fixture.checkTypes as Record<string, ExprType>)).toEqual(c.error !== undefined ? { error: c.error } : { type: c.type! })
    })
  }

  for (const c of fixture.calls as { name: string; call: unknown; value?: unknown; none?: boolean; cel?: string }[]) {
    test(`call: ${c.name}`, () => {
      const e = env({})
      const got = resolveDynamic(c.call, fixture.callData, { now: () => e.now, zone: e.offset })
      expect(got).toEqual(c.none ? undefined : c.value)
      if (c.cel !== undefined) {
        const cel = evaluateExpression(c.cel, fixture.callData as Record<string, unknown>, e)
        if (c.none) expect(`error` in cel).toBe(true)
        else expect(cel).toEqual({ value: c.value })
      }
    })
  }

  for (const c of fixture.packages as { name: string; package: unknown; issues: { path: string; message: string }[] }[]) {
    test(`package: ${c.name}`, () => {
      expect(validatePackage(c.package)).toEqual(c.issues)
    })
  }

  test(`every expression function is in the bind table and the catalog`, () => {
    const core = coreCatalog.functions.core as Record<string, unknown>
    for (const name of EXPRESSION_CALL_NAMES) {
      expect(BIND_FUNCTIONS[name], name).toBeDefined()
      expect(core[name], name).toBeDefined()
      expect(coreCatalog.functions.names, name).toContain(name)
    }
    expect([...EXPRESSION_CALL_NAMES]).toEqual([...Object.keys(EXPRESSION_FUNCTIONS).filter((n) => n !== `size`), `expr`])
  })

  test(`a deep list keeps the step limit`, () => {
    const big = Array.from({ length: 5000 }, (_, i) => i)
    expect(evaluateExpression(`xs.map(x, x * 2)`, { xs: big })).toEqual({ error: `step limit exceeded` })
    expect(evaluateExpression(`size(xs.filter(x, x > 2))`, { xs: big.slice(0, 2000) })).toEqual({ value: 1997 })
  })
})
