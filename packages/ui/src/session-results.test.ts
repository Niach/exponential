import { describe, expect, it } from "vitest"
import {
  MAX_SESSION_RESULTS,
  SESSION_RESULT_TALL_ASPECT,
  SESSION_RESULT_TILE_HEIGHT,
  parseSessionResults,
  sessionResultIsTall,
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
        { topic: `chatui`, label: `web`, attachmentId: `a1`, width: 1600, height: 900, inline: false, caption: null },
        // No label, no attachment, blank topic: all dropped.
        { topic: `chatui`, attachmentId: `a2`, width: 10, height: 10, inline: false, caption: null },
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
      { topic: `chatui`, label: `web`, attachmentId: `a1`, width: 1600, height: 900, inline: false, caption: null },
      { topic: `chatui`, label: `ios`, attachmentId: `a5`, width: null, height: null, inline: false, caption: null },
      { topic: `nav`, label: `web`, attachmentId: `a6`, width: null, height: null, inline: false, caption: null },
    ])
    // The natives hand their decoder the raw column, so a JSON string parses
    // exactly like the array does.
    expect(
      parseSessionResults(`[{"topic":"t","label":"l","attachmentId":"a"}]`)
    ).toEqual([
      { topic: `t`, label: `l`, attachmentId: `a`, width: null, height: null, inline: false, caption: null },
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
      { topic: `t`, label: `l`, attachmentId: `a`, width: 4, height: 2, inline: false, caption: null },
    ])
    expect(parseSessionResults(null)).toEqual([])
    expect(parseSessionResults(undefined)).toEqual([])
    expect(parseSessionResults(``)).toEqual([])
    expect(parseSessionResults(`   `)).toEqual([])
    expect(parseSessionResults(`{`)).toEqual([])
    expect(parseSessionResults({ topic: `t` })).toEqual([])
    expect(parseSessionResults(42)).toEqual([])
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
      { topic: `t`, label: `web`, attachmentId: `a1`, width: 1800, height: 1200, inline: false, caption: null },
      { topic: `t`, label: `ios`, attachmentId: `a2`, width: 828, height: 1800, inline: false, caption: null },
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

// EXP-933: the report fixture, shared ×4.
import fixture from "@exp/domain-contract/fixtures/session-results.json"
import { parseSessionResultGroups } from "./session-results"

describe(`session results report fixture`, () => {
  for (const c of fixture.groups) {
    it(c.name, () => {
      const groups = parseSessionResultGroups(c.raw).map((group) => ({
        topic: group.topic,
        text: group.text,
        entries: group.entries.map((entry) => ({
          label: entry.label,
          attachmentId: entry.attachmentId,
        })),
      }))
      expect(groups).toEqual(c.expected)
    })
  }

  // EXP-1128: the tall rule, the fixture's `tiles` cases ×4.
  it(`frames a tall capture at 4:3 and flags it`, () => {
    expect(SESSION_RESULT_TALL_ASPECT).toBeCloseTo(1 / 3, 10)
    expect(fixture.tiles.cases.length).toBeGreaterThan(0)
    for (const c of fixture.tiles.cases) {
      const entry = { width: c.width, height: c.height }
      expect(sessionResultIsTall(entry), c.name).toBe(c.tall)
      expect(sessionResultTileWidth(entry, 320), c.name).toBe(c.widthAt320)
    }
  })
})

// EXP-1172: the inline-picture fixture, shared ×4.
import inlineFixture from "@exp/domain-contract/fixtures/session-inline.json"
import {
  SESSION_INLINE_TILE_HEIGHT,
  SESSION_RESULTS_EARLIER_LABEL,
  sessionResultPicture,
  sessionResultTileCaption,
} from "./session-results"

