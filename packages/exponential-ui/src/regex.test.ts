// VAPP-289 (round 9 contract §9.3): fixtures/regex.json replayed through
// regexCheck, plus a differential check against the platform engine
// (`new RegExp(p, "u")`, search semantics) on every accepted case. The
// platform engine appears ONLY here, as the oracle; src/regex.ts never uses it.

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/regex.json" with { type: "json" }
import limitsJson from "../catalog/limits.json" with { type: "json" }
import { regexCheck, regexStepCount } from "./regex"

type Case = { name: string; pattern: unknown; value?: unknown; repeat?: { pattern?: number; value?: number }; expected: { valid: boolean } | { error: string }; steps?: number }
const cases = fixture.cases as Case[]
const inputs = (c: Case): [unknown, unknown] => [
  c.repeat?.value !== undefined ? (c.value as string).repeat(c.repeat.value) : c.value,
  c.repeat?.pattern !== undefined ? (c.pattern as string).repeat(c.repeat.pattern) : c.pattern,
]

describe(`regex (fixtures/regex.json)`, () => {
  test(`the fixture's limits are limits.json's`, () => {
    expect(fixture.limits).toEqual({ maxPatternCodePoints: limitsJson.regex.maxPatternCodePoints, maxValueCodePoints: limitsJson.regex.maxValueCodePoints, maxSteps: limitsJson.regex.maxSteps })
  })

  for (const c of cases) {
    test(c.name, () => {
      const [value, pattern] = inputs(c)
      const r = regexCheck(value, pattern)
      expect(`error` in r ? { error: r.error } : r.result).toEqual(c.expected)
      if (c.steps !== undefined) expect(regexStepCount(value, pattern)).toEqual({ valid: (c.expected as { valid: boolean }).valid, steps: c.steps })
    })
  }

  test(`every reason in limits.json issues is locked by a case, and every case's reason is listed`, () => {
    const reasons = limitsJson.issues.regexReasons
    const used = new Set(cases.flatMap((c) => (`error` in c.expected ? [c.expected.error] : [])).map((m) => /^regex: (.*) at \d+$/.exec(m)?.[1]).filter((r): r is string => r !== undefined))
    expect([...used].sort()).toEqual([...reasons].sort())
    for (const rule of [2, 3, 4]) expect(cases.some((c) => `error` in c.expected && c.expected.error === `regex: rule ${rule}`)).toBe(true)
    expect(cases.some((c) => `error` in c.expected && c.expected.error === limitsJson.issues.regexTooExpensive)).toBe(true)
    expect(cases.some((c) => `error` in c.expected && c.expected.error === limitsJson.issues.regexPatternLength)).toBe(true)
    expect(fixture.$comment).toContain(`REASONS`)
    for (const r of reasons) expect(fixture.$comment).toContain(`\`${r}\``)
  })
})

describe(`regex vs the platform engine (u flag, search)`, () => {
  const text = (v: unknown): string | null => (v === null || v === undefined ? `` : typeof v === `string` ? v : typeof v === `number` || typeof v === `boolean` ? String(v) : null)
  const accepted = cases.filter((c) => `valid` in c.expected)
  for (const c of accepted) {
    const [value, pattern] = inputs(c)
    const v = text(value)
    if (typeof pattern !== `string` || v === null || [...v].length > limitsJson.regex.maxValueCodePoints) continue
    test(c.name, () => {
      expect(new RegExp(pattern, `u`).test(v)).toBe((c.expected as { valid: boolean }).valid)
    })
  }

  test(`every pattern the platform refuses is refused here too`, () => {
    for (const c of cases) {
      const [, pattern] = inputs(c)
      if (typeof pattern !== `string`) continue
      let platformOk = true
      try {
        new RegExp(pattern, `u`)
      } catch {
        platformOk = false
      }
      if (!platformOk) expect(`error` in c.expected).toBe(true)
    }
  })

  test(`random patterns agree with the platform (seeded)`, () => {
    const toks = [`a`, `b`, `.`, `\\d`, `\\w`, `\\s`, `[ab]`, `[^a]`, `[a-c]`, `(`, `)`, `(?:`, `(?=`, `(?!`, `|`, `*`, `+`, `?`, `{2}`, `{1,3}`, `{0,}`, `^`, `$`, `\\b`, `\\B`, `*?`, ` `, `1`, `é`, `😀`]
    const chars = [...`ab1 _é😀`]
    let seed = 7
    const rnd = (n: number) => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff
      return seed % n
    }
    let compared = 0
    for (let t = 0; t < 20000; t++) {
      let p = ``
      for (let k = 1 + rnd(7); k > 0; k--) p += toks[rnd(toks.length)]
      let v = ``
      for (let k = rnd(12); k > 0; k--) v += chars[rnd(chars.length)]
      let platform: boolean | null
      try {
        platform = new RegExp(p, `u`).test(v)
      } catch {
        platform = null
      }
      const ours = regexCheck(v, p)
      if (`error` in ours) {
        // A safety refusal is ours alone; a syntax refusal must be the platform's too.
        if (!ours.error.startsWith(`regex: rule`) && ours.error !== limitsJson.issues.regexTooExpensive) expect({ p, platform }).toEqual({ p, platform: null })
        continue
      }
      expect({ p, v, valid: ours.result.valid }).toEqual({ p, v, valid: platform as boolean })
      compared++
    }
    expect(compared).toBeGreaterThan(5000)
  })
})
