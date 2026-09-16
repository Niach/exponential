import { describe, expect, it } from "vitest"
import type { CodingSessionResult } from "@exp/db-schema/domain"
import {
  exceedsSessionResultsCap,
  removeSessionResults,
  resultsSummary,
  upsertSessionResult,
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
