import { beforeEach, describe, expect, it, vi } from "vitest"
import { issueRelationTypeValues } from "@exp/db-schema/domain"

const h = vi.hoisted(() => ({
  recordIssueEvent: vi.fn(async (..._args: unknown[]) => undefined),
  resolveIssueRefs: vi.fn(
    async (..._args: unknown[]) => [] as Array<{ id: string; identifier: string }>
  ),
}))

// Both are thin writers over the same tx; stubbing them keeps this file off
// their db/connection import chain and turns the activity log into an
// assertion surface (a relation write records an event on BOTH issues, or on
// neither).
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: h.recordIssueEvent,
}))
vi.mock(`@/lib/integrations/mentions`, () => ({
  resolveIssueRefs: h.resolveIssueRefs,
}))

import { comments, issueRelations, issues } from "@/db/schema"
import {
  canonicalizeRelation,
  insertRelationInTx,
  relationEventParts,
  relationEventPhrase,
  relationLabel,
  syncDuplicateMirror,
  syncReferenceRelations,
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

// ---------------------------------------------------------------------------
// The writer half. Fake-tx harness in the style of lib/trpc/relations.test.ts:
// selects are routed by TABLE (and, for `issues`, by the projected fields —
// the identifier lookup and the stored-description read hit the same table),
// while insert/delete hand back seeded rows in FIFO order.

const REL_ID = `44444444-4444-4444-8444-444444444444`
const TEAM = `ws-1`

type Row = Record<string, unknown>

const state = {
  /** Rows the pair lookup in insertRelationInTx sees. */
  relationRows: [] as Row[],
  /** `{id, identifier}` rows: the event lookup AND the orphan resolution. */
  issueRows: [] as Row[],
  /** The stored description the survivor scan reads (comment-slot calls). */
  descriptionRows: [] as Row[],
  /** Comment bodies the survivor scan reads. */
  commentRows: [] as Row[],
  insertResults: [] as Row[][],
  deleteResults: [] as Row[][],
  inserted: [] as Row[],
  updated: [] as Row[],
  deletes: 0,
}

function rowsFor(fields: Record<string, unknown>, table: unknown): Row[] {
  if (table === issueRelations) return state.relationRows
  if (table === comments) return state.commentRows
  if (table === issues) {
    return `description` in fields ? state.descriptionRows : state.issueRows
  }
  return []
}

function awaitable(rows: Row[]): Promise<Row[]> & Record<string, unknown> {
  const p = Promise.resolve(rows) as Promise<Row[]> & Record<string, unknown>
  for (const m of [`where`, `limit`, `innerJoin`, `orderBy`, `for`]) {
    p[m] = () => p
  }
  return p
}

const fakeTx = {
  select: (fields: Record<string, unknown> = {}) => ({
    from: (table: unknown) => awaitable(rowsFor(fields, table)),
  }),
  insert: (_table: unknown) => ({
    values: (values: Row) => {
      state.inserted.push(values)
      const returning = async () => state.insertResults.shift() ?? []
      return { onConflictDoNothing: () => ({ returning }), returning }
    },
  }),
  update: (_table: unknown) => ({
    set: (values: Row) => {
      state.updated.push(values)
      return { where: async () => undefined }
    },
  }),
  delete: (_table: unknown) => ({
    where: () => {
      state.deletes += 1
      return { returning: async () => state.deleteResults.shift() ?? [] }
    },
  }),
} as never

const relationRow = (over: Row = {}): Row => ({
  id: REL_ID,
  issueId: A,
  relatedIssueId: B,
  type: `related`,
  source: `reference`,
  teamId: TEAM,
  ...over,
})

beforeEach(() => {
  state.relationRows = []
  state.issueRows = [
    { id: A, identifier: `EXP-1` },
    { id: B, identifier: `EXP-2` },
  ]
  state.descriptionRows = [{ description: `` }]
  state.commentRows = []
  state.insertResults = []
  state.deleteResults = []
  state.inserted = []
  state.updated = []
  state.deletes = 0
  h.recordIssueEvent.mockClear()
  h.resolveIssueRefs.mockClear()
  h.resolveIssueRefs.mockResolvedValue([])
})

describe(`insertRelationInTx`, () => {
  it(`records one event per issue on a real insert`, async () => {
    state.insertResults = [[relationRow({ source: `user` })]]

    const inserted = await insertRelationInTx(fakeTx, {
      issueId: A,
      relatedIssueId: B,
      type: `related`,
      source: `user`,
      teamId: TEAM,
      actorUserId: `actor`,
    })

    expect(inserted).not.toBeNull()
    // One row PER issue, each naming the OTHER one from its own side.
    expect(h.recordIssueEvent).toHaveBeenCalledTimes(2)
    expect(h.recordIssueEvent.mock.calls[0]![1]).toMatchObject({
      issueId: A,
      type: `relation_added`,
      payload: expect.objectContaining({
        relatedIdentifier: `EXP-2`,
        direction: `forward`,
      }),
    })
    expect(h.recordIssueEvent.mock.calls[1]![1]).toMatchObject({
      issueId: B,
      payload: expect.objectContaining({
        relatedIdentifier: `EXP-1`,
        direction: `inverse`,
      }),
    })
  })

  it(`is a silent no-op when a reference row already stands`, async () => {
    // The `#IDENT` re-scan hits this on every save of an unchanged body: it
    // must not re-insert, re-event or touch the row's source.
    state.relationRows = [{ id: REL_ID, source: `reference` }]

    const result = await insertRelationInTx(fakeTx, {
      issueId: A,
      relatedIssueId: B,
      type: `related`,
      source: `reference`,
      teamId: TEAM,
      actorUserId: `actor`,
    })

    expect(result).toBeNull()
    expect(state.inserted).toHaveLength(0)
    expect(state.updated).toHaveLength(0)
    expect(h.recordIssueEvent).not.toHaveBeenCalled()
  })

  it(`upgrades an auto-derived row to a manual one, without an event`, async () => {
    // Picking "Related to" on a pair the description already references makes
    // the link survive the `#IDENT` token being edited out. It is the SAME
    // link, so the timeline gets nothing.
    state.relationRows = [{ id: REL_ID, source: `reference` }]

    const result = await insertRelationInTx(fakeTx, {
      issueId: A,
      relatedIssueId: B,
      type: `related`,
      source: `user`,
      teamId: TEAM,
      actorUserId: `actor`,
    })

    expect(result).toBeNull()
    expect(state.updated).toEqual([{ source: `user` }])
    expect(h.recordIssueEvent).not.toHaveBeenCalled()
  })

  it(`never downgrades a manual row to a reference`, async () => {
    state.relationRows = [{ id: REL_ID, source: `user` }]

    await insertRelationInTx(fakeTx, {
      issueId: A,
      relatedIssueId: B,
      type: `related`,
      source: `reference`,
      teamId: TEAM,
      actorUserId: `actor`,
    })

    expect(state.updated).toHaveLength(0)
  })
})

describe(`syncDuplicateMirror`, () => {
  // The repair path: a duplicate link written before EXP-736 (or one whose
  // mirror was lost) has previous === next on every subsequent write, so a
  // delta-driven mirror could never be healed.
  it(`writes the missing mirror even when the column did not change`, async () => {
    state.insertResults = [[relationRow({ type: `duplicate`, source: `user` })]]

    await syncDuplicateMirror(fakeTx, {
      issueId: A,
      teamId: TEAM,
      actorUserId: `actor`,
      previousDuplicateOfId: B,
      nextDuplicateOfId: B,
    })

    expect(state.inserted).toHaveLength(1)
    expect(state.inserted[0]).toMatchObject({
      issueId: A,
      relatedIssueId: B,
      type: `duplicate`,
      source: `user`,
    })
  })

  it(`leaves a mirror that already stands alone`, async () => {
    state.relationRows = [{ id: REL_ID, source: `user` }]

    await syncDuplicateMirror(fakeTx, {
      issueId: A,
      teamId: TEAM,
      actorUserId: `actor`,
      previousDuplicateOfId: B,
      nextDuplicateOfId: B,
    })

    expect(state.inserted).toHaveLength(0)
    expect(h.recordIssueEvent).not.toHaveBeenCalled()
  })

  it(`drops every duplicate row when the link is cleared`, async () => {
    state.deleteResults = [[relationRow({ type: `duplicate`, source: `user` })]]

    await syncDuplicateMirror(fakeTx, {
      issueId: A,
      teamId: TEAM,
      actorUserId: `actor`,
      previousDuplicateOfId: B,
      nextDuplicateOfId: null,
    })

    expect(state.deletes).toBe(1)
    expect(state.inserted).toHaveLength(0)
    expect(h.recordIssueEvent).toHaveBeenCalledTimes(2)
    expect(h.recordIssueEvent.mock.calls[0]![1]).toMatchObject({
      type: `relation_removed`,
    })
  })
})

describe(`syncReferenceRelations`, () => {
  it(`keeps the row while the identifier survives in another comment`, async () => {
    state.commentRows = [{ body: `still discussing #EXP-2` }]

    await syncReferenceRelations(fakeTx, {
      issueId: A,
      teamId: TEAM,
      actorUserId: `actor`,
      previousText: `see #EXP-2`,
      nextText: ``,
      excludeCommentId: `comment-1`,
    })

    expect(state.deletes).toBe(0)
    expect(h.recordIssueEvent).not.toHaveBeenCalled()
  })

  it(`keeps the row while the identifier survives in the description`, async () => {
    // A comment edit: the survivor scan reads the stored description plus
    // every OTHER comment.
    state.descriptionRows = [{ description: `context: #EXP-2` }]

    await syncReferenceRelations(fakeTx, {
      issueId: A,
      teamId: TEAM,
      actorUserId: `actor`,
      previousText: `see #EXP-2`,
      nextText: `never mind`,
      excludeCommentId: `comment-1`,
    })

    expect(state.deletes).toBe(0)
  })

  it(`deletes the row once the last mention is gone`, async () => {
    state.issueRows = [{ id: B, identifier: `EXP-2` }]
    state.deleteResults = [[relationRow()]]

    await syncReferenceRelations(fakeTx, {
      issueId: A,
      teamId: TEAM,
      actorUserId: `actor`,
      previousText: `see #EXP-2`,
      nextText: ``,
      excludeCommentId: `comment-1`,
    })

    expect(state.deletes).toBe(1)
    expect(h.recordIssueEvent).toHaveBeenCalledTimes(2)
    expect(h.recordIssueEvent.mock.calls[0]![1]).toMatchObject({
      type: `relation_removed`,
    })
  })

  it(`writes a reference row for an identifier the edit added`, async () => {
    h.resolveIssueRefs.mockResolvedValue([{ id: B, identifier: `EXP-2` }])
    state.insertResults = [[relationRow()]]

    await syncReferenceRelations(fakeTx, {
      issueId: A,
      teamId: TEAM,
      actorUserId: `actor`,
      previousText: ``,
      nextText: `blocked on #EXP-2`,
    })

    expect(state.inserted).toHaveLength(1)
    expect(state.inserted[0]).toMatchObject({
      issueId: A,
      relatedIssueId: B,
      type: `related`,
      source: `reference`,
    })
  })

  it(`does nothing at all when the token set is unchanged`, async () => {
    await syncReferenceRelations(fakeTx, {
      issueId: A,
      teamId: TEAM,
      actorUserId: `actor`,
      previousText: `see #EXP-2`,
      nextText: `see #EXP-2 (still)`,
    })

    expect(h.resolveIssueRefs).not.toHaveBeenCalled()
    expect(state.inserted).toHaveLength(0)
    expect(state.deletes).toBe(0)
  })
})