describe(`session inline fixture`, () => {
  it(`pins the shared constants`, () => {
    expect(SESSION_INLINE_TILE_HEIGHT).toBe(inlineFixture.inlineTileHeight)
    expect(SESSION_RESULTS_EARLIER_LABEL).toBe(inlineFixture.earlierLabel)
  })
  const pick = (entry: { label: string; attachmentId: string; inline: boolean; caption: string | null }) => ({
    label: entry.label,
    attachmentId: entry.attachmentId,
    inline: entry.inline,
    caption: entry.caption,
  })
  for (const c of inlineFixture.groups) {
    it(`folds: ${c.name}`, () => {
      const groups = parseSessionResultGroups(c.raw).map((group) => ({
        topic: group.topic,
        text: group.text,
        entries: group.entries.map(pick),
        earlier: group.earlier.map(pick),
      }))
      expect(groups).toEqual(c.expected)
    })
  }
  for (const c of inlineFixture.lookup) {
    it(`looks up: ${c.name}`, () => {
      const entry = sessionResultPicture(c.raw, c.attachmentId)
      expect(
        entry && {
          label: entry.label,
          inline: entry.inline,
          caption: entry.caption,
          tileCaption: sessionResultTileCaption(entry),
        }
      ).toEqual(c.expected)
    })
  }
})

// EXP-1154: a text entry's files + the Guide rules, the fixture's `files` and
// `guide` blocks ×4.
import {
  SESSION_RESULT_FILES_MAX,
  guideSectionCaption,
  guideSections,
  isSummaryTopic,
} from "./session-results"

describe(`session results guide fixture`, () => {
  it(`pins the files cap`, () => {
    expect(SESSION_RESULT_FILES_MAX).toBe(fixture.files.maxFiles)
  })
  for (const c of fixture.files.cases) {
    it(`files: ${c.name}`, () => {
      expect(
        parseSessionResultGroups(c.raw).map((group) => ({
          topic: group.topic,
          files: group.files,
        }))
      ).toEqual(c.expected)
    })
  }
  it(`caps a topic's files`, () => {
    const many = Array.from({ length: 50 }, (_, index) => `f${index}.ts`)
    const [group] = parseSessionResultGroups([{ topic: `t`, text: `x`, files: many }])
    expect(group.files).toHaveLength(SESSION_RESULT_FILES_MAX)
    expect(group.files[0]).toBe(`f0.ts`)
  })
  for (const c of fixture.guide.sections) {
    it(`sections: ${c.name}`, () => {
      const guide = guideSections(c.topics.map((topic) => ({ topic })))
      expect(guide.lead?.topic ?? null).toBe(c.lead)
      expect(
        guide.sections.map((section) => [section.group.topic, section.index, section.total])
      ).toEqual(c.sections)
    })
  }
  it(`captions`, () => {
    for (const c of fixture.guide.captions) {
      expect(guideSectionCaption(c.index, c.total)).toBe(c.text)
    }
  })
  it(`reads the Summary topic trimmed and case-insensitive`, () => {
    expect(isSummaryTopic(` summary `)).toBe(true)
    expect(isSummaryTopic(`SUMMARY`)).toBe(true)
    expect(isSummaryTopic(`Summary of nav`)).toBe(false)
  })
})

// EXP-1175: the Run face's thread, the fixture's `thread` cases ×4.
import { sessionThread } from "./session-results"

describe(`session thread`, () => {
  for (const c of fixture.thread.cases) {
    it(c.name, () => {
      const thread = sessionThread(c.raw)
      expect({
        items: thread.items.map((item) =>
          item.kind === `text`
            ? { kind: `text`, topic: item.topic, text: item.text }
            : { kind: `picture`, attachmentId: item.entry.attachmentId }
        ),
        reply: thread.reply,
      }).toEqual(c.expected)
    })
  }
})

// EXP-1251: the Guide's coverage + PR scope, the fixture's `coverage` and
// `prScope` blocks ×4.
import {
  GUIDE_CHANGES_TOPIC,
  GUIDE_OTHER_CHANGES_TOPIC,
  guideCoverage,
  guideFileCountLabel,
  sessionResultPrUrls,
  sessionResultsForPr,
  type GuideChangeSet,
  type GuideCoverageFile,
} from "./session-results"

