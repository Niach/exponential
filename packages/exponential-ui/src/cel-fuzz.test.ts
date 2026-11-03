// The CEL differential fuzz (src/cel-fuzz.ts): the committed corpus is what
// the TS reference answers today (the Rust core replays the same file in
// eui-core tests/cel_fuzz.rs), and no case escapes the caps.

import { describe, expect, test } from "bun:test"
import { EXPRESSION_LIMITS, evaluateCounted, parseExpression, parseTimestamp } from "./cel"
import { CORPUS_COUNT, CORPUS_SEED, generateCorpus, zoneOffset, type FuzzCorpus } from "./cel-fuzz"
import committed from "../fixtures/cel-fuzz.json" with { type: "json" }

describe(`cel-fuzz.json`, () => {
  const corpus = generateCorpus(CORPUS_SEED, CORPUS_COUNT)

  test(`the committed corpus is the generator's (bun run ./scripts/cel-fuzz.ts --write)`, () => {
    expect(JSON.parse(JSON.stringify(corpus))).toEqual(committed as unknown as FuzzCorpus)
  })

  test(`every outcome kind is exercised`, () => {
    const errors = corpus.cases.filter((c) => `error` in c.eval).map((c) => (c.eval as { error: string }).error)
    expect(corpus.cases.length - errors.length).toBeGreaterThan(CORPUS_COUNT / 3)
    for (const e of [`step limit exceeded`, `date out of range`, `expression is longer than 1000 characters`]) expect(errors, e).toContain(e)
    expect(errors.some((e) => e.endsWith(`expression is nested too deeply`))).toBe(true)
    expect(errors.some((e) => /^at \d+: unexpected/.test(e))).toBe(true)
    expect(corpus.calls.some((c) => `none` in c.result)).toBe(true)
    expect(corpus.calls.some((c) => `outside` in c.result)).toBe(true)
  })

  test(`no case escapes the caps`, () => {
    for (const c of corpus.cases) {
      const parsed = parseExpression(c.expr)
      if (`error` in parsed) continue
      expect(Array.from(c.expr).length, c.expr).toBeLessThanOrEqual(EXPRESSION_LIMITS.maxLength)
      const max = c.maxSteps ?? EXPRESSION_LIMITS.maxSteps
      const { steps } = evaluateCounted(parsed.ast, corpus.varSets[c.set]!.vars, { now: parseTimestamp(c.now)!, offset: zoneOffset(corpus.zones[c.zone]!), maxSteps: max })
      expect(steps, c.expr).toBeLessThanOrEqual(max + 1)
    }
  })
})
