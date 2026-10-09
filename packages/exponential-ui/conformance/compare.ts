// The COMPARATOR of the conformance harness: two frame dumps (a reference —
// the committed web baseline — and a candidate: a fresh web dump, the gpui
// dump…) node by node, matched by id.
//
// Tolerances (fixtures/conformance-cases.json `tolerance`): x, y, w, h within
// ±`px`; a node with text lines on either side may differ in height by up to
// `textLines` × its line height (one more or fewer wrapped line). Lines that
// differ are ALWAYS reported (`wrap`), tolerated when within `textLines`.
// Every node outside tolerance is a divergence:
//   size      its own w/h differ
//   position  only x/y differ
//   wrap      its line count differs by more than `textLines`
// and an ORIGIN when nothing below explains it (a size divergence with no
// size-diverging descendant; a position divergence whose parent is in
// tolerance) — the list to fix; the rest are its cascade. Nodes only one side
// placed are reported apart (`onlyRef` / `onlyCand`): the dumps' coverage
// differs there (e.g. a control the core expands into parts the DOM renders
// without `data-xui-id`), nothing was compared.
//
// The Rust gate (apps/desktop/crates/exponential-ui-gpui/tests/conformance.rs)
// implements the same rules.

import type { CaseDump, Dump, DumpNode } from "./dump"

export interface Tolerance {
  px: number
  textLines: number
}

export type DivergenceKind = `size` | `position` | `wrap`

export interface NodeDiff {
  id: string
  component: string
  kinds: DivergenceKind[]
  origin: boolean
  ref: Pick<DumpNode, `x` | `y` | `w` | `h` | `lines` | `lh`>
  cand: Pick<DumpNode, `x` | `y` | `w` | `h` | `lines` | `lh`>
  d: { x: number; y: number; w: number; h: number }
  text?: string
}

export interface CaseReport {
  key: string
  matched: number
  within: number
  size: number
  position: number
  /** Wrap divergences (beyond `textLines`). */
  wrap: number
  /** Line-count differences within tolerance (reported, not divergences). */
  wrapTolerated: number
  origins: number
  maxDelta: number
  onlyRef: string[]
  onlyCand: string[]
  height: { ref: number; cand: number }
  diffs: NodeDiff[]
  /** Tolerated line-count changes (`id: ref → cand lines`). */
  rewraps: string[]
}

export interface Report {
  ref: string
  cand: string
  tolerance: Tolerance
  cases: CaseReport[]
  missingCases: string[]
}

const box = (n: DumpNode) => ({ x: n.x, y: n.y, w: n.w, h: n.h, ...(n.lines !== undefined ? { lines: n.lines, lh: n.lh } : {}) })
const r2 = (v: number) => Math.round(v * 100) / 100

