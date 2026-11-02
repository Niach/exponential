import { describe, expect, it } from "vitest"
import {
  SESSION_RESULTS_REPORT_TOTAL_MAX,
  SESSION_RESULT_FILES_MAX,
  type GuideEntry,
} from "@exp/db-schema/domain"
import {
  canonicalGithubPrUrl,
  guideAttachmentIds,
  guideSummary,
  logicalSessionIds,
  missingGuideFiles,
  nextShowLabel,
  ownedBy,
  ownedByAny,
  ownedBySubtree,
  removeGuideEntries,
  soleHeadSession,
  stampUntaggedGuideEntries,
  upsertGuidePicture,
  upsertGuideText,
} from "@/lib/sessions/guide-entries"

// EXP-1291: the thread Guide's list algebra (ported from the EXP-879/933/
// 1154/1251 results writes): ordering, per-writer ownership, caps.

const A = `session-a`
const B = `session-b`
const text = (topic: string, body: string, sessionId = A): GuideEntry => ({
  topic,
  label: null,
  attachmentId: null,
  width: null,
  height: null,
  text: body,
  sessionId,
})
const pic = (topic: string, label: string, attachmentId: string, sessionId = A): GuideEntry => ({
  topic,
  label,
  attachmentId,
  width: 10,
  height: 10,
  sessionId,
})

describe(`upsertGuideText`, () => {
  it(`appends a new topic, replaces the writer's own in place`, () => {
    let guide = upsertGuideText(null, `Summary`, `one`, undefined, { at: 1, sessionId: A })!
    guide = upsertGuideText(guide, `UI`, `two`, [`a.ts`], { at: 2, sessionId: A })!
    guide = upsertGuideText(guide, `Summary`, `three`, undefined, { at: 3, sessionId: A })!
    expect(guide.map((row) => [row.topic, row.text, row.at])).toEqual([
      [`Summary`, `three`, 3],
      [`UI`, `two`, 2],
    ])
  })

  it(`never replaces another writer's topic: files its own beside it`, () => {
    const guide = upsertGuideText([text(`Summary`, `mine`, B)], `Summary`, `yours`, undefined, {
      at: 1,
      sessionId: A,
    })!
    expect(guide.map((row) => [row.text, row.sessionId])).toEqual([
      [`mine`, B],
      [`yours`, A],
    ])
  })

  it(`puts text above the topic's existing pictures and keeps/cleans files`, () => {
    let guide = upsertGuideText([pic(`UI`, `web`, `att-1`)], `UI`, `report`, [` a.ts `, `a.ts`, ``], {
      at: 1,
      sessionId: A,
    })!
    expect(guide[0]).toMatchObject({ text: `report`, files: [`a.ts`] })
    guide = upsertGuideText(guide, `UI`, `again`, undefined, { at: 2, sessionId: A })!
    expect(guide[0]!.files).toEqual([`a.ts`])
    guide = upsertGuideText(guide, `UI`, `again`, [], { at: 3, sessionId: A })!
    expect(guide[0]!.files).toBeUndefined()
  })

  it(`keeps, sets and clears the PR tag`, () => {
    let guide = upsertGuideText(null, `T`, `x`, undefined, {
      at: 1,
      sessionId: A,
      prUrl: `https://github.com/o/r/pull/1`,
    })!
    guide = upsertGuideText(guide, `T`, `y`, undefined, { at: 2, sessionId: A })!
    expect(guide[0]!.prUrl).toBe(`https://github.com/o/r/pull/1`)
    guide = upsertGuideText(guide, `T`, `z`, undefined, { at: 3, sessionId: A, prUrl: null })!
    expect(guide[0]!.prUrl).toBeUndefined()
  })

  it(`refuses growth past the per-writer caps only`, () => {
    const big = `x`.repeat(SESSION_RESULTS_REPORT_TOTAL_MAX)
    const theirs = [text(`Other`, big, B)]
    // Another writer's text does not count against A.
    expect(upsertGuideText(theirs, `Mine`, `short`, undefined, { at: 1, sessionId: A })).not.toBeNull()
    const mine = [text(`Mine`, big)]
    expect(upsertGuideText(mine, `More`, `y`, undefined, { at: 1, sessionId: A })).toBeNull()
    // Shrinking an over-cap report still lands.
    expect(upsertGuideText(mine, `Mine`, `small`, undefined, { at: 1, sessionId: A })).not.toBeNull()
    const files = Array.from({ length: SESSION_RESULT_FILES_MAX + 5 }, (_, i) => `f${i}`)
    const capped = upsertGuideText(null, `F`, `x`, files, { at: 1, sessionId: A })!
    expect(capped[0]!.files).toHaveLength(SESSION_RESULT_FILES_MAX)
  })
})

