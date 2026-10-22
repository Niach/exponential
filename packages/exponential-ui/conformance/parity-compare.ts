// FEED-128: the PURE half of the web parity check (parity.ts drives the
// browser): the serialised DOM shape, generated-id normalisation, and the
// first-difference finders for DOM, computed style and frames. No browser,
// no I/O: parity.test.ts locks them on hand-written inputs.

/** One serialised DOM node: an element (`t` tag, `a` attributes sorted by
 *  name, `c` children) or a text node (`x`). Comments are dropped. */
export type DomNode = { t: string; a: [string, string][]; c: DomNode[] } | { x: string }

/** The attributes whose values are element ids (space-separated lists for
 *  the ARIA relations). Only these are normalised. */
export const ID_ATTRIBUTES = new Set([`id`, `for`, `name`, `aria-controls`, `aria-labelledby`, `aria-describedby`, `aria-owns`, `aria-activedescendant`, `aria-errormessage`, `aria-details`, `aria-flowto`, `list`, `form`])

/** A GENERATED id token: React `useId` (`:r1:`, `«r1»`, `_r_1_`), anything
 *  Radix derives (`radix-…`), Solid / Kobalte `createUniqueId` (`cl-12…`). */
export const GENERATED_ID_PATTERNS: readonly RegExp[] = [/radix-/, /:r[0-9a-z]+:/, /«r[0-9a-z]+»/, /(^|[^0-9A-Za-z])_r_[0-9a-z]+_/, /(^|[^0-9A-Za-z])cl-\d+/]

export const isGeneratedId = (token: string): boolean => GENERATED_ID_PATTERNS.some((re) => re.test(token))

/** A copy of the tree with every generated id token in an id attribute
 *  replaced by `#id-N`, N = first-seen order (document order, attributes by
 *  name): the same token always maps to the same N, so a relation still
 *  points at its target after normalisation. */
export function normalizeIds(roots: DomNode[]): DomNode[] {
  const seen = new Map<string, string>()
  const map = (token: string) => {
    if (!isGeneratedId(token)) return token
    let out = seen.get(token)
    if (!out) seen.set(token, (out = `#id-${seen.size + 1}`))
    return out
  }
  const walk = (n: DomNode): DomNode => {
    if (`x` in n) return n
    return { t: n.t, a: n.a.map(([k, v]) => [k, ID_ATTRIBUTES.has(k) ? v.split(/(\s+)/).map((p) => (p.trim() ? map(p) : p)).join(``) : v]), c: n.c.map(walk) }
  }
  return roots.map(walk)
}

export interface DomDiff {
  /** `0/div[0]/span[2]` (surface index, then element children by tag +
   *  index among ALL children), `@attr` / `#text` for the differing part. */
  path: string
  reference: string
  candidate: string
}

const describe = (n: DomNode | undefined): string => (n === undefined ? `(nothing)` : `x` in n ? `#text ${JSON.stringify(n.x)}` : `<${n.t}>`)

