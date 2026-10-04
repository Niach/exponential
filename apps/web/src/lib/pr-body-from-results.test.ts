import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/pr-body.json"
import { PR_BODY_MAX, prBodyFromResults } from "./pr-body-from-results"
import { PR_BODY_MAX as PR_UPDATE_BODY_MAX } from "./trpc/pr-update"

// EXP-1154: the PR body = a text projection of the run's report, locked by
// the shared fixture.
describe(`prBodyFromResults`, () => {
  for (const c of fixture.cases as Array<{
    name: string
    raw: unknown
    resultsUrl: string | null
    max?: number
    expected: string | null
  }>) {
    it(c.name, () => {
      const body = prBodyFromResults(c.raw, {
        resultsUrl: c.resultsUrl,
        ...(c.max ? { max: c.max } : {}),
      })
      expect(body).toBe(c.expected)
      if (body && c.max) expect(body.length).toBeLessThanOrEqual(c.max)
    })
  }

  it(`pr_update shares the one body limit`, () => {
    expect(PR_UPDATE_BODY_MAX).toBe(PR_BODY_MAX)
  })

  it(`a body over the default limit still fits it and keeps the footer`, () => {
    const raw = Array.from({ length: 200 }, (_, i) => ({
      topic: `t${i}`,
      text: `${`word `.repeat(80)}\n${`more `.repeat(80)}`,
    }))
    const body = prBodyFromResults(raw, { resultsUrl: `https://e.x/r` })
    expect(body!.length).toBeLessThanOrEqual(PR_BODY_MAX)
    expect(body!.endsWith(`(https://e.x/r)`)).toBe(true)
  })
})