describe(`pictures and removal`, () => {
  it(`replaces a (topic, label) picture in place and reports the displaced id`, () => {
    const out = upsertGuidePicture([pic(`UI`, `web`, `old`)], pic(`UI`, `web`, `new`), ownedBy(A))
    expect(out.results.map((row) => row.attachmentId)).toEqual([`new`])
    expect(out.displacedAttachmentId).toBe(`old`)
    const other = upsertGuidePicture([pic(`UI`, `web`, `theirs`, B)], pic(`UI`, `web`, `new`), ownedBy(A))
    expect(other.results).toHaveLength(2)
    expect(other.displacedAttachmentId).toBeNull()
  })

  it(`removes a label, a topic's text, or the whole topic — own entries only`, () => {
    const guide = [text(`UI`, `t`), pic(`UI`, `web`, `a1`), pic(`UI`, `ios`, `a2`), pic(`UI`, `x`, `b1`, B)]
    expect(removeGuideEntries(guide, { topic: `UI`, label: `web` }, ownedBy(A))).toMatchObject({
      removedAttachmentIds: [`a1`],
    })
    expect(removeGuideEntries(guide, { topic: `UI`, text: true }, ownedBy(A)).results).toHaveLength(3)
    const whole = removeGuideEntries(guide, { topic: `UI` }, ownedBy(A))
    expect(whole.removedAttachmentIds).toEqual([`a1`, `a2`])
    expect(whole.results).toEqual([pic(`UI`, `x`, `b1`, B)])
  })

  it(`EXP-1351: a remove reaches the caller's subtree, never a sibling's or parent's`, () => {
    const tree = [
      { id: A, parentId: null },
      { id: B, parentId: A },
      { id: `grandchild`, parentId: B },
      { id: `sibling`, parentId: A },
    ]
    const guide = [
      pic(`UI`, `web`, `a1`),
      pic(`UI`, `web`, `b1`, B),
      pic(`UI`, `web`, `g1`, `grandchild`),
      pic(`UI`, `web`, `s1`, `sibling`),
    ]
    const fromRoot = removeGuideEntries(guide, { topic: `UI`, label: `web` }, ownedBySubtree(A, tree))
    expect(fromRoot.removedAttachmentIds).toEqual([`a1`, `b1`, `g1`, `s1`])
    const fromChild = removeGuideEntries(guide, { topic: `UI`, label: `web` }, ownedBySubtree(B, tree))
    expect(fromChild.removedAttachmentIds).toEqual([`b1`, `g1`])
  })

  it(`summarizes, labels shows, lists attachment ids`, () => {
    const guide = [text(`UI`, `four`), pic(`UI`, `Shot 1`, `a1`)]
    expect(guideSummary(guide)).toEqual([
      { topic: `UI`, text: 4 },
      { topic: `UI`, label: `Shot 1` },
    ])
    expect(nextShowLabel(guide, `UI`)).toBe(`Shot 2`)
    expect(guideAttachmentIds(guide)).toEqual([`a1`])
  })
})

describe(`PR helpers`, () => {
  it(`canonicalizes GitHub PR urls`, () => {
    expect(canonicalGithubPrUrl(`http://WWW.github.com/o/r/pull/007/files?x=1#y`)).toBe(
      `https://github.com/o/r/pull/7`
    )
    expect(canonicalGithubPrUrl(`https://gitlab.com/o/r/pull/7`)).toBeNull()
    expect(canonicalGithubPrUrl(`nope`)).toBeNull()
  })

  it(`tags only the writer's untagged text topics`, () => {
    const guide = [text(`A`, `x`), { ...text(`B`, `y`), prUrl: `p0` }, text(`C`, `z`, B)]
    const { results, changed } = stampUntaggedGuideEntries(guide, `p1`, ownedBy(A))
    expect(changed).toBe(true)
    expect(results.map((row) => row.prUrl ?? null)).toEqual([`p1`, `p0`, null])
  })

  it(`names the listed files a diff lacks (renames count)`, () => {
    expect(
      missingGuideFiles([`a`, `b`, `old`], [{ path: `a` }, { path: `new`, previousPath: `old` }])
    ).toEqual([`b`])
  })
})