describe(`session results coverage fixture`, () => {
  it(`titles the automatic section off the contract`, () => {
    expect(GUIDE_OTHER_CHANGES_TOPIC).toBe(`Other changes`)
    expect(GUIDE_CHANGES_TOPIC).toBe(`Changes`)
  })
  it(`labels a Changes row's file count`, () => {
    for (const [count, label] of fixture.coverage.fileCountLabels) {
      expect(guideFileCountLabel(count as number)).toBe(label)
    }
  })
  const paths = (set: GuideChangeSet<GuideCoverageFile>) => set.files.map((file) => file.path)
  for (const c of fixture.coverage.cases) {
    it(`coverage: ${c.name}`, () => {
      const coverage = guideCoverage(c.groups, c.diff)
      const covered = (
        entry: { group: { topic: string }; changes: GuideChangeSet<GuideCoverageFile> | null; missing: string[] },
        extra: Record<string, unknown> = {}
      ) =>
        entry.changes
          ? {
              topic: entry.group.topic,
              ...extra,
              files: paths(entry.changes),
              additions: entry.changes.additions,
              deletions: entry.changes.deletions,
              missing: entry.missing,
            }
          : { topic: entry.group.topic, ...extra, changes: null, missing: entry.missing }
      expect({
        lead: coverage.lead ? covered(coverage.lead) : null,
        sections: coverage.sections.map((section) =>
          covered(section, { index: section.index, total: section.total })
        ),
        other: coverage.other
          ? {
              topic: coverage.other.topic,
              files: paths(coverage.other.changes),
              additions: coverage.other.changes.additions,
              deletions: coverage.other.changes.deletions,
            }
          : null,
        complete: coverage.complete
          ? {
              fileCount: coverage.complete.fileCount,
              additions: coverage.complete.additions,
              deletions: coverage.complete.deletions,
            }
          : null,
      }).toEqual(c.expected)
    })
  }
  for (const c of fixture.prScope.cases) {
    it(`pr scope: ${c.name}`, () => {
      expect(sessionResultPrUrls(c.raw)).toEqual(c.prUrls)
      for (const scope of c.byPr) {
        expect(
          sessionResultsForPr(c.raw, scope.prUrl).map((record) => record.topic),
          `${scope.prUrl}`
        ).toEqual(scope.topics)
      }
    })
  }
  it(`reads a topic's prUrl onto its group`, () => {
    const [group] = parseSessionResultGroups([
      { topic: `t`, text: `x`, prUrl: ` https://github.com/o/r/pull/1 ` },
    ])
    expect(group.prUrl).toBe(`https://github.com/o/r/pull/1`)
  })
})

// EXP-1245: the thread as turns, the fixture's `turns` cases ×4.
import { sessionTurns, type SessionTurnEvent } from "./session-results"

describe(`session turns`, () => {
  for (const c of fixture.turns.cases) {
    it(c.name, () => {
      const turns = sessionTurns(c.raw, c.feed as SessionTurnEvent[] | null)
      expect({
        perTurn: turns.perTurn,
        turns: turns.turns.map((turn) => ({
          message: turn.message,
          startedAt: turn.startedAt,
          endedAt: turn.endedAt,
          items: turn.items.map((item) =>
            item.kind === `text`
              ? { kind: `text`, topic: item.topic, text: item.text }
              : { kind: `picture`, attachmentId: item.entry.attachmentId }
          ),
          reply: turn.reply,
        })),
      }).toEqual(c.expected)
    })
  }
})

describe(`sessionTurns files (wave D)`, () => {
  it(`carries a message's file links only when it has any`, () => {
    const turns = sessionTurns([], [
      {
        kind: `user_message`,
        at: 1,
        text: `see`,
        files: [{ name: `build.log`, url: `/api/attachments/f1` }],
      },
      { kind: `user_message`, at: 2, text: `plain` },
    ])
    expect(turns.turns[0]!.message).toEqual({
      text: `see`,
      at: 1,
      images: [],
      files: [{ name: `build.log`, url: `/api/attachments/f1` }],
    })
    expect(turns.turns[1]!.message).toEqual({ text: `plain`, at: 2, images: [] })
  })
})