export function compareCase(key: string, ref: CaseDump, cand: CaseDump, tol: Tolerance): CaseReport {
  const refById = new Map<string, DumpNode>()
  for (const n of ref.nodes) if (!refById.has(n.id)) refById.set(n.id, n)
  const candById = new Map<string, DumpNode>()
  for (const n of cand.nodes) if (!candById.has(n.id)) candById.set(n.id, n)
  const report: CaseReport = { key, matched: 0, within: 0, size: 0, position: 0, wrap: 0, wrapTolerated: 0, origins: 0, maxDelta: 0, onlyRef: [], onlyCand: [], height: { ref: ref.height, cand: cand.height }, diffs: [], rewraps: [] }
  const diffById = new Map<string, NodeDiff>()
  for (const [id, a] of refById) {
    const b = candById.get(id)
    if (!b) {
      report.onlyRef.push(id)
      continue
    }
    report.matched++
    const d = { x: r2(b.x - a.x), y: r2(b.y - a.y), w: r2(b.w - a.w), h: r2(b.h - a.h) }
    const lines = a.lines !== undefined || b.lines !== undefined
    const lh = Math.max(a.lh ?? 0, b.lh ?? 0)
    const hTol = lines ? Math.max(tol.px, tol.textLines * lh) : tol.px
    const kinds: DivergenceKind[] = []
    if (Math.abs(d.w) > tol.px || Math.abs(d.h) > hTol) kinds.push(`size`)
    else if (Math.abs(d.x) > tol.px || Math.abs(d.y) > tol.px) kinds.push(`position`)
    if (a.lines !== undefined && b.lines !== undefined && a.lines !== b.lines) {
      if (Math.abs(a.lines - b.lines) > tol.textLines) kinds.push(`wrap`)
      else {
        report.wrapTolerated++
        report.rewraps.push(`${id}: ${a.lines} → ${b.lines} lines`)
      }
    }
    report.maxDelta = Math.max(report.maxDelta, Math.abs(d.x), Math.abs(d.y), Math.abs(d.w), Math.abs(d.h))
    if (!kinds.length) {
      report.within++
      continue
    }
    const diff: NodeDiff = { id, component: a.component, kinds, origin: false, ref: box(a), cand: box(b), d }
    if (a.text ?? b.text) diff.text = a.text ?? b.text
    diffById.set(id, diff)
    report.diffs.push(diff)
  }
  for (const id of candById.keys()) if (!refById.has(id)) report.onlyCand.push(id)
  // Origins: a size divergence no diverging descendant explains; a position
  // divergence whose parent is in tolerance.
  const sizeBelow = new Set<string>()
  for (const diff of report.diffs) {
    if (!diff.kinds.includes(`size`) && !diff.kinds.includes(`wrap`)) continue
    let p = refById.get(diff.id)?.parent
    while (p) {
      sizeBelow.add(p)
      p = refById.get(p)?.parent
    }
  }
  for (const diff of report.diffs) {
    const parent = refById.get(diff.id)?.parent
    if (diff.kinds.includes(`size`) || diff.kinds.includes(`wrap`)) diff.origin = !sizeBelow.has(diff.id)
    else diff.origin = !parent || !diffById.has(parent)
    if (diff.kinds.includes(`size`)) report.size++
    if (diff.kinds.includes(`position`)) report.position++
    if (diff.kinds.includes(`wrap`)) report.wrap++
    if (diff.origin) report.origins++
  }
  return report
}

export function compareDumps(ref: Dump, cand: Dump, tol: Tolerance, keys?: string[]): Report {
  const cases: CaseReport[] = []
  const missingCases: string[] = []
  for (const key of keys ?? Object.keys(ref.cases)) {
    const a = ref.cases[key]
    const b = cand.cases[key]
    if (!a || !b) {
      missingCases.push(key)
      continue
    }
    cases.push(compareCase(key, a, b, tol))
  }
  return { ref: ref.renderer, cand: cand.renderer, tolerance: tol, cases, missingCases }
}

/** Divergences of a case (matched nodes outside tolerance). */
export const divergences = (c: CaseReport) => c.diffs.length

const pad = (s: string | number, n: number, right = false) => (right ? String(s).padStart(n) : String(s).padEnd(n))

/** The summary table: one row per case. */
export function formatTable(report: Report): string {
  const head = [pad(`case`, 44), pad(`matched`, 7, true), pad(`ok`, 5, true), pad(`size`, 5, true), pad(`pos`, 5, true), pad(`wrap`, 5, true), pad(`±1ln`, 5, true), pad(`origin`, 6, true), pad(`only ref`, 8, true), pad(`only cand`, 9, true), pad(`max Δ`, 8, true), pad(`height ref/cand`, 17, true)].join(` `)
  const rows = report.cases.map((c) =>
    [pad(c.key, 44), pad(c.matched, 7, true), pad(c.within, 5, true), pad(c.size, 5, true), pad(c.position, 5, true), pad(c.wrap, 5, true), pad(c.wrapTolerated, 5, true), pad(c.origins, 6, true), pad(c.onlyRef.length, 8, true), pad(c.onlyCand.length, 9, true), pad(c.maxDelta.toFixed(1), 8, true), pad(`${Math.round(c.height.ref)}/${Math.round(c.height.cand)}`, 17, true)].join(` `)
  )
  const t = report.cases.reduce((s, c) => ({ matched: s.matched + c.matched, within: s.within + c.within, size: s.size + c.size, position: s.position + c.position, wrap: s.wrap + c.wrap, tol: s.tol + c.wrapTolerated, origins: s.origins + c.origins, onlyRef: s.onlyRef + c.onlyRef.length, onlyCand: s.onlyCand + c.onlyCand.length }), { matched: 0, within: 0, size: 0, position: 0, wrap: 0, tol: 0, origins: 0, onlyRef: 0, onlyCand: 0 })
  const total = [pad(`TOTAL (${report.cases.length} cases)`, 44), pad(t.matched, 7, true), pad(t.within, 5, true), pad(t.size, 5, true), pad(t.position, 5, true), pad(t.wrap, 5, true), pad(t.tol, 5, true), pad(t.origins, 6, true), pad(t.onlyRef, 8, true), pad(t.onlyCand, 9, true), pad(``, 8), pad(``, 17)].join(` `)
  const lines = [`${report.ref} (ref) vs ${report.cand} (cand) — ±${report.tolerance.px}px, wrapped text ±${report.tolerance.textLines} line`, head, `-`.repeat(head.length), ...rows, `-`.repeat(head.length), total]
  if (report.missingCases.length) lines.push(`missing cases: ${report.missingCases.join(`, `)}`)
  return lines.join(`\n`)
}

