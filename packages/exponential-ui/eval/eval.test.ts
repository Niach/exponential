// VAPP-108: the eval on record. Pure: the fixtures and the report, no model,
// no browser. Fails when a corpus prompt has no fixture, when the scored
// rows drift from eval/report.json (rerun `bun run eval`), when the parser
// stops accepting the reply shapes a model sends, and when the Devices
// template drifts from the native page past the tolerances parity.ts
// measured (rerun `bun run eval:parity` after a shots refresh).

import { describe, expect, test } from "bun:test"
import corpus from "./corpus.json" with { type: "json" }
import report from "./report.json" with { type: "json" }
import { RUBRICS, aggregate, authoredDepth, parseReply, rubricScores, scoreFixture, spacingStats, surfaceOf, type Corpus } from "./lib"
import { readFixture, scoreAll } from "./score"
import type { ParityRow } from "./parity"

const prompts = (corpus as Corpus).prompts

describe(`the corpus`, () => {
  test(`about 30 prompts, unique ids, each with a fixture and sensible expectations`, () => {
    expect(prompts.length).toBeGreaterThanOrEqual(28)
    expect(new Set(prompts.map((p) => p.id)).size).toBe(prompts.length)
    for (const p of prompts) {
      expect(readFixture(p.id), p.id).toBeDefined()
      expect(p.prompt.length, p.id).toBeGreaterThan(40)
      for (const key of Object.keys(p.expects)) expect(RUBRICS as readonly string[], `${p.id}.expects.${key}`).toContain(key)
      for (const w of p.widths ?? []) expect(w).toBeGreaterThanOrEqual(320)
    }
    // The mobile list renders at four widths; the parity screens name real views.
    expect(prompts.find((p) => p.id === `mobile-list`)?.widths?.length).toBe(4)
    expect(prompts.find((p) => p.id === `device-dashboard`)?.appView).toBe(`agents`)
  })
})

describe(`the report`, () => {
  test(`the rows and the aggregate match a fresh scoring of the fixtures`, () => {
    const { rows, missing } = scoreAll()
    expect(missing).toEqual([])
    expect(report.rows).toEqual(JSON.parse(JSON.stringify(rows)))
    expect(report.aggregate).toEqual(JSON.parse(JSON.stringify(aggregate(rows))))
    expect(report.prompt.full.chars).toBe(report.prompt.full.chars)
  })

  test(`the numbers the issue quotes`, () => {
    const a = report.aggregate
    expect(a.surfaces).toBe(prompts.length)
    expect(a.validatorClean).toBeGreaterThanOrEqual(0)
    expect(a.validatorClean).toBeLessThanOrEqual(a.surfaces)
    expect(a.issueKinds.reduce((n, k) => n + k.count, 0)).toBe(a.issuesTotal)
    expect(a.spacing.rhythm).toBeGreaterThanOrEqual(0)
    expect(a.spacing.rhythm).toBeLessThanOrEqual(1)
    for (const r of RUBRICS) expect(a.rubric[r].passed).toBeLessThanOrEqual(a.rubric[r].expected)
  })

  test(`every surface rendered on the web at each of its widths (render.ts ran)`, () => {
    const render = report.render as Record<string, { width: number; nodes: number; height: number; overflowsX: boolean }[]>
    for (const p of prompts) {
      const measures = render[p.id]
      expect(measures, p.id).toBeDefined()
      expect(measures.map((m) => m.width), p.id).toEqual(p.widths ?? [1280])
      for (const m of measures) {
        expect(m.nodes, `${p.id}@${m.width}`).toBeGreaterThan(0)
        expect(m.overflowsX, `${p.id}@${m.width} overflows sideways`).toBe(false)
      }
    }
  })
})