/** The FIRST difference between two serialised trees in document order. */
export function firstDomDiff(reference: DomNode[], candidate: DomNode[]): DomDiff | undefined {
  const visit = (a: DomNode | undefined, b: DomNode | undefined, path: string): DomDiff | undefined => {
    if (a === undefined || b === undefined || `x` in a !== `x` in b) return { path, reference: describe(a), candidate: describe(b) }
    if (`x` in a && `x` in b) return a.x === b.x ? undefined : { path: `${path}#text`, reference: a.x, candidate: b.x }
    const ea = a as Extract<DomNode, { t: string }>
    const eb = b as Extract<DomNode, { t: string }>
    if (ea.t !== eb.t) return { path, reference: describe(ea), candidate: describe(eb) }
    const names = [...new Set([...ea.a.map(([k]) => k), ...eb.a.map(([k]) => k)])].sort()
    const av = new Map(ea.a)
    const bv = new Map(eb.a)
    for (const k of names) if (av.get(k) !== bv.get(k)) return { path: `${path}@${k}`, reference: av.has(k) ? av.get(k)! : `(absent)`, candidate: bv.has(k) ? bv.get(k)! : `(absent)` }
    for (let i = 0; i < Math.max(ea.c.length, eb.c.length); i++) {
      const child = ea.c[i] ?? eb.c[i]!
      const d = visit(ea.c[i], eb.c[i], `${path}/${`x` in child ? `#text` : child.t}[${i}]`)
      if (d) return d
    }
    return undefined
  }
  for (let i = 0; i < Math.max(reference.length, candidate.length); i++) {
    const d = visit(reference[i], candidate[i], String(i))
    if (d) return d
  }
  return undefined
}

/** One compared element: its key (`data-xui-id`, or `<owner id>/<part>` for
 *  an overlay part without its own id), the computed-style values in
 *  `StyleTable.properties` order, and its frame. */
export interface ElementProbe {
  key: string
  values: string[]
  frame: { x: number; y: number; w: number; h: number }
}

export interface StyleTable {
  properties: string[]
  elements: ElementProbe[]
}

export interface StyleDiff {
  id: string
  property: string
  reference: string
  candidate: string
}

export interface FrameDiff {
  id: string
  key: `x` | `y` | `w` | `h` | `element`
  reference: number | string
  candidate: number | string
}

/** The element lists must line up key for key; the first mismatch is the
 *  difference (`property: "(element)"`). */
function elementMismatch(a: StyleTable, b: StyleTable): { index: number; reference: string; candidate: string } | undefined {
  for (let i = 0; i < Math.max(a.elements.length, b.elements.length); i++) {
    const ka = a.elements[i]?.key
    const kb = b.elements[i]?.key
    if (ka !== kb) return { index: i, reference: ka ?? `(nothing)`, candidate: kb ?? `(nothing)` }
  }
  return undefined
}

/** The FIRST computed-style difference (element order, then property order). */
export function firstStyleDiff(a: StyleTable, b: StyleTable): StyleDiff | undefined {
  const props = [...new Set([...a.properties, ...b.properties])]
  const ia = new Map(a.properties.map((p, i) => [p, i]))
  const ib = new Map(b.properties.map((p, i) => [p, i]))
  const n = Math.min(a.elements.length, b.elements.length)
  const mismatch = elementMismatch(a, b)
  const upto = mismatch ? mismatch.index : n
  for (let i = 0; i < upto; i++) {
    const ea = a.elements[i]!
    const eb = b.elements[i]!
    for (const p of props) {
      const va = ia.has(p) ? (ea.values[ia.get(p)!] ?? `(absent)`) : `(absent)`
      const vb = ib.has(p) ? (eb.values[ib.get(p)!] ?? `(absent)`) : `(absent)`
      if (va !== vb) return { id: ea.key, property: p, reference: va, candidate: vb }
    }
  }
  if (mismatch) return { id: `#${mismatch.index}`, property: `(element)`, reference: mismatch.reference, candidate: mismatch.candidate }
  return undefined
}

/** The FIRST frame difference, exact (0 px tolerance). */
export function firstFrameDiff(a: StyleTable, b: StyleTable): FrameDiff | undefined {
  const mismatch = elementMismatch(a, b)
  const upto = mismatch ? mismatch.index : Math.min(a.elements.length, b.elements.length)
  for (let i = 0; i < upto; i++) {
    const fa = a.elements[i]!.frame
    const fb = b.elements[i]!.frame
    for (const k of [`x`, `y`, `w`, `h`] as const) if (fa[k] !== fb[k]) return { id: a.elements[i]!.key, key: k, reference: fa[k], candidate: fb[k] }
  }
  if (mismatch) return { id: `#${mismatch.index}`, key: `element`, reference: mismatch.reference, candidate: mismatch.candidate }
  return undefined
}

/** A case id → a file-name-safe slug. */
export const caseSlug = (id: string): string => id.replace(/[^A-Za-z0-9._-]+/g, `_`)

// ---------------------------------------------------------------------------
// The report (parity-report.schema.json)
// ---------------------------------------------------------------------------

export const PARITY_FORMAT = `xui-web-parity/1`

export interface ParityCaseResult {
  case: string
  ok: boolean
  elapsedMs: number
  /** A page error, a timeout: the case could not be compared. */
  error?: string
  dom?: DomDiff
  style?: StyleDiff
  frame?: FrameDiff
  pixels?: { differing: number; size: string; referencePng?: string; candidatePng?: string; diffPng?: string }
}

export interface ParityReport {
  format: typeof PARITY_FORMAT
  reference: string
  candidate: string
  chromium: string
  startedAt: string
  elapsedMs: number
  summary: { cases: number; passed: number; failed: number; byGroup: Record<string, { cases: number; passed: number }> }
  cases: ParityCaseResult[]
}

/** Every problem of a parity report (empty = parity). */
export function checkParityReport(report: ParityReport): string[] {
  const problems: string[] = []
  if (report.format !== PARITY_FORMAT) problems.push(`format ${String(report.format)}, want ${PARITY_FORMAT}`)
  if (!Array.isArray(report.cases) || report.cases.length === 0) problems.push(`no cases`)
  for (const c of report.cases ?? []) {
    if (c.ok) continue
    const what = c.error ? `error ${c.error.split(`\n`)[0]}` : [c.dom && `dom ${c.dom.path}`, c.style && `style ${c.style.id} ${c.style.property}`, c.frame && `frame ${c.frame.id}.${c.frame.key}`, c.pixels?.differing && `${c.pixels.differing} px`].filter(Boolean).join(`, `)
    problems.push(`${c.case}: ${what}`)
  }
  return problems
}
