// VAPP-108: the eval on record. Pure: the fixtures and the report, no model,
// no browser. Fails when a corpus prompt has no fixture, when the scored
// rows drift from eval/report.json (rerun `bun run eval`), when the parser
// stops accepting the reply shapes a model sends, when the Solid or element
// render of a surface differs from the React one beyond the findings on record (rerun
// `bun run eval:render`), and when the Devices template drifts from the
// native page past the tolerances parity.ts measured (rerun
// `bun run eval:parity` after a shots refresh).

import { describe, expect, test } from "bun:test"
import corpus from "./corpus.json" with { type: "json" }
import report from "./report.json" with { type: "json" }
import { RUBRICS, aggregate, authoredDepth, parseReply, rubricScores, scoreFixture, spacingStats, surfaceOf, type Corpus } from "./lib"
import specimens from "./eval-specimens.json" with { type: "json" }
import { buildSpecimens, readFixture, scoreAll } from "./score"
import type { NativeMeasure, NativesReport } from "./natives"
import type { ParityRow, SurfaceParityRow } from "./parity"
import { CANDIDATES, RENDERERS, type Candidate, type RenderMeasure } from "./render"

/** Where a renderer differs from React on the corpus, by surface and
 *  candidate: the findings the eval produced (the web-parity gate had no
 *  case for them). */
const RENDER_FINDINGS: Record<string, Partial<Record<Candidate, RegExp[]>>> = {}

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

  test(`eval/eval-specimens.json is every fixture as a native specimen (rerun \`bun run eval\`)`, () => {
    expect(specimens).toEqual(JSON.parse(JSON.stringify(buildSpecimens())))
    expect(specimens.specimens.map((s) => s.corpusId)).toEqual(prompts.map((p) => p.id))
    for (const s of specimens.specimens) expect(s.id).toBe(`exponential-ui-eval-${s.corpusId}`)
  })

  test(`the numbers the issue quotes`, () => {
    const a = report.aggregate
    expect(a.surfaces).toBe(prompts.length)
    expect(a.validatorClean).toBeGreaterThanOrEqual(0)
    expect(a.validatorClean).toBeLessThanOrEqual(a.surfaces)
    expect((a.issueKinds as { count: number }[]).reduce((n, k) => n + k.count, 0)).toBe(a.issuesTotal)
    expect(a.spacing.rhythm).toBeGreaterThanOrEqual(0)
    expect(a.spacing.rhythm).toBeLessThanOrEqual(1)
    for (const r of RUBRICS) expect(a.rubric[r].passed).toBeLessThanOrEqual(a.rubric[r].expected)
  })

  test(`every surface rendered on the web with React, Solid and the element at each of its widths (render.ts ran)`, () => {
    const render = report.render as unknown as Record<string, RenderMeasure[]>
    for (const p of prompts) {
      const measures = render[p.id]
      expect(measures, p.id).toBeDefined()
      expect(measures.map((m) => m.width), p.id).toEqual(p.widths ?? [1280])
      for (const m of measures) {
        for (const renderer of RENDERERS) {
          expect(m[renderer].nodes, `${p.id}@${m.width} ${renderer}`).toBeGreaterThan(0)
          expect(m[renderer].overflowsX, `${p.id}@${m.width} ${renderer} overflows sideways`).toBe(false)
          expect(m[renderer].picture).toBe(`eval/out/${p.id}/${renderer}-${m.width}.png`)
        }
      }
    }
  })

  test(`Solid and the element paint what React paints: the same boxes and the same pixels, except the findings on record`, () => {
    const render = report.render as unknown as Record<string, RenderMeasure[]>
    expect(CANDIDATES).toEqual([`solid`, `element`])
    for (const p of prompts) {
      for (const m of render[p.id]) {
        for (const c of CANDIDATES) {
          const known = RENDER_FINDINGS[p.id]?.[c]
          const where = `${p.id}@${m.width} ${c}`
          if (!known) {
            expect(m.differences[c], where).toEqual([])
            expect(m.pixelDiff[c].pixels, where).toBe(0)
            expect(m[c].height, where).toBe(m.react.height)
            expect(m[c].gaps, where).toEqual(m.react.gaps)
            continue
          }
          // A finding stays on record until the renderer is fixed and render.ts rerun (this list then shrinks).
          for (const re of known) expect(m.differences[c].some((d) => re.test(d)), `${where}: ${re} in ${m.differences[c].join(`; `)}`).toBe(true)
          expect(m.differences[c].length, where).toBe(known.length)
        }
        expect(m.equal, `${p.id}@${m.width}`).toBe(!RENDER_FINDINGS[p.id])
      }
    }
  })
})

/** Where a native paints a corpus surface with other spacing than React (VAPP-127, natives.ts): narrow-width
 *  overflows (a table or a terminal wider than the phone: the web renders those at 1280 only) and a padding
 *  the narrower layout changes. A finding stays on record until the surface or the renderer changes. */
const NATIVE_FINDINGS: Record<string, string> = {
  "issue-list-filters/android": `overflowsX at 411`,
  "api-keys/android": `overflowsX at 411`,
  "api-keys/ios": `overflowsX at 440`,
  "terminal-output/android": `overflowsX at 411`,
  "terminal-output/ios": `overflowsX at 440`,
  "pricing/ios": `textPadding: web 24, native 8`,
  "team-switcher/android": `textPadding: web 165, native 159`,
  "team-switcher/ios": `textPadding: web 165, native 163`,
}

describe(`the natives (VAPP-127: the shots pipeline's eval lane + natives.ts)`, () => {
  const natives = (report as { natives?: NativesReport }).natives!

  test(`every surface was painted and measured on android, ios and desktop`, () => {
    for (const p of prompts) {
      for (const platform of [`android`, `ios`, `desktop`] as const) {
        const m = natives.surfaces[p.id]?.[platform] as NativeMeasure | undefined
        expect(m, `${p.id}/${platform}`).toBeDefined()
        expect(m!.picture).toBe(`eval/out/${p.id}/${platform}.webp`)
        expect(m!.nodes, `${p.id}/${platform}`).toBeGreaterThan(0)
      }
    }
  })

  test(`the spacing equals React's everywhere but the findings on record`, () => {
    const found: Record<string, string> = {}
    for (const [id, byPlatform] of Object.entries(natives.surfaces))
      for (const [platform, m] of Object.entries(byPlatform)) if (m!.differences.length) found[`${id}/${platform}`] = m!.differences.join(`; `)
    expect(found).toEqual(NATIVE_FINDINGS)
  })
})

describe(`every appView surface against its app screen (VAPP-127, parity.ts surfaces)`, () => {
  const rows = (report.parity as { surfaces: SurfaceParityRow[] }).surfaces

  test(`each surface naming an appView is compared on the web and on every native whose app shot exists`, () => {
    for (const p of prompts.filter((p) => p.appView)) {
      const mine = rows.filter((r) => r.surface === p.id)
      expect(mine.map((r) => r.platform), p.id).toContain(`web`)
      expect(mine.map((r) => r.platform), p.id).toContain(`desktop`)
      for (const r of mine) {
        expect(r.appView).toBe(p.appView!)
        expect(r.sharedGaps).toBeGreaterThanOrEqual(0)
        expect(r.sharedGaps).toBeLessThanOrEqual(1)
      }
    }
    // The phones only where the app screen has a phone shot (settings pages are web + desktop).
    expect(rows.filter((r) => r.platform === `android`).length).toBeGreaterThanOrEqual(10)
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
