// #EXP-2204: fixtures/ci-log.json replays (eui-core, Swift and Kotlin replay
// the same file), the constants equal catalog/style.json, the status table
// equals the neutral theme's recipes, and the hand-checked rules.

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/ci-log.json"
import styleJson from "../catalog/style.json" with { type: "json" }
import stringsJson from "../catalog/strings.json" with { type: "json" }
import neutral from "../themes/neutral.theme.json" with { type: "json" }
import { ansiRuns } from "./code"
import { coreCatalog } from "./catalog"
import { loadTheme, resolveRecipe, resolveToken } from "./theme"
import type { ThemeSource } from "./theme-types"
import {
  CI_STATUSES,
  CI_STATUS_SPIN_MS,
  CI_STATUS_TABLE,
  blameBlocks,
  ciStatusEntry,
  extendLineAnchor,
  formatLineAnchor,
  logModel,
  logOpenDefault,
  logReveal,
  logRows,
  logSearch,
  parseLineAnchor,
  stripAnsi,
} from "./ci-log"

const plain = (v: unknown) => JSON.parse(JSON.stringify(v))

describe(`fixtures/ci-log.json`, () => {
  test(`the status table`, () => {
    expect(fixture.statusTable.spinMs).toBe(CI_STATUS_SPIN_MS)
    for (const row of fixture.statusTable.rows) {
      const { input, ...entry } = row
      expect(ciStatusEntry(input), String(input)).toEqual(entry as never)
    }
  })

  test(`anchors: parse, format, extend`, () => {
    expect(fixture.anchors.parse.length).toBeGreaterThan(20)
    for (const c of fixture.anchors.parse) expect(parseLineAnchor(c.text), String(c.text)).toEqual(c.range as never)
    for (const c of fixture.anchors.format) expect(formatLineAnchor(c.start, (c as { end?: number }).end)).toBe(c.anchor)
    for (const c of fixture.anchors.extend) expect(extendLineAnchor(c.current, c.line, c.shift), JSON.stringify(c)).toBe(c.anchor)
  })

  test(`logs: model, default open set, rows, search, reveal`, () => {
    expect(fixture.logs.length).toBeGreaterThan(5)
    for (const c of fixture.logs) {
      const model = logModel(c.lines, (c as { sections?: unknown[] }).sections)
      expect(plain(model), c.name).toEqual(c.model)
      expect(logOpenDefault(model), c.name).toEqual(c.openDefault)
      for (const r of c.rows) expect(logRows(model, r.open), `${c.name} rows ${JSON.stringify(r.open)}`).toEqual(r.rows as never)
      for (const s of c.search) expect(logSearch(model, s.query), `${c.name} search ${s.query}`).toEqual(s.matches)
      for (const r of c.reveal) expect(logReveal(model, r.open, r.line), `${c.name} reveal ${r.line}`).toEqual(r.result)
    }
  })

  test(`blame blocks`, () => {
    for (const c of fixture.blame) expect(blameBlocks(c.blame, c.lineCount), c.name).toEqual(c.lines)
  })
})

describe(`the CI status table`, () => {
  test(`is style.json ciStatus and the ciStatus enum`, () => {
    expect([...CI_STATUSES]).toEqual([...coreCatalog.enums.ciStatus!])
    expect(styleJson.ciStatus.spinMs).toBe(CI_STATUS_SPIN_MS)
    for (const status of CI_STATUSES) {
      const style = (styleJson.ciStatus as unknown as Record<string, { icon: string; spin: boolean }>)[status]!
      expect({ icon: CI_STATUS_TABLE[status].icon, spin: CI_STATUS_TABLE[status].spin }, status).toEqual(style)
      expect((stringsJson.strings as Record<string, string>)[CI_STATUS_TABLE[status].label], status).toBeDefined()
    }
  })

  test(`the neutral theme paints the table's colours, cancelled and skipped text muted, no strikethrough`, () => {
    const theme = loadTheme(neutral as unknown as ThemeSource)
    for (const status of CI_STATUSES) {
      const entry = CI_STATUS_TABLE[status]
      for (const [component, mark, text] of [[`LogLines`, `sectionIcon`, `sectionTitle`], [`JobGraph`, `icon`, `name`]] as const) {
        for (const mode of [`light`, `dark`] as const) {
          const markStyle = resolveRecipe(theme, { component, part: mark, props: { status }, states: [] }, mode)
          const textStyle = resolveRecipe(theme, { component, part: text, props: { status }, states: [] }, mode)
          expect(markStyle.color, `${component}/${mark} ${status} ${mode}`).toBe(resolveToken(theme, entry.color, mode) as never)
          expect(textStyle.color, `${component}/${text} ${status} ${mode}`).toBe(resolveToken(theme, entry.text, mode) as never)
          expect(textStyle.textDecoration, `${component}/${text} ${status}`).toBeUndefined()
        }
      }
    }
  })
})

describe(`the log model`, () => {
  test(`stripAnsi = the CodeBlock ansi tokenizer's escape-free text`, () => {
    for (const s of [`\u001b[31mred\u001b[0m`, `a\u001b[1;32mb\u001b[mc`, `\u001b[31`, `\u001b]0;t\u0007x`, `plain`]) expect(stripAnsi(s)).toBe(ansiRuns(s).map((r) => r.text).join(``))
  })

  test(`markers get no number; a timestamp is cut; prefixes colour the line`, () => {
    const m = logModel([`2026-11-27T09:00:01.5Z ##[group]Build`, `2026-11-27T09:00:02Z ##[warning]slow`, `##[endgroup]`, `after`])
    expect(m.lines.map((l) => [l.n, l.section, l.time, l.kind, l.text])).toEqual([[1, 0, `09:00:02`, `warning`, `slow`], [2, -1, null, null, `after`]])
    expect(m.sections).toEqual([{ index: 0, title: `Build`, start: 1, end: 1, count: 1, status: `passed`, duration: null }])
  })

  test(`an unclosed last group runs and opens; search folds ASCII only`, () => {
    const m = logModel([`##[group]A`, `x`, `##[group]B`, `Straße`])
    expect(m.sections.map((s) => s.status)).toEqual([`passed`, `running`])
    expect(logOpenDefault(m)).toEqual([1])
    expect(logRows(m, [])).toEqual([{ kind: `section`, section: 0 }, { kind: `section`, section: 1 }])
    expect(logSearch(m, `STRASSE`)).toEqual([])
    expect(logSearch(m, `STRAßE`)).toEqual([{ line: 2, start: 0, end: 6 }])
    expect(logReveal(m, [], 1)).toEqual([0])
  })
})
