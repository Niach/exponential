// FEED-128: the pure comparison functions of the web parity check
// (parity-compare.ts) on hand-written inputs; no browser.

import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { caseSlug, checkParityReport, firstDomDiff, firstFrameDiff, firstStyleDiff, isGeneratedId, normalizeIds, PARITY_FORMAT, type DomNode, type ParityReport, type StyleTable } from "./parity-compare"

const el = (t: string, a: Record<string, string> = {}, c: DomNode[] = []): DomNode => ({ t, a: Object.entries(a).sort(([x], [y]) => (x < y ? -1 : 1)), c })
const text = (x: string): DomNode => ({ x })

describe(`generated ids`, () => {
  test(`React useId, Radix and Kobalte tokens are generated; renderer ids are not`, () => {
    for (const t of [`:r1:`, `:r1a:`, `«r3»`, `_r_7_`, `radix-_r_7_`, `radix-:r2:-content-a`, `cl-12`, `cl-3-trigger`]) expect(isGeneratedId(t), t).toBe(true)
    for (const t of [`xui-specimen-title`, `c0-button`, `dash-bar`, `search_r_1`, `ocl-12`, `circle-1`]) expect(isGeneratedId(t), t).toBe(false)
  })

  test(`normalised to #id-N in first-seen order, relations keep pointing at their target`, () => {
    const react = [el(`div`, { class: `xui-surface` }, [el(`button`, { "aria-controls": `radix-_r_1_`, id: `radix-_r_0_` }), el(`div`, { "aria-labelledby": `radix-_r_0_`, id: `radix-_r_1_`, role: `dialog` }), el(`label`, { for: `_r_2_ c0-input` })])]
    const solid = [el(`div`, { class: `xui-surface` }, [el(`button`, { "aria-controls": `cl-9-content`, id: `cl-9-trigger` }), el(`div`, { "aria-labelledby": `cl-9-trigger`, id: `cl-9-content`, role: `dialog` }), el(`label`, { for: `cl-14 c0-input` })])]
    const a = normalizeIds(react)
    expect(a).toEqual([el(`div`, { class: `xui-surface` }, [el(`button`, { "aria-controls": `#id-1`, id: `#id-2` }), el(`div`, { "aria-labelledby": `#id-2`, id: `#id-1`, role: `dialog` }), el(`label`, { for: `#id-3 c0-input` })])])
    expect(normalizeIds(solid)).toEqual(a)
    expect(firstDomDiff(a, normalizeIds(solid))).toBeUndefined()
  })

  test(`only id attributes are normalised (data-xui-*, classes verbatim)`, () => {
    const [n] = normalizeIds([el(`div`, { "data-xui-id": `radix-x`, class: `cl-1`, id: `cl-1` })])
    expect(n).toEqual(el(`div`, { "data-xui-id": `radix-x`, class: `cl-1`, id: `#id-1` }))
  })
})

describe(`first DOM difference`, () => {
  const base = () => [el(`div`, { class: `xui-surface`, "data-xui-surface": `s` }, [el(`style`, { "data-xui-style": `theme` }, [text(`.a{}`)]), el(`div`, { "data-xui-id": `root` }, [el(`span`, { "data-xui-part": `label` }, [text(`Hi`)])])])]
  test(`equal trees`, () => expect(firstDomDiff(base(), base())).toBeUndefined())
  test(`an attribute value, an extra attribute`, () => {
    const b = base()
    ;(b[0] as { c: DomNode[] }).c[1] = el(`div`, { "data-xui-id": `root`, "data-r-variant": `ghost` }, [el(`span`, { "data-xui-part": `label` }, [text(`Hi`)])])
    expect(firstDomDiff(base(), b)).toEqual({ path: `0/div[1]@data-r-variant`, reference: `(absent)`, candidate: `ghost` })
  })
  test(`text, then the earliest difference wins`, () => {
    const b = base()
    ;(b[0] as { c: DomNode[] }).c[0] = el(`style`, { "data-xui-style": `theme` }, [text(`.b{}`)])
    ;(b[0] as { c: DomNode[] }).c[1] = el(`p`)
    expect(firstDomDiff(base(), b)).toEqual({ path: `0/style[0]/#text[0]#text`, reference: `.a{}`, candidate: `.b{}` })
  })
  test(`a missing child, a tag, a missing surface`, () => {
    const b = base()
    ;(b[0] as { c: DomNode[] }).c.pop()
    expect(firstDomDiff(base(), b)).toEqual({ path: `0/div[1]`, reference: `<div>`, candidate: `(nothing)` })
    expect(firstDomDiff([el(`div`)], [el(`section`)])).toEqual({ path: `0`, reference: `<div>`, candidate: `<section>` })
    expect(firstDomDiff(base(), [])).toEqual({ path: `0`, reference: `<div>`, candidate: `(nothing)` })
  })
})