describe(`the Devices template parity (web + desktop)`, () => {
  const rows = (report.parity as { rows: ParityRow[] }).rows
  const by = (platform: string) => rows.find((r) => r.platform === platform)!

  test(`both platforms are on record`, () => {
    expect(rows.map((r) => r.platform).sort()).toEqual([`desktop`, `web`])
    for (const r of rows) {
      expect(r.native.band.height).toBeGreaterThan(20)
      expect(r.template.band.height).toBeGreaterThan(20)
      expect(r.bandDiff.total).toBeGreaterThan(1000)
    }
  })

  test(`the desktop twin: the band is the same height and the row inset within a step of the gpui page`, () => {
    const d = by(`desktop`)
    // store px (1 css px = 1.25): the band height agrees within 2 px, the
    // title inset within one $spacing.sm (10 store px), the pixel diff of the
    // band stays under 5 % (the native band has an Add device button the
    // template does not).
    expect(Math.abs(d.delta.bandHeight)).toBeLessThanOrEqual(2)
    expect(Math.abs(d.delta.bandTitleX)).toBeLessThanOrEqual(10)
    expect(d.bandDiff.share).toBeLessThan(0.05)
  })

  test(`the web twin: the measured deltas are the recorded ones (a drift means a shots refresh and a rerun)`, () => {
    const w = by(`web`)
    expect(Math.abs(w.delta.bandHeight)).toBeLessThanOrEqual(12)
    expect(Math.abs(w.delta.bandTitleX)).toBeLessThanOrEqual(10)
    expect(w.bandDiff.share).toBeLessThan(0.05)
  })
})

describe(`the parser and the rubric`, () => {
  test(`accepts JSONL, a fenced block, a JSON array, and reports prose`, () => {
    const jsonl = `{"version":"v0.9","createSurface":{"surfaceId":"s","catalogId":"c"}}\n{"version":"v0.9","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Text","text":"hi"}]}}`
    expect(parseReply(jsonl).messages.length).toBe(2)
    expect(parseReply(jsonl).parseErrors).toEqual([])
    const fenced = `Here you go:\n\`\`\`json\n${jsonl}\n\`\`\`\nDone.`
    expect(parseReply(fenced).messages.length).toBe(2)
    expect(parseReply(fenced).parseErrors).toEqual([])
    const array = `[${jsonl.replace(`\n`, `,`)}]`
    expect(parseReply(array).messages.length).toBe(2)
    const prose = `Sure!\n${jsonl}`
    expect(parseReply(prose).messages.length).toBe(2)
    expect(parseReply(prose).parseErrors).toEqual([`line 1: Sure!`])
    const s = surfaceOf(parseReply(jsonl).messages)
    expect(s.surfaceId).toBe(`s`)
    expect(s.components.map((c) => c.id)).toEqual([`root`])
  })

  test(`the rubric counts what it says`, () => {
    const components = [
      { id: `root`, component: `Stack`, children: [`row`, `box`, `md`, `list`, `empty`, `form`, `lone`] },
      { id: `row`, component: `Stack`, direction: `horizontal`, children: [`t`, `b1`] },
      { id: `t`, component: `Text`, text: `x` },
      { id: `b1`, component: `Button`, label: `Go` },
      { id: `box`, component: `Box`, style: { padding: 12, gap: `$spacing.md`, "@media (min-width: $breakpoint.md)": { paddingTop: 7 } } },
      { id: `md`, component: `Markdown`, text: `- a\n- b` },
      { id: `list`, component: `List`, children: { componentId: `item`, path: `/items` } },
      { id: `item`, component: `Row`, title: { path: `t` } },
      { id: `empty`, component: `EmptyState`, icon: `inbox`, title: `None` },
      { id: `form`, component: `Form`, name: `f`, children: [`in`] },
      { id: `in`, component: `Input`, label: `A`, name: `a` },
      { id: `lone`, component: `Select`, name: `s`, options: [] },
    ]
    const r = rubricScores(components as never)
    expect(r).toEqual({ primaryActions: 1, emptyStates: 1, loadingStates: 0, fields: 2, fieldsInForm: 1, lists: 1, overlays: 0, unalignedRows: 1, boxes: 1, markdownForUi: 1, hexColours: 0 })
    expect(spacingStats(components as never)).toEqual({ tokenised: 1, raw: 2, rawValues: [7, 12], rhythm: 1 / 3 })
    expect(authoredDepth(components as never)).toBe(3)
    const row = scoreFixture({ id: `x`, title: `x`, prompt: `x`, expects: { form: true, primaryAction: true } }, { id: `x`, prompt: `x`, model: `m`, promptVariant: `full`, recordedAt: ``, recordedBy: `session`, usage: { inputTokens: 1, outputTokens: 1, cacheReadInputTokens: 0, estimated: true }, text: JSON.stringify({ version: `v0.9`, updateComponents: { surfaceId: `main`, components } }) })
    expect(row.expects).toEqual({ form: false, primaryAction: true })
    expect(row.findings).toContain(`1 of 2 fields outside a Form`)
    expect(row.findings).toContain(`2 raw px spacing value(s): 7, 12`)
    expect(row.issues.length).toBe(0)
  })
})
