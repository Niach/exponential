import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/pr-body.json"
import { PR_BODY_MAX, prBodyFromGuide, prBodyFromStories, renderPrBody } from "./pr-body"
import { PR_BODY_MAX as PR_UPDATE_BODY_MAX } from "./trpc/pr-update"

// EXP-1154: the PR body = a text projection of the run's report, locked by
// the shared fixture.
describe(`renderPrBody`, () => {
  for (const c of fixture.cases as Array<{
    name: string
    raw: unknown
    resultsUrl: string | null
    max?: number
    prUrl?: string | null
    sessionIds?: string[]
    expected: string | null
  }>) {
    it(c.name, () => {
      const body = renderPrBody(c.raw, {
        resultsUrl: c.resultsUrl,
        ...(c.max ? { max: c.max } : {}),
        ...(`prUrl` in c ? { prUrl: c.prUrl } : {}),
        ...(c.sessionIds ? { sessionIds: c.sessionIds } : {}),
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
    const body = renderPrBody(raw, { resultsUrl: `https://e.x/r` })
    expect(body!.length).toBeLessThanOrEqual(PR_BODY_MAX)
    expect(body!.endsWith(`(https://e.x/r)`)).toBe(true)
  })
})

// EXP-1291: the thread's Guide → one PR's body.
describe(`prBodyFromGuide`, () => {
  const entry = (topic: string, text: string, prUrl?: string) => ({
    topic,
    label: null,
    attachmentId: null,
    width: null,
    height: null,
    text,
    ...(prUrl ? { prUrl } : {}),
  })

  it(`keeps the PR's own topics plus the untagged ones`, () => {
    const guide = [
      entry(`Summary`, `Did it.`),
      entry(`a`, `For PR 1.`, `https://github.com/o/r/pull/1`),
      entry(`b`, `For PR 2.`, `https://github.com/o/r/pull/2`),
    ]
    expect(
      prBodyFromGuide(guide, {
        prUrl: `https://github.com/o/r/pull/2`,
        guideUrl: `https://e.x/g`,
      })
    ).toBe(
      `Did it.\n\n### b\n\nFor PR 2.\n\n---\n\n[Guide and screenshots in Exponential](https://e.x/g)`
    )
  })

  it(`a PR not opened yet takes the untagged topics only`, () => {
    const guide = [entry(`a`, `Tagged.`, `https://github.com/o/r/pull/1`), entry(`b`, `Free.`)]
    expect(prBodyFromGuide(guide, { prUrl: null })).toBe(`### b\n\nFree.`)
  })

  it(`no Guide, no body`, () => {
    expect(prBodyFromGuide(null)).toBeNull()
  })
})

describe(`prBodyFromStories`, () => {
  for (const c of fixture.storyCases) {
    it(c.name, () => {
      expect(prBodyFromStories(c.input)).toBe(c.expected)
    })
  }
})