// EXP-1397/#EXP-1402: ownership follows the logical session: the
// `continuedFromId` lineage, never the branch.
describe(`logicalSessionIds`, () => {
  const tree = [
    { id: `root`, parentId: null, status: `idle` },
    { id: `c1`, parentId: `root`, status: `ended` },
    { id: `c2`, parentId: `root`, continuedFromId: `c1`, status: `ended` },
    { id: `c3`, parentId: `root`, continuedFromId: `c2`, status: `running` },
    // Same branch as c1..c3 but no lineage: never theirs.
    { id: `other`, parentId: `root`, status: `ended` },
    { id: `g`, parentId: `c1`, status: `ended` },
  ]

  it(`is the row, the ended rows it continues and the rows continuing it`, () => {
    expect(logicalSessionIds(`c3`, tree)).toEqual([`c3`, `c2`, `c1`])
    // An ended predecessor's logical session reaches its running continuation.
    expect(logicalSessionIds(`c1`, tree)).toEqual([`c1`, `c2`, `c3`])
    expect(logicalSessionIds(`other`, tree)).toEqual([`other`])
    expect(logicalSessionIds(`x`, [{ id: `x`, parentId: `root`, status: `queued` }])).toEqual([`x`])
  })

  it(`walks forward only from ended rows: a revived root never owns its live continuation`, () => {
    const revived = [
      { id: `R`, parentId: null, status: `idle` },
      { id: `C`, parentId: `R`, continuedFromId: `R`, status: `running` },
    ]
    expect(logicalSessionIds(`R`, revived)).toEqual([`R`])
    // Nor does C reach back to the live R.
    expect(logicalSessionIds(`C`, revived)).toEqual([`C`])
    // A live middle link stops the walk past it.
    const chain = [
      { id: `a`, parentId: `root`, status: `ended` },
      { id: `b`, parentId: `root`, continuedFromId: `a`, status: `running` },
      { id: `c`, parentId: `root`, continuedFromId: `b`, status: `running` },
    ]
    expect(logicalSessionIds(`a`, chain)).toEqual([`a`, `b`])
  })

  it(`never takes a sibling continuation of a shared predecessor`, () => {
    const forked = [...tree, { id: `c4`, parentId: `root`, continuedFromId: `c2`, status: `running` }]
    expect(logicalSessionIds(`c4`, forked)).toEqual([`c4`, `c2`, `c1`])
    expect(logicalSessionIds(`c3`, forked)).toEqual([`c3`, `c2`, `c1`])
  })

  it(`owns and reaches across the chain`, () => {
    const mine = logicalSessionIds(`c3`, tree)
    expect(ownedByAny(mine)({ sessionId: `c1` } as GuideEntry)).toBe(true)
    expect(ownedByAny(mine)({ sessionId: `other` } as GuideEntry)).toBe(false)
    // A predecessor's child is in the continuation's subtree.
    expect(ownedBySubtree(mine, tree)({ sessionId: `g` } as GuideEntry)).toBe(true)
    expect(ownedBySubtree(mine, tree)({ sessionId: `root` } as GuideEntry)).toBe(false)
  })
})

