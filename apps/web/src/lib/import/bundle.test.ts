import { describe, expect, it } from "vitest"
import { ISSUE_ESTIMATE_MAX } from "@exp/db-schema/domain"
import { importBundleSchema, type BundleIssue } from "@/lib/import/bundle"
import { bundleFixture } from "@/lib/import/fixtures"
import { IMPORT_MAX_BUNDLE_COMMENTS, IMPORT_MAX_BUNDLE_ISSUES } from "@/lib/import/limits"
import { previewFromBundle } from "@/lib/import/preview"

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

// EXP-1076: the relevance maps the wizard narrows its lists by.
describe(`previewFromBundle per-board maps`, () => {
  const preview = previewFromBundle(bundleFixture())
  const sum = (counts: Record<string, number> | undefined) =>
    Object.values(counts ?? {}).reduce((total, count) => total + count, 0)

  it(`keys every status and label map by board key and sums to the flat count`, () => {
    for (const status of preview.statuses) {
      expect(Object.keys(status.issueCountByBoard ?? {})).toEqual(
        status.issueCount > 0 ? [`b-main`] : []
      )
      expect(sum(status.issueCountByBoard)).toBe(status.issueCount)
    }
    for (const label of preview.labels) {
      expect(sum(label.issueCountByBoard)).toBe(label.issueCount)
    }
  })

  it(`counts a user's issues by assignee OR creator, comments by their board`, () => {
    const hannes = preview.users.find((user) => user.key === `u-h`)!
    // Assignee of MAIN-10 only — the displayed count stays assignee-only.
    expect(hannes.issueCount).toBe(1)
    expect(hannes.issueCountByBoard).toEqual({ "b-main": 1 })
    expect(hannes.commentCountByBoard).toEqual({ "b-main": 1 })
    const stranger = preview.users.find((user) => user.key === `u-x`)!
    // Creator of MAIN-10, assigned to nothing: the relevance map still has it.
    expect(stranger.issueCount).toBe(0)
    expect(stranger.issueCountByBoard).toEqual({ "b-main": 1 })
    expect(sum(stranger.commentCountByBoard)).toBe(stranger.commentCount)
  })
})
