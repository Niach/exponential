import { describe, expect, it } from "vitest"
import { issueRelationTypeValues } from "@exp/db-schema/domain"
import {
  canonicalizeRelation,
  relationEventParts,
  relationEventPhrase,
  relationLabel,
} from "@/lib/issue-relations"

// EXP-736 — the pure half of the relation module. Canonicalization is what
// makes UNIQUE(issue_id, related_issue_id, type) actually dedupe (a pick made
// from either side has to land on the SAME row), and the phrase table is a
// CROSS-CLIENT contract: desktop timeline.rs, iOS EventPhrases and Android
// EventRow all reproduce these exact strings.

const A = `11111111-1111-4111-8111-111111111111`
const B = `22222222-2222-4222-8222-222222222222`

describe(`canonicalizeRelation`, () => {
  it(`keeps the forward pick as written`, () => {
    expect(canonicalizeRelation(A, B, `blocks`)).toEqual({
      issueId: A,
      relatedIssueId: B,
      type: `blocks`,
    })
    expect(canonicalizeRelation(A, B, `parent`)).toEqual({
      issueId: A,
      relatedIssueId: B,
      type: `parent`,
    })
  })

  it(`swaps the pair for an inverse pick`, () => {
    // "Blocked by B" IS "B blocks this issue".
    expect(canonicalizeRelation(A, B, `blocks`, true)).toEqual({
      issueId: B,
      relatedIssueId: A,
      type: `blocks`,
    })
    // "Sub-issue of B" IS "B is the parent of this issue".
    expect(canonicalizeRelation(A, B, `parent`, true)).toEqual({
      issueId: B,
      relatedIssueId: A,
      type: `parent`,
    })
  })

  it(`orders the symmetric type by id, from either side and either flag`, () => {
    const expected = { issueId: A, relatedIssueId: B, type: `related` as const }
    expect(canonicalizeRelation(A, B, `related`)).toEqual(expected)
    expect(canonicalizeRelation(B, A, `related`)).toEqual(expected)
    expect(canonicalizeRelation(A, B, `related`, true)).toEqual(expected)
    expect(canonicalizeRelation(B, A, `related`, true)).toEqual(expected)
  })
})

describe(`relationLabel`, () => {
  it(`covers every type on both sides`, () => {
    for (const type of issueRelationTypeValues) {
      expect(relationLabel(type, `forward`).length).toBeGreaterThan(0)
      expect(relationLabel(type, `inverse`).length).toBeGreaterThan(0)
    }
  })

  it(`names the two halves of each directed type`, () => {
    expect(relationLabel(`blocks`, `forward`)).toBe(`blocks`)
    expect(relationLabel(`blocks`, `inverse`)).toBe(`blocked by`)
    expect(relationLabel(`parent`, `forward`)).toBe(`parent of`)
    expect(relationLabel(`parent`, `inverse`)).toBe(`sub-issue of`)
    expect(relationLabel(`duplicate`, `forward`)).toBe(`duplicate of`)
    expect(relationLabel(`duplicate`, `inverse`)).toBe(`duplicated by`)
    // Symmetric: both sides read the same.
    expect(relationLabel(`related`, `forward`)).toBe(`related to`)
    expect(relationLabel(`related`, `inverse`)).toBe(`related to`)
  })
})

describe(`relationEventPhrase (the cross-client phrase table)`, () => {
  const payload = (over: Record<string, unknown> = {}) => ({
    type: `blocks`,
    relatedIssueId: B,
    relatedIdentifier: `EXP-3`,
    direction: `forward`,
    source: `user`,
    ...over,
  })

  it(`reads a symmetric link as an addition or a removal`, () => {
    expect(
      relationEventPhrase(`relation_added`, payload({ type: `related` }))
    ).toBe(`added related issue EXP-3`)
    expect(
      relationEventPhrase(`relation_removed`, payload({ type: `related` }))
    ).toBe(`removed related issue EXP-3`)
  })

  it(`reads a directed link as a state, per side`, () => {
    expect(relationEventPhrase(`relation_added`, payload())).toBe(
      `marked as blocks EXP-3`
    )
    expect(
      relationEventPhrase(`relation_added`, payload({ direction: `inverse` }))
    ).toBe(`marked as blocked by EXP-3`)
    expect(
      relationEventPhrase(`relation_removed`, payload({ type: `parent` }))
    ).toBe(`no longer parent of EXP-3`)
    expect(
      relationEventPhrase(
        `relation_removed`,
        payload({ type: `parent`, direction: `inverse` })
      )
    ).toBe(`no longer sub-issue of EXP-3`)
  })

  it(`degrades to a generic phrase for an unreadable payload`, () => {
    // Old rows, or an issue that has since been hard-deleted.
    expect(relationEventPhrase(`relation_added`, {})).toBe(
      `added related issue an issue`
    )
    expect(
      relationEventParts(`relation_added`, payload({ relatedIdentifier: `` }))
    ).toEqual({ prefix: `marked as blocks`, identifier: null })
  })

  it(`hands the identifier back separately so clients can link it`, () => {
    expect(relationEventParts(`relation_added`, payload())).toEqual({
      prefix: `marked as blocks`,
      identifier: `EXP-3`,
    })
  })
})