const fmtBox = (b: NodeDiff[`ref`]) => `${b.x},${b.y} ${b.w}×${b.h}${b.lines !== undefined ? ` ${b.lines}ln@${b.lh}` : ``}`

/** One case's divergences: origins first (the list to fix), then a count of
 *  the cascade, the tolerated rewraps and the unmatched ids. */
export function formatCase(c: CaseReport, limit = 60): string {
  const out = [`## ${c.key}`]
  const origins = c.diffs.filter((d) => d.origin)
  for (const d of origins.slice(0, limit)) out.push(`  ${d.kinds.join(`+`).padEnd(13)} ${d.id} (${d.component}) ref ${fmtBox(d.ref)} → cand ${fmtBox(d.cand)}  Δ ${d.d.x},${d.d.y} ${d.d.w}×${d.d.h}${d.text ? `  "${d.text.slice(0, 40)}"` : ``}`)
  if (origins.length > limit) out.push(`  … ${origins.length - limit} more origins`)
  const cascade = c.diffs.length - origins.length
  if (cascade) out.push(`  + ${cascade} cascaded (moved/resized by the origins above)`)
  if (c.rewraps.length) out.push(`  rewrapped within tolerance: ${c.rewraps.slice(0, 12).join(`; `)}${c.rewraps.length > 12 ? ` … +${c.rewraps.length - 12}` : ``}`)
  if (c.onlyRef.length) out.push(`  only in ref (${c.onlyRef.length}): ${c.onlyRef.slice(0, 12).join(`, `)}${c.onlyRef.length > 12 ? ` …` : ``}`)
  if (c.onlyCand.length) out.push(`  only in cand (${c.onlyCand.length}): ${c.onlyCand.slice(0, 12).join(`, `)}${c.onlyCand.length > 12 ? ` …` : ``}`)
  return out.join(`\n`)
}

/** Divergences grouped ACROSS cases: the same node diverging the same way
 *  in many cases is one finding (`id kinds → case count, Δ range`). */
export function groupFindings(report: Report): { id: string; component: string; kinds: string; cases: number; dw: [number, number]; dh: [number, number]; text?: string; example: string }[] {
  const groups = new Map<string, { id: string; component: string; kinds: string; cases: number; dw: [number, number]; dh: [number, number]; text?: string; example: string }>()
  for (const c of report.cases)
    for (const d of c.diffs) {
      if (!d.origin) continue
      const k = `${d.id}|${d.kinds.join(`+`)}`
      const g = groups.get(k) ?? { id: d.id, component: d.component, kinds: d.kinds.join(`+`), cases: 0, dw: [Infinity, -Infinity] as [number, number], dh: [Infinity, -Infinity] as [number, number], text: d.text, example: `${c.key}: ref ${fmtBox(d.ref)} → cand ${fmtBox(d.cand)}` }
      g.cases++
      g.dw = [Math.min(g.dw[0], d.d.w), Math.max(g.dw[1], d.d.w)]
      g.dh = [Math.min(g.dh[0], d.d.h), Math.max(g.dh[1], d.d.h)]
      groups.set(k, g)
    }
  return [...groups.values()].sort((a, b) => b.cases - a.cases || a.id.localeCompare(b.id))
}
