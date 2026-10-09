// The FRAME DUMP format of the renderer conformance harness: every placed
// node of one case (fixture × theme × mode × width × direction) as
// {id, component, part, parent, x, y, w, h, text?, lines?, lh?}, positions
// relative to the surface's root node, in CSS px. Produced by the React
// renderer in headless Chromium (conformance/dom.ts, real fonts) and by the
// gpui painter headless (apps/desktop/crates/exponential-ui-gpui/examples/
// conformance_dump.rs, gpui's cosmic-text system with the same font files);
// conformance/compare.ts diffs two of them. The case matrix and the fixtures
// live in fixtures/conformance-cases.json, the font set in conformance/fonts.json.

export const DUMP_FORMAT = `xui-frame-dump/1`

export interface DumpNode {
  id: string
  component: string
  part?: string
  /** The nearest placed ancestor's id. */
  parent?: string
  x: number
  y: number
  w: number
  h: number
  /** The text the node lays out itself (leaves; ≤ 80 chars, informational). */
  text?: string
  /** Laid-out text lines (text components only: `textComponents`). */
  lines?: number
  /** The text's line height (px). */
  lh?: number
}

export interface CaseDump {
  width: number
  height: number
  nodes: DumpNode[]
}

export interface Dump {
  format: string
  /** `react-chromium` | `gpui-cosmic-headless` | … */
  renderer: string
  /** The font files the dump was laid out with. */
  fonts: string
  cases: Record<string, CaseDump>
}

export interface ConformanceManifest {
  format: number
  fonts: string
  fixtures: Record<string, { tree?: string; data?: string; geometry?: string }>
  themes: string[]
  modes: string[]
  widths: number[]
  directions: (`ltr` | `rtl`)[]
  locale: string
  textComponents: string[]
  tolerance: { px: number; textLines: number }
}

export interface FontFace {
  file: string
  weight: number
  style: `normal` | `italic`
}

export interface FontManifest {
  default: string
  families: Record<string, { faces?: FontFace[]; substitute?: string }>
  lastResort?: { families: string[] }
}

export interface ConformanceCase {
  key: string
  fixture: string
  theme: string
  mode: `light` | `dark`
  width: number
  direction: `ltr` | `rtl`
}

export const MANIFEST_PATH = `packages/exponential-ui/fixtures/conformance-cases.json`
export const BASELINE_PATH = `packages/exponential-ui/fixtures/conformance-baseline.json`
export const KNOWN_PATH = `packages/exponential-ui/fixtures/conformance-known.json`

/** `<fixture>/<theme>/<mode>/<width>/<direction>` (the Rust side builds the same). */
export const caseKey = (c: Omit<ConformanceCase, `key`>) => `${c.fixture}/${c.theme}/${c.mode}/${c.width}/${c.direction}`

export function parseCaseKey(key: string): ConformanceCase {
  const [fixture, theme, mode, width, direction] = key.split(`/`)
  return { key, fixture, theme, mode: mode as `light` | `dark`, width: Number(width), direction: direction as `ltr` | `rtl` }
}

/** Every case of the matrix, in the manifest's order (the Rust order too). */
export function allCases(m: ConformanceManifest): ConformanceCase[] {
  const out: ConformanceCase[] = []
  for (const fixture of Object.keys(m.fixtures))
    for (const theme of m.themes)
      for (const mode of m.modes)
        for (const width of m.widths)
          for (const direction of m.directions) {
            const c = { fixture, theme, mode: mode as `light` | `dark`, width, direction }
            out.push({ key: caseKey(c), ...c })
          }
  return out
}

/** The case's nested tree (root `direction` = the case's) and data model;
 *  `read` loads a repo-relative JSON file (fetch in the page, fs in Bun). */
export async function caseInput(m: ConformanceManifest, c: ConformanceCase, read: (path: string) => Promise<unknown>): Promise<{ tree: Record<string, unknown>; data: Record<string, unknown> }> {
  const spec = m.fixtures[c.fixture]
  if (!spec) throw new Error(`unknown conformance fixture ${c.fixture}`)
  let tree: Record<string, unknown>
  let data: Record<string, unknown> = {}
  if (spec.geometry) {
    const g = (await read(spec.geometry)) as { surface: Record<string, unknown>; data?: Record<string, unknown> }
    tree = structuredClone(g.surface)
    data = structuredClone(g.data ?? {})
  } else {
    tree = structuredClone((await read(spec.tree!)) as Record<string, unknown>)
    if (spec.data) {
      const { $comment: _c, ...rest } = (await read(spec.data)) as Record<string, unknown>
      data = rest
    }
  }
  tree.style = { ...((tree.style as Record<string, unknown> | undefined) ?? {}), direction: c.direction }
  return { tree, data }
}

/** The @font-face rules of the font set (`url(path)` → `src`). Substitutes
 *  are declared under the substituted name with the substitute's files. */
export function fontFaceCss(fonts: FontManifest, url: (file: string) => string): string {
  const rules: string[] = []
  for (const [family, spec] of Object.entries(fonts.families)) {
    const faces = spec.faces ?? (spec.substitute ? fonts.families[spec.substitute]?.faces : undefined) ?? []
    for (const f of faces) rules.push(`@font-face { font-family: "${family}"; src: url("${url(f.file)}") format("truetype"); font-weight: ${f.weight}; font-style: ${f.style}; font-display: block; }`)
  }
  return rules.join(`\n`)
}

/** Every font file the set names (for the server's allowlist). */
export const fontFiles = (fonts: FontManifest) => Object.values(fonts.families).flatMap((f) => (f.faces ?? []).map((x) => x.file))

/** The one placement rule both producers apply: a node whose box is 0×0
 *  together with every descendant's is NOT placed (the web drops
 *  `display: none` outright, the core gives such subtrees 0×0 frames). */
export function dropCollapsed(nodes: DumpNode[]): DumpNode[] {
  const parentOf = new Map(nodes.map((n) => [n.id, n.parent]))
  const visible = new Set<string>()
  for (const n of nodes) {
    if (n.w <= 0 && n.h <= 0) continue
    // The node and every ancestor are placed (any order of `nodes`).
    for (let id: string | undefined = n.id; id && !visible.has(id); id = parentOf.get(id)) visible.add(id)
  }
  return nodes.filter((n) => visible.has(n.id))
}

const r2 = (v: number) => Math.round(v * 100) / 100

/** Round a dump's numbers to 1/100 px (both producers do). */
export function roundNode(n: DumpNode): DumpNode {
  const out: DumpNode = { ...n, x: r2(n.x), y: r2(n.y), w: r2(n.w), h: r2(n.h) }
  if (n.lh !== undefined) out.lh = r2(n.lh)
  return out
}
