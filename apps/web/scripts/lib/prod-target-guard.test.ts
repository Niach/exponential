import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import fixture from "@exp/emcp/fixtures/deletes.json" with { type: "json" }
import { prodBaseVerdict as reference } from "@exp/emcp/deletes"
// @ts-expect-error -- plain .mjs, no types
import { assertToolTestBase, DEFAULT_BASE, PROD_FLAG, prodBaseVerdict } from "./prod-target-guard.mjs"

// #EXP-1798: the tool-test harnesses refuse prod unless told.
describe(`prod-target-guard`, () => {
  it(`replays deletes.json prodHostCases`, () => {
    for (const entry of fixture.prodHostCases) {
      expect(prodBaseVerdict(entry.base, entry.flag), entry.base).toBe(entry.expected)
      expect(reference(entry.base, entry.flag), entry.base).toBe(entry.expected)
    }
  })

  it(`defaults to the local stack and refuses prod without the flag`, () => {
    expect(assertToolTestBase(undefined, [])).toBe(DEFAULT_BASE)
    expect(assertToolTestBase(`https://next.exponential.at/`, [])).toBe(`https://next.exponential.at`)
    expect(() => assertToolTestBase(`https://app.exponential.at`, [])).toThrow(/Refusing to run a tool test/)
    expect(assertToolTestBase(`https://app.exponential.at`, [PROD_FLAG])).toBe(`https://app.exponential.at`)
    expect(() => assertToolTestBase(`nope`, [])).toThrow(/not a URL/)
  })

  it(`guards the MCP e2e harness`, () => {
    const script = readFileSync(join(__dirname, `../mcp-oauth-e2e.mjs`), `utf8`)
    expect(script).toContain(`assertToolTestBase(`)
  })
})