// FEED-156: continuation c91709c1 of ended 308a43e7 (same branch, PR #1257).
describe(`FEED-156: a continuation owns its predecessor's topics`, () => {
  const tree = [
    { id: `574977b3`, parentId: null, status: `idle` },
    { id: `308a43e7`, parentId: `574977b3`, status: `ended` },
    { id: `c91709c1`, parentId: `574977b3`, continuedFromId: `308a43e7`, status: `running` },
  ]
  const owned = ownedByAny(logicalSessionIds(`c91709c1`, tree))
  const stale: GuideEntry = { ...text(`Summary`, `Old.`), sessionId: `308a43e7`, at: 1 }

  it(`removes the predecessor's topic`, () => {
    const { results } = removeGuideEntries([stale], { topic: `Summary` }, owned)
    expect(results).toEqual([])
  })

  it(`a same-name write replaces the row instead of adding a second`, () => {
    const out = upsertGuideText([stale], `Summary`, `New.`, undefined, { at: 2, sessionId: `c91709c1` }, owned)!
    expect(out).toHaveLength(1)
    expect(out[0]).toMatchObject({ topic: `Summary`, text: `New.`, sessionId: `c91709c1` })
  })

  it(`and folds the duplicates an earlier write left behind`, () => {
    const dup: GuideEntry = { ...text(`Summary`, `Dup.`), sessionId: `c91709c1`, at: 3 }
    const foreign: GuideEntry = { ...text(`Summary`, `Theirs.`), sessionId: `elsewhere`, at: 4 }
    const out = upsertGuideText([stale, foreign, dup], `Summary`, `New.`, undefined, { at: 5, sessionId: `c91709c1` }, owned)!
    expect(out.map((row) => row.text)).toEqual([`New.`, `Theirs.`])
  })
})

describe(`#EXP-1402: the explicit untag and the stamp guard`, () => {
  it(`prUrl: null is remembered across rewrites and never stamped`, () => {
    const first = upsertGuideText(null, `T`, `a`, undefined, { at: 1, sessionId: `s`, prUrl: null })!
    expect(first[0]!.untagged).toBe(true)
    const again = upsertGuideText(first, `T`, `b`, undefined, { at: 2, sessionId: `s` })!
    expect(again[0]!.untagged).toBe(true)
    expect(stampUntaggedGuideEntries(again, `https://github.com/o/r/pull/1`).changed).toBe(false)
    // Tagging clears the marker.
    const tagged = upsertGuideText(again, `T`, `c`, undefined, { at: 3, sessionId: `s`, prUrl: `https://github.com/o/r/pull/2` })!
    expect(tagged[0]!.untagged).toBeUndefined()
  })

  it(`a rewrite drops a kept tag whose PR is merged or closed for the open-PR default`, () => {
    const old = `https://github.com/o/r/pull/1`
    const live = `https://github.com/o/r/pull/2`
    const first = upsertGuideText(null, `T`, `a`, undefined, { at: 1, sessionId: `s`, prUrl: old })!
    const stale = (url: string) => url === old
    const moved = upsertGuideText(first, `T`, `b`, undefined, { at: 2, sessionId: `s`, stalePrUrl: stale, fallbackPrUrl: live })!
    expect(moved[0]).toMatchObject({ text: `b`, prUrl: live })
    // No open PR: untagged, but NOT sticky, so the next PR open stamps it.
    const bare = upsertGuideText(first, `T`, `b`, undefined, { at: 2, sessionId: `s`, stalePrUrl: stale, fallbackPrUrl: null })!
    expect(bare[0]!.prUrl).toBeUndefined()
    expect(bare[0]!.untagged).toBeUndefined()
    expect(stampUntaggedGuideEntries(bare, live).changed).toBe(true)
    // A live kept tag stays.
    const kept = upsertGuideText(first, `T`, `b`, undefined, { at: 2, sessionId: `s`, stalePrUrl: () => false, fallbackPrUrl: live })!
    expect(kept[0]!.prUrl).toBe(old)
  })

})

describe(`soleHeadSession`, () => {
  it(`resolves a head only when one lineage sits on it`, () => {
    const one = [
      { id: `a`, parentId: null, status: `ended`, branch: `exp/x` },
      { id: `b`, parentId: null, continuedFromId: `a`, status: `running`, branch: `exp/x` },
    ]
    expect(soleHeadSession(one, `exp/x`)).toBe(`b`)
    expect(soleHeadSession(one, `exp/y`)).toBeNull()
    const two = [...one, { id: `c`, parentId: null, status: `running`, branch: `exp/x` }]
    expect(soleHeadSession(two, `exp/x`)).toBeNull()
  })
})