const table = (rows: [string, string[], [number, number, number, number]][], properties = [`color`, `display`, `width`]): StyleTable => ({ properties, elements: rows.map(([key, values, [x, y, w, h]]) => ({ key, values, frame: { x, y, w, h } })) })

describe(`style and frame differences`, () => {
  const a = table([
    [`root`, [`rgb(0, 0, 0)`, `flex`, `100px`], [0, 0, 100, 40]],
    [`btn`, [`rgb(1, 2, 3)`, `inline-flex`, `50px`], [8, 8, 50, 24]],
    [`btn/label`, [`rgb(1, 2, 3)`, `inline`, `auto`], [12, 10, 30, 20]],
  ])
  test(`equal`, () => {
    expect(firstStyleDiff(a, structuredClone(a))).toBeUndefined()
    expect(firstFrameDiff(a, structuredClone(a))).toBeUndefined()
  })
  test(`the first element, then the first property in property order`, () => {
    const b = structuredClone(a)
    b.elements[1]!.values = [`rgb(1, 2, 4)`, `flex`, `50px`]
    b.elements[2]!.values[0] = `red`
    expect(firstStyleDiff(a, b)).toEqual({ id: `btn`, property: `color`, reference: `rgb(1, 2, 3)`, candidate: `rgb(1, 2, 4)` })
  })
  test(`sub-pixel frames are differences (0 tolerance)`, () => {
    const b = structuredClone(a)
    b.elements[2]!.frame.w = 30.015625
    b.elements[1]!.frame.y = 8.5
    expect(firstFrameDiff(a, b)).toEqual({ id: `btn`, key: `y`, reference: 8, candidate: 8.5 })
  })
  test(`element lists that do not line up`, () => {
    const b = table([
      [`root`, [`rgb(0, 0, 0)`, `flex`, `100px`], [0, 0, 100, 40]],
      [`btn/label`, [`rgb(1, 2, 3)`, `inline`, `auto`], [12, 10, 30, 20]],
    ])
    expect(firstStyleDiff(a, b)).toEqual({ id: `#1`, property: `(element)`, reference: `btn`, candidate: `btn/label` })
    expect(firstFrameDiff(a, b)).toEqual({ id: `#1`, key: `element`, reference: `btn`, candidate: `btn/label` })
    // An earlier value difference still wins over a later list mismatch.
    const c = structuredClone(b)
    c.elements[0]!.values[2] = `101px`
    expect(firstStyleDiff(a, c)).toEqual({ id: `root`, property: `width`, reference: `100px`, candidate: `101px` })
  })
  test(`a property one side lacks`, () => {
    const b = { ...structuredClone(a), properties: [`color`, `display`] }
    b.elements.forEach((e) => e.values.pop())
    expect(firstStyleDiff(a, b)).toEqual({ id: `root`, property: `width`, reference: `100px`, candidate: `(absent)` })
  })
})

describe(`report`, () => {
  const report = (cases: ParityReport[`cases`]): ParityReport => ({ format: PARITY_FORMAT, reference: `react`, candidate: `solid`, chromium: `x`, startedAt: ``, elapsedMs: 0, summary: { cases: cases.length, passed: 0, failed: 0, byGroup: {} }, cases })
  test(`every case must pass`, () => {
    expect(checkParityReport(report([{ case: `a`, ok: true, elapsedMs: 1, pixels: { differing: 0, size: `1x1` } }]))).toEqual([])
    expect(checkParityReport(report([]))).toEqual([`no cases`])
    expect(checkParityReport(report([{ case: `b`, ok: false, elapsedMs: 1, frame: { id: `n`, key: `w`, reference: 1, candidate: 2 }, pixels: { differing: 3, size: `1x1` } }]))).toEqual([`b: frame n.w, 3 px`])
  })
  test(`the schema names every report field`, () => {
    const schema = JSON.parse(readFileSync(join(import.meta.dir, `parity-report.schema.json`), `utf8`)) as { required: string[]; properties: Record<string, unknown>; $defs: { case: { properties: Record<string, unknown> } } }
    expect(Object.keys(schema.properties).sort()).toEqual(Object.keys(report([])).sort())
    expect(schema.required.sort()).toEqual(Object.keys(report([])).sort())
    expect(Object.keys(schema.$defs.case.properties).sort()).toEqual([`case`, `dom`, `elapsedMs`, `error`, `frame`, `ok`, `pixels`, `style`])
  })
  test(`case slugs are file names`, () => expect(caseSlug(`matrix:kitchen-sink/neutral/dark/390/rtl`)).toBe(`matrix_kitchen-sink_neutral_dark_390_rtl`))
})
