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

// EXP-1397: ownership follows the logical session (its continuations).
describe(`logicalSessionIds`, () => {
  const tree = [
    { id: `root`, parentId: null, branch: `exp/root`, status: `idle` },
    { id: `c1`, parentId: `root`, branch: `exp/kid`, status: `ended` },
    { id: `c2`, parentId: `root`, branch: `exp/kid`, status: `ended` },
    { id: `c3`, parentId: `root`, branch: `exp/kid`, status: `running` },
    { id: `other`, parentId: `root`, branch: `exp/other`, status: `ended` },
    { id: `g`, parentId: `c1`, branch: `exp/g`, status: `ended` },
  ]

  it(`is the row and every ended row on its branch`, () => {
    expect(logicalSessionIds(`c3`, tree)).toEqual([`c3`, `c1`, `c2`])
    expect(logicalSessionIds(`other`, tree)).toEqual([`other`])
    // No branch yet: only itself.
    expect(logicalSessionIds(`x`, [{ id: `x`, parentId: `root`, branch: null, status: `queued` }])).toEqual([`x`])
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
