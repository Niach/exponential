// The conformance harness's pure parts: the matrix, the font set, the
// comparator's tolerance/origin rules, the baseline codec and the committed
// baseline + ratchet covering exactly the matrix. The tolerance cases are
// mirrored by `compare_rules` in apps/desktop/crates/exponential-ui-gpui/
// tests/conformance.rs (the Rust port of compareCase).

import { describe, expect, it } from "bun:test"
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { decodeBaseline, encodeBaseline, type Baseline } from "./baseline"
import { compareCase, compareDumps, formatCase, formatTable, groupFindings } from "./compare"
import { allCases, BASELINE_PATH, caseInput, caseKey, dropCollapsed, fontFaceCss, fontFiles, KNOWN_PATH, MANIFEST_PATH, parseCaseKey, type CaseDump, type ConformanceManifest, type Dump, type DumpNode, type FontManifest } from "./dump"

const repoRoot = join(import.meta.dir, `..`, `..`, `..`)
const readRepo = (p: string) => JSON.parse(readFileSync(join(repoRoot, p), `utf8`)) as unknown
const manifest = readRepo(MANIFEST_PATH) as ConformanceManifest
const fonts = readRepo(manifest.fonts) as FontManifest
const tol = { px: 1, textLines: 1 }

const node = (id: string, x: number, y: number, w: number, h: number, extra: Partial<DumpNode> = {}): DumpNode => ({ id, component: `Box`, x, y, w, h, ...extra })
const dump = (...nodes: DumpNode[]): CaseDump => ({ width: 400, height: 400, nodes })

describe(`the matrix`, () => {
  it(`is fixtures × themes × modes × widths × directions, keyed like the Rust side`, () => {
    const cases = allCases(manifest)
    expect(cases.length).toBe(Object.keys(manifest.fixtures).length * manifest.themes.length * manifest.modes.length * manifest.widths.length * manifest.directions.length)
    expect(cases[0].key).toBe(caseKey(cases[0]))
    expect(parseCaseKey(`kitchen-sink/neutral/dark/390/rtl`)).toEqual({ key: `kitchen-sink/neutral/dark/390/rtl`, fixture: `kitchen-sink`, theme: `neutral`, mode: `dark`, width: 390, direction: `rtl` })
    expect(manifest.widths).toEqual([390, 600, 900, 1280])
    expect(manifest.themes.sort()).toEqual([`exponential`, `neutral`, `playful`])
  })

  it(`builds each case's input: the root direction is the case's, the data without $comment`, async () => {
    const read = async (p: string) => readRepo(p)
    for (const key of [`kitchen-sink/exponential/dark/390/rtl`, `responsive/playful/dark/1280/ltr`]) {
      const c = parseCaseKey(key)
      const { tree, data } = await caseInput(manifest, c, read)
      expect((tree.style as Record<string, unknown>).direction).toBe(c.direction)
      expect(`$comment` in data).toBe(false)
      expect(Object.keys(data).length).toBeGreaterThan(0)
    }
  })
})

describe(`the font set`, () => {
  it(`names committed files and maps every theme family`, () => {
    for (const f of fontFiles(fonts)) expect(existsSync(join(repoRoot, f)), f).toBe(true)
    for (const id of manifest.themes) {
      const theme = readRepo(`packages/exponential-ui/themes/${id}.theme.json`) as { tokens: { type: { family: Record<string, string> } } }
      for (const family of Object.values(theme.tokens.type.family)) expect(fonts.families[family], `${id}: ${family}`).toBeDefined()
    }
    for (const spec of Object.values(fonts.families)) if (spec.substitute) expect(fonts.families[spec.substitute]?.faces?.length).toBeGreaterThan(0)
  })

  it(`declares substitutes under the substituted name; Nunito and Fira Code are real faces (round 2)`, () => {
    const css = fontFaceCss(fonts, (f) => `/repo/${f}`)
    expect(css).toContain(`font-family: "ui-monospace"; src: url("/repo/apps/desktop/assets/fonts/JetBrainsMono-Regular.ttf")`)
    expect(css).toContain(`font-family: "Nunito"; src: url("/repo/packages/exponential-ui/conformance/fonts/Nunito-Regular.ttf")`)
    expect(css).toContain(`font-family: "Fira Code"; src: url("/repo/packages/exponential-ui/conformance/fonts/FiraCode-Regular.ttf")`)
    expect(fonts.families.Nunito.substitute).toBeUndefined()
    expect(fonts.families[`Fira Code`].substitute).toBeUndefined()
    expect(css).toContain(`font-weight: 600`)
  })
})

