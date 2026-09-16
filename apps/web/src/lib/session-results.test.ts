import { describe, expect, it } from "vitest"
import {
  MAX_SESSION_RESULTS,
  SESSION_RESULT_TILE_HEIGHT,
  groupSessionResults,
  parseSessionResults,
  sessionResultTileHeightFitting,
  sessionResultTileWidth,
} from "./session-results"

// EXP-879: the Results face's pure rules. Test names are mirrored by desktop
// `session_results.rs`, iOS `SessionResultsTests.swift` and Android
// `SessionResultsTest.kt`.

describe(`session results`, () => {
  it(`parses a flat ordered list and drops malformed entries`, () => {
    expect(
      parseSessionResults([
        { topic: `chatui`, label: `web`, attachmentId: `a1`, width: 1600, height: 900 },
        // No label, no attachment, blank topic: all dropped.
        { topic: `chatui`, attachmentId: `a2`, width: 10, height: 10 },
        { topic: `chatui`, label: `ios`, width: 10, height: 10 },
        { topic: `   `, label: `android`, attachmentId: `a3` },
        { topic: `chatui`, label: 7, attachmentId: `a4` },
        // Unmeasurable dimensions degrade to null, the entry survives.
        { topic: `chatui`, label: `ios`, attachmentId: `a5`, width: 0, height: -3 },
        { topic: `nav`, label: `web`, attachmentId: `a6`, width: `800`, height: null },
        null,
        `nope`,
        [],
      ])
    ).toEqual([
      { topic: `chatui`, label: `web`, attachmentId: `a1`, width: 1600, height: 900 },
      { topic: `chatui`, label: `ios`, attachmentId: `a5`, width: null, height: null },
      { topic: `nav`, label: `web`, attachmentId: `a6`, width: null, height: null },
    ])
    // The natives hand their decoder the raw column, so a JSON string parses
    // exactly like the array does.
    expect(
      parseSessionResults(`[{"topic":"t","label":"l","attachmentId":"a"}]`)
    ).toEqual([
      { topic: `t`, label: `l`, attachmentId: `a`, width: null, height: null },
    ])
    // Topic, label and id are trimmed.
    expect(
      parseSessionResults([{ topic: ` t `, label: ` l `, attachmentId: ` a ` }])[0]
    ).toMatchObject({ topic: `t`, label: `l`, attachmentId: `a` })
  })

  it(`ignores unknown fields and a null or blank blob`, () => {
    expect(
      parseSessionResults([
        {
          topic: `t`,
          label: `l`,
          attachmentId: `a`,
          width: 4,
          height: 2,
          url: `/api/attachments/a`,
          extra: { nope: true },
        },
      ])
    ).toEqual([
      { topic: `t`, label: `l`, attachmentId: `a`, width: 4, height: 2 },
    ])
    expect(parseSessionResults(null)).toEqual([])
    expect(parseSessionResults(undefined)).toEqual([])
    expect(parseSessionResults(``)).toEqual([])
    expect(parseSessionResults(`   `)).toEqual([])
    expect(parseSessionResults(`{`)).toEqual([])
    expect(parseSessionResults({ topic: `t` })).toEqual([])
    expect(parseSessionResults(42)).toEqual([])
  })

  it(`groups by topic in first-seen order`, () => {
    const entries = parseSessionResults([
      { topic: `chatui`, label: `web`, attachmentId: `a1` },
      { topic: `nav`, label: `web`, attachmentId: `a2` },
      { topic: `chatui`, label: `ios`, attachmentId: `a3` },
    ])
    expect(groupSessionResults(entries)).toEqual([
      {
        topic: `chatui`,
        entries: [
          { topic: `chatui`, label: `web`, attachmentId: `a1`, width: null, height: null },
          { topic: `chatui`, label: `ios`, attachmentId: `a3`, width: null, height: null },
        ],
      },
      {
        topic: `nav`,
        entries: [
          { topic: `nav`, label: `web`, attachmentId: `a2`, width: null, height: null },
        ],
      },
    ])
    expect(groupSessionResults([])).toEqual([])
  })

  it(`caps at 60 entries`, () => {
    expect(MAX_SESSION_RESULTS).toBe(60)
    const many = Array.from({ length: 80 }, (_, index) => ({
      topic: `t`,
      label: `l${index}`,
      attachmentId: `a${index}`,
    }))
    const parsed = parseSessionResults(many)
    expect(parsed.length).toBe(60)
    expect(parsed[59].label).toBe(`l59`)
  })

  it(`sizes a tile from the probed aspect, 4:3 without one`, () => {
    expect(SESSION_RESULT_TILE_HEIGHT).toBe(320)
    expect(sessionResultTileWidth({ width: 1600, height: 900 })).toBe(569)
    expect(sessionResultTileWidth({ width: 1170, height: 2532 })).toBe(148)
    // Either dimension missing falls back to 4:3.
    expect(sessionResultTileWidth({ width: null, height: 900 })).toBe(427)
    expect(sessionResultTileWidth({ width: 1600, height: null })).toBe(427)
    expect(sessionResultTileWidth({ width: null, height: null }, 120)).toBe(160)
    expect(sessionResultTileWidth({ width: 1000, height: 1000 }, 200)).toBe(200)
  })

  it(`scales every tile down by one factor when the widest overflows the page`, () => {
    const entries = parseSessionResults([
      // 480px wide at the 320px base — wider than a phone.
      { topic: `t`, label: `web`, attachmentId: `a1`, width: 1800, height: 1200 },
      { topic: `t`, label: `ios`, attachmentId: `a2`, width: 828, height: 1800 },
    ])
    expect(sessionResultTileWidth(entries[0])).toBe(480)
    // A 358px phone column: floor(320 * 358 / 480).
    const height = sessionResultTileHeightFitting(entries, 358)
    expect(height).toBe(238)
    // Aspects survive the scale: the one factor is the page's, not a row's.
    expect(sessionResultTileWidth(entries[0], height)).toBe(357)
    expect(sessionResultTileWidth(entries[1], height)).toBe(109)
    // A page that already fits — and an unmeasured one — keep the base.
    expect(sessionResultTileHeightFitting(entries, 1000)).toBe(320)
    expect(sessionResultTileHeightFitting(entries, 0)).toBe(320)
    expect(sessionResultTileHeightFitting(entries, -10)).toBe(320)
    expect(sessionResultTileHeightFitting(entries, Number.NaN)).toBe(320)
    expect(sessionResultTileHeightFitting([], 10)).toBe(320)
  })
})
