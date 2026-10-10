import { describe, expect, it } from "vitest"
import {
  SESSION_RESULT_FILES_MAX,
  SESSION_RESULT_REPORT_MAX,
  SESSION_RESULTS_FILES_TOTAL_MAX,
  SESSION_RESULTS_REPORT_TOTAL_MAX,
  type CodingSessionResult,
} from "@exp/db-schema/domain"
import {
  exceedsSessionResultsCap,
  removeSessionResults,
  reportFileCount,
  resultsSummary,
  upsertSessionResult,
  upsertSessionResultText,
} from "./session-result-writes"

// EXP-879: the server-side list algebra behind the token upload route and MCP
// `exponential_sessions_guide`.

const entry = (
  topic: string,
  label: string,
  attachmentId: string
): CodingSessionResult => ({
  topic,
  label,
  attachmentId,
  width: 100,
  height: 50,
})

describe(`upsertSessionResult`, () => {
  it(`appends a new topic/label pair at the end`, () => {
    const { results, displacedAttachmentId } = upsertSessionResult(
      [entry(`chatui`, `web`, `a1`)],
      entry(`chatui`, `ios`, `a2`)
    )
    expect(results.map((row) => row.attachmentId)).toEqual([`a1`, `a2`])
    expect(displacedAttachmentId).toBeNull()
  })

  it(`starts a list from null`, () => {
    const { results } = upsertSessionResult(null, entry(`t`, `l`, `a`))
    expect(results).toEqual([entry(`t`, `l`, `a`)])
  })

  it(`replaces the same topic and label IN PLACE and names the displaced blob`, () => {
    const current = [
      entry(`chatui`, `web`, `a1`),
      entry(`chatui`, `ios`, `a2`),
      entry(`nav`, `web`, `a3`),
    ]
    const { results, displacedAttachmentId } = upsertSessionResult(
      current,
      entry(`chatui`, `ios`, `a9`)
    )
    expect(results.map((row) => row.attachmentId)).toEqual([`a1`, `a9`, `a3`])
    expect(displacedAttachmentId).toBe(`a2`)
    // The input list is never mutated.
    expect(current[1].attachmentId).toBe(`a2`)
  })

  it(`never displaces the attachment it just wrote`, () => {
    const { displacedAttachmentId } = upsertSessionResult(
      [entry(`t`, `l`, `a1`)],
      entry(`t`, `l`, `a1`)
    )
    expect(displacedAttachmentId).toBeNull()
  })
})

describe(`removeSessionResults`, () => {
  const current = [
    entry(`chatui`, `web`, `a1`),
    entry(`chatui`, `ios`, `a2`),
    entry(`nav`, `web`, `a3`),
  ]

  it(`removes one label and reports its attachment`, () => {
    const { results, removedAttachmentIds } = removeSessionResults(current, {
      topic: `chatui`,
      label: `ios`,
    })
    expect(results.map((row) => row.attachmentId)).toEqual([`a1`, `a3`])
    expect(removedAttachmentIds).toEqual([`a2`])
  })

  it(`removes a whole topic without a label`, () => {
    const { results, removedAttachmentIds } = removeSessionResults(current, {
      topic: `chatui`,
    })
    expect(results.map((row) => row.attachmentId)).toEqual([`a3`])
    expect(removedAttachmentIds).toEqual([`a1`, `a2`])
  })

  it(`is a no-op for an unknown topic or label, and for a null list`, () => {
    expect(
      removeSessionResults(current, { topic: `chatui`, label: `android` })
    ).toEqual({ results: current, removedAttachmentIds: [] })
    expect(removeSessionResults(current, { topic: `nope` }).results).toEqual(
      current
    )
    expect(removeSessionResults(null, { topic: `t` })).toEqual({
      results: [],
      removedAttachmentIds: [],
    })
  })
})

describe(`resultsSummary`, () => {
  it(`projects the compact topic/label list every response carries`, () => {
    expect(
      resultsSummary([entry(`chatui`, `web`, `a1`), entry(`nav`, `ios`, `a2`)])
    ).toEqual([
      { topic: `chatui`, label: `web` },
      { topic: `nav`, label: `ios` },
    ])
    expect(resultsSummary(null)).toEqual([])
    expect(
      resultsSummary([{ topic: `t` } as unknown as CodingSessionResult])
    ).toEqual([])
  })
})

describe(`exceedsSessionResultsCap`, () => {
  it(`trips one past the 60-entry cap`, () => {
    const list = (count: number) =>
      Array.from({ length: count }, (_, index) =>
        entry(`t`, `l${index}`, `a${index}`)
      )
    expect(exceedsSessionResultsCap(list(60))).toBe(false)
    expect(exceedsSessionResultsCap(list(61))).toBe(true)
  })
})