describe(`dropCollapsed`, () => {
  it(`drops 0×0 subtrees in any order, keeps 0×0 parents of placed nodes and thin boxes`, () => {
    const nodes = [node(`leaf`, 0, 0, 10, 10, { parent: `zeroParent` }), node(`root`, 0, 0, 100, 100), node(`zeroParent`, 0, 0, 0, 0, { parent: `root` }), node(`gone`, 5, 5, 0, 0, { parent: `root` }), node(`goneChild`, 5, 5, 0, 0, { parent: `gone` }), node(`rule`, 0, 0, 100, 0, { parent: `root` })]
    expect(dropCollapsed(nodes).map((n) => n.id)).toEqual([`leaf`, `root`, `zeroParent`, `rule`])
  })
})

describe(`compareCase`, () => {
  it(`identical dumps compare clean`, () => {
    const a = dump(node(`root`, 0, 0, 400, 100), node(`a`, 10, 10, 50, 20, { parent: `root` }))
    const r = compareCase(`k`, a, structuredClone(a), tol)
    expect(r).toMatchObject({ matched: 2, within: 2, size: 0, position: 0, wrap: 0, origins: 0 })
  })

  it(`±1 px is within tolerance, more is a divergence`, () => {
    const a = dump(node(`root`, 0, 0, 400, 100), node(`a`, 10, 10, 50, 20, { parent: `root` }))
    expect(compareCase(`k`, a, dump(node(`root`, 0, 0, 400, 100), node(`a`, 11, 9, 51, 21, { parent: `root` })), tol).diffs).toEqual([])
    const r = compareCase(`k`, a, dump(node(`root`, 0, 0, 400, 100), node(`a`, 11.5, 10, 50, 20, { parent: `root` })), tol)
    expect(r.diffs.map((d) => [d.id, d.kinds, d.origin])).toEqual([[`a`, [`position`], true]])
  })

  it(`origins: the deepest size divergence; moved children of a moved parent cascade`, () => {
    const a = dump(node(`root`, 0, 0, 400, 100), node(`card`, 0, 0, 400, 60, { parent: `root` }), node(`text`, 0, 0, 100, 20, { parent: `card` }), node(`below`, 0, 60, 400, 40, { parent: `root` }), node(`belowChild`, 0, 60, 10, 10, { parent: `below` }))
    const b = dump(node(`root`, 0, 0, 400, 120), node(`card`, 0, 0, 400, 80, { parent: `root` }), node(`text`, 0, 0, 100, 40, { parent: `card` }), node(`below`, 0, 80, 400, 40, { parent: `root` }), node(`belowChild`, 0, 80, 10, 10, { parent: `below` }))
    const r = compareCase(`k`, a, b, tol)
    const origin = Object.fromEntries(r.diffs.map((d) => [d.id, d.origin]))
    // `below` moved because its sibling grew inside a diverging parent: cascade.
    expect(origin).toEqual({ root: false, card: false, text: true, below: false, belowChild: false })
    expect(r.size).toBe(3)
    expect(r.position).toBe(2)
    expect(r.origins).toBe(1)
    // A node that moved while its parent stayed put is an origin.
    const moved = compareCase(`k`, a, dump(node(`root`, 0, 0, 400, 100), node(`card`, 0, 0, 400, 60, { parent: `root` }), node(`text`, 0, 0, 100, 20, { parent: `card` }), node(`below`, 0, 64, 400, 40, { parent: `root` }), node(`belowChild`, 0, 64, 10, 10, { parent: `below` })), tol)
    expect(moved.diffs.map((d) => [d.id, d.origin])).toEqual([[`below`, true], [`belowChild`, false]])
  })

  it(`text: one more line is tolerated and reported, two is a wrap divergence`, () => {
    const t = (lines: number) => dump(node(`t`, 0, 0, 200, 20 * lines, { component: `Text`, lines, lh: 20 }))
    const one = compareCase(`k`, t(2), t(3), tol)
    expect(one.diffs).toEqual([])
    expect(one.wrapTolerated).toBe(1)
    expect(one.rewraps).toEqual([`t: 2 → 3 lines`])
    const two = compareCase(`k`, t(2), t(4), tol)
    expect(two.diffs.map((d) => d.kinds)).toEqual([[`size`, `wrap`]])
    expect(two.wrap).toBe(1)
  })

  it(`nodes only one side placed are coverage, not divergences`, () => {
    const r = compareCase(`k`, dump(node(`a`, 0, 0, 1, 1), node(`webOnly`, 0, 0, 1, 1)), dump(node(`a`, 0, 0, 1, 1), node(`gpuiOnly`, 0, 0, 1, 1)), tol)
    expect(r.onlyRef).toEqual([`webOnly`])
    expect(r.onlyCand).toEqual([`gpuiOnly`])
    expect(r.diffs).toEqual([])
  })

  it(`reports and groups across cases`, () => {
    const a: Dump = { format: `xui-frame-dump/1`, renderer: `web`, fonts: ``, cases: { x: dump(node(`n`, 0, 0, 10, 10)), y: dump(node(`n`, 0, 0, 10, 10)) } }
    const b: Dump = { format: `xui-frame-dump/1`, renderer: `gpui`, fonts: ``, cases: { x: dump(node(`n`, 0, 0, 20, 10)), y: dump(node(`n`, 0, 0, 30, 10)) } }
    const report = compareDumps(a, b, tol)
    expect(groupFindings(report)).toMatchObject([{ id: `n`, kinds: `size`, cases: 2, dw: [10, 20] }])
    expect(formatTable(report)).toContain(`TOTAL (2 cases)`)
    expect(formatCase(report.cases[0])).toContain(`size          n (Box)`)
    expect(compareDumps(a, { ...b, cases: { x: b.cases.x } }, tol).missingCases).toEqual([`y`])
  })
})

