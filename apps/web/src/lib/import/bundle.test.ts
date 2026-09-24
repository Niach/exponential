import { describe, expect, it } from "vitest"
import { ISSUE_ESTIMATE_MAX } from "@exp/db-schema/domain"
import { importBundleSchema, type BundleIssue } from "@/lib/import/bundle"
import { bundleFixture } from "@/lib/import/fixtures"
import { IMPORT_MAX_BUNDLE_COMMENTS, IMPORT_MAX_BUNDLE_ISSUES } from "@/lib/import/limits"

// EXP-630: the bounds `imports.ingest` enforces on a raw bundle.

function issueLike(index: number, overrides: Partial<BundleIssue> = {}): BundleIssue {
  const base = bundleFixture().issues[0]!
  return { ...base, key: `i-${index}`, externalRef: `MAIN-${index}`, number: index, ...overrides }
}

describe(`importBundleSchema bounds`, () => {
  it(`caps an issue estimate like issueEstimateSchema does`, () => {
    const ok = { ...bundleFixture(), issues: [issueLike(1, { estimate: ISSUE_ESTIMATE_MAX })] }
    expect(importBundleSchema.safeParse(ok).success).toBe(true)
    const over = { ...bundleFixture(), issues: [issueLike(1, { estimate: ISSUE_ESTIMATE_MAX + 1 })] }
    expect(importBundleSchema.safeParse(over).success).toBe(false)
  })

  it(`refuses more issues than IMPORT_MAX_BUNDLE_ISSUES`, () => {
    const issues = Array.from({ length: IMPORT_MAX_BUNDLE_ISSUES + 1 }, (_, index) =>
      issueLike(index + 1, { comments: [], events: [], assets: [] })
    )
    const result = importBundleSchema.safeParse({ ...bundleFixture(), issues })
    expect(result.success).toBe(false)
    expect(result.error?.issues[0]?.path).toEqual([`issues`])
  })

  it(`refuses more comments in total than IMPORT_MAX_BUNDLE_COMMENTS`, () => {
    const comment = bundleFixture().issues[0]!.comments[0]!
    const perIssue = 1_000
    const issues = Array.from({ length: IMPORT_MAX_BUNDLE_COMMENTS / perIssue + 1 }, (_, index) =>
      issueLike(index + 1, {
        events: [],
        assets: [],
        comments: Array.from({ length: perIssue }, (_, c) => ({ ...comment, key: `c-${index}-${c}` })),
      })
    )
    const result = importBundleSchema.safeParse({ ...bundleFixture(), issues })
    expect(result.success).toBe(false)
    expect(result.error?.issues[0]?.message).toMatch(/at most 500000 comments/)
  })
})