// EXP-933: a topic's report text rides the same list as its pictures.
describe(`upsertSessionResultText`, () => {
  it(`appends a new topic's text at the end`, () => {
    const results = upsertSessionResultText([entry(`a`, `web`, `x`)], `Summary`, `Did it`)
    expect(results?.map((row) => [row.topic, row.label, row.text ?? null])).toEqual([
      [`a`, `web`, null],
      [`Summary`, null, `Did it`],
    ])
  })

  it(`puts text for a topic that has pictures ABOVE them`, () => {
    const results = upsertSessionResultText(
      [entry(`a`, `web`, `x`), entry(`b`, `web`, `y`)],
      `b`,
      `About b`
    )
    expect(results?.map((row) => row.attachmentId ?? row.text)).toEqual([`x`, `About b`, `y`])
  })

  it(`replaces a topic's text in place`, () => {
    const first = upsertSessionResultText(null, `t`, `one`)
    const second = upsertSessionResultText([...(first ?? []), entry(`u`, `web`, `z`)], `t`, `two`)
    expect(second?.map((row) => row.text ?? row.attachmentId)).toEqual([`two`, `z`])
  })

  it(`refuses a list whose report text outgrows the total cap`, () => {
    let results: CodingSessionResult[] | null = null
    const per = SESSION_RESULT_REPORT_MAX
    const count = SESSION_RESULTS_REPORT_TOTAL_MAX / per
    for (let i = 0; i < Math.floor(count); i++) {
      results = upsertSessionResultText(results, `t${i}`, `x`.repeat(per))
    }
    results = upsertSessionResultText(
      results,
      `rest`,
      `x`.repeat(SESSION_RESULTS_REPORT_TOTAL_MAX - Math.floor(count) * per)
    )
    expect(results).not.toBeNull()
    expect(upsertSessionResultText(results, `t-over`, `x`)).toBeNull()
  })

  // EXP-1154: a run filed under the older, larger caps may still shorten.
  it(`lets a list already over the total cap shrink, never grow`, () => {
    const big = (topic: string, n: number): CodingSessionResult => ({
      topic,
      label: null,
      attachmentId: null,
      width: null,
      height: null,
      text: `x`.repeat(n),
    })
    const over = [big(`a`, 3000), big(`b`, 3000)]
    expect(upsertSessionResultText(over, `a`, `short`)?.[0]?.text).toBe(`short`)
    expect(upsertSessionResultText(over, `a`, `x`.repeat(3001))).toBeNull()
  })
})

// EXP-1154: a text entry's files.
describe(`upsertSessionResultText files`, () => {
  it(`cleans, dedupes and stores files only when there are any`, () => {
    const results = upsertSessionResultText(null, `t`, `r`, [` a.ts `, ``, `b.ts`, `a.ts`])
    expect(results?.[0]?.files).toEqual([`a.ts`, `b.ts`])
    expect(upsertSessionResultText(null, `t`, `r`, [])?.[0]).not.toHaveProperty(`files`)
    expect(upsertSessionResultText(null, `t`, `r`)?.[0]).not.toHaveProperty(`files`)
  })

  it(`undefined keeps the replaced entry's files, [] clears them`, () => {
    const first = upsertSessionResultText(null, `t`, `one`, [`a.ts`])
    expect(upsertSessionResultText(first, `t`, `two`)?.[0]?.files).toEqual([`a.ts`])
    expect(upsertSessionResultText(first, `t`, `two`, [])?.[0]).not.toHaveProperty(`files`)
    expect(upsertSessionResultText(first, `t`, `two`, [`b.ts`])?.[0]?.files).toEqual([`b.ts`])
  })

  it(`caps a topic's files`, () => {
    const many = Array.from({ length: SESSION_RESULT_FILES_MAX + 5 }, (_, i) => `f${i}.ts`)
    expect(upsertSessionResultText(null, `t`, `r`, many)?.[0]?.files).toHaveLength(
      SESSION_RESULT_FILES_MAX
    )
  })

  it(`refuses a write that grows the run past the files cap, allows a shrink`, () => {
    const many = (prefix: string) =>
      Array.from({ length: SESSION_RESULT_FILES_MAX }, (_, i) => `${prefix}${i}.ts`)
    let results: CodingSessionResult[] | null = null
    const topics = SESSION_RESULTS_FILES_TOTAL_MAX / SESSION_RESULT_FILES_MAX
    for (let i = 0; i < topics; i++) {
      results = upsertSessionResultText(results, `t${i}`, `r`, many(`t${i}/`))
    }
    expect(reportFileCount(results ?? [])).toBe(SESSION_RESULTS_FILES_TOTAL_MAX)
    expect(upsertSessionResultText(results, `extra`, `r`, [`x.ts`])).toBeNull()
    expect(upsertSessionResultText(results, `extra`, `r`)).not.toBeNull()
    expect(upsertSessionResultText(results, `t0`, `r`, [`x.ts`])).not.toBeNull()
  })

  it(`a picture upsert never replaces the topic's text`, () => {
    const withText = upsertSessionResultText(null, `t`, `report`) ?? []
    const { results } = upsertSessionResult(withText, entry(`t`, `web`, `a`))
    expect(results).toHaveLength(2)
  })

  it(`removing a label keeps the text, removing text keeps the pictures, removing the topic drops both`, () => {
    const list = [...(upsertSessionResultText(null, `t`, `r`) ?? []), entry(`t`, `web`, `a`)]
    expect(removeSessionResults(list, { topic: `t`, label: `web` }).results).toHaveLength(1)
    expect(removeSessionResults(list, { topic: `t`, text: true }).results.map((r) => r.attachmentId)).toEqual([`a`])
    const whole = removeSessionResults(list, { topic: `t` })
    expect(whole.results).toEqual([])
    expect(whole.removedAttachmentIds).toEqual([`a`])
  })

  it(`summarises text entries by length`, () => {
    const list = [...(upsertSessionResultText(null, `t`, `abc`) ?? []), entry(`t`, `web`, `a`)]
    expect(resultsSummary(list)).toEqual([
      { topic: `t`, text: 3 },
      { topic: `t`, label: `web` },
    ])
  })
})