describe(`the baseline`, () => {
  it(`round-trips a dump`, () => {
    const d: Dump = { format: `xui-frame-dump/1`, renderer: `react-chromium`, fonts: `f`, cases: { "kitchen-sink/a/dark/390/ltr": dump(node(`root`, 0, 0, 390, 50), node(`t`, 1.5, 2, 3, 20, { component: `Text`, parent: `root`, part: `P/t`, text: `hi`, lines: 1, lh: 20 })), "kitchen-sink/a/dark/600/ltr": dump(node(`root`, 0, 0, 600, 50)) } }
    expect(decodeBaseline(JSON.parse(encodeBaseline(d, `test`)) as Baseline)).toEqual(d)
  })

  it(`is committed for exactly the matrix, with a desktop ratchet for every case`, () => {
    const keys = allCases(manifest)
      .map((c) => c.key)
      .sort()
    const b = decodeBaseline(readRepo(BASELINE_PATH) as Baseline)
    expect(Object.keys(b.cases).sort()).toEqual(keys)
    for (const k of keys) expect(b.cases[k].nodes.length, k).toBeGreaterThan(10)
    const known = readRepo(KNOWN_PATH) as { cases: Record<string, Record<string, number>> }
    expect(Object.keys(known.cases).sort()).toEqual(keys)
  })
})
