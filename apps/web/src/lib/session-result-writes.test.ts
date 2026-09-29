import { describe, expect, it } from "vitest"
import type { CodingSessionResult } from "@exp/db-schema/domain"
import {
  exceedsSessionResultsCap,
  removeSessionResults,
  resultsSummary,
  upsertSessionResult,
  upsertSessionResultText,
} from "./session-result-writes"

// EXP-879: the server-side list algebra behind the token upload route and MCP
// `exponential_sessions_results`.

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
    for (let i = 0; i < 3; i++) results = upsertSessionResultText(results, `t${i}`, `x`.repeat(4000))
    expect(results).not.toBeNull()
    expect(upsertSessionResultText(results, `t3`, `x`)).toBeNull()
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