// EXP-1172: a show without a label never replaces an earlier picture.
import { nextShowLabel } from "./session-result-writes"

describe(`nextShowLabel`, () => {
  it(`counts the topic's pictures and skips a taken label`, () => {
    expect(nextShowLabel(null, `Progress`)).toBe(`Shot 1`)
    const pic = (topic: string, label: string) => ({
      topic,
      label,
      attachmentId: label,
      width: null,
      height: null,
    })
    expect(
      nextShowLabel(
        [
          pic(`Progress`, `Shot 1`),
          pic(`other`, `Shot 2`),
          { topic: `Progress`, label: null, attachmentId: null, width: null, height: null, text: `t` },
        ],
        `Progress`
      )
    ).toBe(`Shot 2`)
    expect(nextShowLabel([pic(`Progress`, `Shot 2`)], `Progress`)).toBe(`Shot 3`)
  })
})

// EXP-1245/1251: the write stamp, the per-PR tag and the Guide's missing-file
// answer.
import {
  canonicalGithubPrUrl,
  missingGuideFiles,
  stampUntaggedResults,
} from "./session-result-writes"

describe(`guide writes`, () => {
  it(`stamps at and tags a topic's PR, keeping the tag on a rewrite`, () => {
    const first = upsertSessionResultText(null, `t`, `one`, undefined, {
      at: 1000,
      prUrl: ` https://github.com/o/r/pull/2 `,
    })
    expect(first?.[0]).toMatchObject({ at: 1000, prUrl: `https://github.com/o/r/pull/2` })
    const second = upsertSessionResultText(first, `t`, `two`, undefined, { at: 2000 })
    expect(second?.[0]).toMatchObject({ text: `two`, at: 2000, prUrl: `https://github.com/o/r/pull/2` })
    const cleared = upsertSessionResultText(second, `t`, `three`, undefined, { prUrl: `` })
    expect(cleared?.[0]).not.toHaveProperty(`prUrl`)
    expect(upsertSessionResultText(null, `t`, `x`)?.[0]).not.toHaveProperty(`at`)
  })

  it(`names the listed paths the diff lacks, renames matching by their source`, () => {
    expect(
      missingGuideFiles([`a.ts`, `old.ts`, `gone.ts`], [
        { path: `a.ts` },
        { path: `new.ts`, previousPath: `old.ts` },
      ])
    ).toEqual([`gone.ts`])
    expect(missingGuideFiles([], [])).toEqual([])
  })

  it(`folds every spelling of a GitHub PR url to the canonical one`, () => {
    const canonical = `https://github.com/o/r/pull/12`
    for (const raw of [
      canonical,
      ` ${canonical} `,
      `http://github.com/o/r/pull/12`,
      `https://GitHub.com/o/r/pull/12`,
      `https://www.github.com/o/r/pull/12/`,
      `https://github.com/o/r/pull/12/files`,
      `https://github.com/o/r/pull/12/commits/abc`,
      `https://github.com/o/r/pull/12?diff=split#top`,
      `https://github.com/o/r/pull/012`,
    ]) {
      expect(canonicalGithubPrUrl(raw), raw).toBe(canonical)
    }
    for (const raw of [
      ``,
      `not a url`,
      `ftp://github.com/o/r/pull/12`,
      `https://gitlab.com/o/r/-/merge_requests/1`,
      `https://github.com/o/r/issues/12`,
      `https://github.com/o/r/pull/`,
      `https://github.com/o/r/pull/x`,
      `https://github.com/o/pull/12`,
      `https://evil.github.com/o/r/pull/12`,
    ]) {
      expect(canonicalGithubPrUrl(raw), raw).toBeNull()
    }
  })

  it(`tags only the untagged text topics with a new PR`, () => {
    const list: CodingSessionResult[] = [
      { topic: `a`, label: null, attachmentId: null, width: null, height: null, text: `x` },
      { topic: `b`, label: null, attachmentId: null, width: null, height: null, text: `y`, prUrl: `p1` },
      entry(`a`, `web`, `img`),
    ]
    const out = stampUntaggedResults(list, `p2`)
    expect(out.changed).toBe(true)
    expect(out.results.map((row) => row.prUrl ?? null)).toEqual([`p2`, `p1`, null])
    expect(stampUntaggedResults(out.results, `p3`).changed).toBe(false)
    expect(stampUntaggedResults(null, `p3`)).toEqual({ results: [], changed: false })
  })
})
