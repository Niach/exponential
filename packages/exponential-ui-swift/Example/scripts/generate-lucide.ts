// EXP-1555: the Lucide geometry the SwiftUI painter draws instead of SF
// Symbols, so iOS shows the glyphs web, desktop and Android show.
//
//   bun packages/exponential-ui-swift/Example/scripts/generate-lucide.ts   (from the repo root)
//
// Reads lucide-react's `__iconNode` (dist/esm/icons/<name>.js, the version
// the web renders) and icons.json's `custom` nodes, normalises every element
// (circle, ellipse, rect, line, polyline, polygon, path with relative
// commands, H/V, S/T and arcs) to absolute M/L/C/Q/Z in the 24-unit box, and
// writes:
//   Sources/ExponentialUI/Paint/BuiltinGlyphs.generated.swift
//     the glyphs `BuiltinIcons.lucide` names (read from ConceptIcon.swift)
//     plus the `circle-dashed` placeholder;
//   Example/KitchenSink/LucideIcons.swift
//     the example host's registry: every icons.json concept, every catalog
//     ICON_NAMES entry and every icon the specimens name.
// A path string = space-separated tokens; a leading `F` = filled only
// (stroke none), `B` = filled AND stroked, else stroked.
import { existsSync, readFileSync, writeFileSync } from "fs"
import { join, resolve } from "path"

const root = resolve(import.meta.dir, "../../../..")
const pkg = join(root, "packages/exponential-ui-swift")
const lucideDir = join(root, "node_modules/lucide-react/dist/esm/icons")
const icons = JSON.parse(readFileSync(join(root, "packages/icons/icons.json"), "utf8"))

type Node = [string, Record<string, string>]

// #EXP-1764: icons.json `imported` marks (`brand-claude`, `brand-codex`),
// already scaled into the 24-unit box by packages/icons.
const importedNodes: Record<string, Node[]> = Object.fromEntries(
  [...readFileSync(join(root, "packages/exponential-ui/src/web/custom-icons.generated.ts"), "utf8").matchAll(/^  "([^"]+)": (\[\[.*\]\]),$/gm)]
    .filter(([, name]) => name in (icons.imported ?? {}))
    .map(([, name, node]) => [name, JSON.parse(node) as Node[]])
)

function lucideNode(name: string): Node[] | undefined {
  if (icons.custom?.[name]) return icons.custom[name]
  if (importedNodes[name]) return importedNodes[name]
  const file = join(lucideDir, `${name}.js`)
  if (!existsSync(file)) return undefined
  const src = readFileSync(file, "utf8")
  const m = src.match(/const __iconNode = (\[[\s\S]*?\]);\n/)
  if (!m) throw new Error(`no __iconNode in ${file}`)
  return new Function(`return ${m[1]}`)() as Node[]
}

const fmt = (n: number) => {
  const r = Math.round(n * 1000) / 1000
  return Object.is(r, -0) ? `0` : `${r}`
}

class Out {
  parts: string[] = []
  cmd(c: string, ...xs: number[]) {
    this.parts.push(c, ...xs.map(fmt))
  }
}

// SVG arc (endpoint form) → cubic Béziers, ≤ 90° each.
function arcToCubics(out: Out, x1: number, y1: number, rx: number, ry: number, phi: number, large: boolean, sweep: boolean, x2: number, y2: number) {
  if (rx === 0 || ry === 0) return out.cmd(`L`, x2, y2)
  if (x1 === x2 && y1 === y2) return
  rx = Math.abs(rx)
  ry = Math.abs(ry)
  const p = (phi * Math.PI) / 180
  const cos = Math.cos(p), sin = Math.sin(p)
  const dx = (x1 - x2) / 2, dy = (y1 - y2) / 2
  const x1p = cos * dx + sin * dy, y1p = -sin * dx + cos * dy
  const lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry)
  if (lambda > 1) {
    rx *= Math.sqrt(lambda)
    ry *= Math.sqrt(lambda)
  }
  const num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p
  const den = rx * rx * y1p * y1p + ry * ry * x1p * x1p
  let co = Math.sqrt(Math.max(0, num / den))
  if (large === sweep) co = -co
  const cxp = (co * rx * y1p) / ry, cyp = (-co * ry * x1p) / rx
  const cx = cos * cxp - sin * cyp + (x1 + x2) / 2
  const cy = sin * cxp + cos * cyp + (y1 + y2) / 2
  const ang = (ux: number, uy: number, vx: number, vy: number) => {
    const a = Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy)
    return a
  }
  const t1 = ang(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry)
  let dt = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry)
  if (!sweep && dt > 0) dt -= 2 * Math.PI
  if (sweep && dt < 0) dt += 2 * Math.PI
  const n = Math.max(1, Math.ceil(Math.abs(dt) / (Math.PI / 2) - 1e-9))
  const seg = dt / n
  const k = (4 / 3) * Math.tan(seg / 4)
  const pt = (t: number) => [cx + rx * Math.cos(t) * cos - ry * Math.sin(t) * sin, cy + rx * Math.cos(t) * sin + ry * Math.sin(t) * cos]
  const dpt = (t: number) => [-rx * Math.sin(t) * cos - ry * Math.cos(t) * sin, -rx * Math.sin(t) * sin + ry * Math.cos(t) * cos]
  for (let i = 0; i < n; i++) {
    const a = t1 + i * seg, b = a + seg
    const [ax, ay] = pt(a), [bx, by] = pt(b)
    const [dax, day] = dpt(a), [dbx, dby] = dpt(b)
    const end = i === n - 1 ? [x2, y2] : [bx, by]
    out.cmd(`C`, ax + k * dax, ay + k * day, bx - k * dbx, by - k * dby, end[0], end[1])
  }
}

function tokenize(d: string): string[] {
  return d.match(/[a-zA-Z]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?/g) ?? []
}

function pathD(d: string, out: Out) {
  const t = tokenize(d)
  let i = 0, cmd = ``
  let x = 0, y = 0, sx = 0, sy = 0
  let lc: [number, number] | null = null // last cubic ctrl2
  let lq: [number, number] | null = null // last quad ctrl
  const n = () => parseFloat(t[i++])
  const flag = () => {
    // Arc flags may be packed ("011"): take one digit.
    const s = t[i]
    if (s.length > 1 && (s[0] === `0` || s[0] === `1`) && !s.startsWith(`0.`)) {
      t[i] = s.slice(1)
      return s[0] === `1`
    }
    i++
    return s === `1`
  }
  while (i < t.length) {
    if (/[a-zA-Z]/.test(t[i])) cmd = t[i++]
    const rel = cmd === cmd.toLowerCase()
    const ox = rel ? x : 0, oy = rel ? y : 0
    switch (cmd.toUpperCase()) {
      case `M`:
        x = ox + n(); y = oy + n(); sx = x; sy = y
        out.cmd(`M`, x, y)
        cmd = rel ? `l` : `L`
        lc = lq = null
        break
      case `L`:
        x = ox + n(); y = oy + n(); out.cmd(`L`, x, y); lc = lq = null
        break
      case `H`:
        x = (rel ? x : 0) + n(); out.cmd(`L`, x, y); lc = lq = null
        break
      case `V`:
        y = (rel ? y : 0) + n(); out.cmd(`L`, x, y); lc = lq = null
        break
      case `C`: {
        const c1x = ox + n(), c1y = oy + n(), c2x = ox + n(), c2y = oy + n()
        x = ox + n(); y = oy + n()
        out.cmd(`C`, c1x, c1y, c2x, c2y, x, y); lc = [c2x, c2y]; lq = null
        break
      }
      case `S`: {
        const c1x = lc ? 2 * x - lc[0] : x, c1y = lc ? 2 * y - lc[1] : y
        const c2x = ox + n(), c2y = oy + n()
        x = ox + n(); y = oy + n()
        out.cmd(`C`, c1x, c1y, c2x, c2y, x, y); lc = [c2x, c2y]; lq = null
        break
      }
      case `Q`: {
        const cx = ox + n(), cy = oy + n()
        x = ox + n(); y = oy + n()
        out.cmd(`Q`, cx, cy, x, y); lq = [cx, cy]; lc = null
        break
      }
      case `T`: {
        const cx = lq ? 2 * x - lq[0] : x, cy = lq ? 2 * y - lq[1] : y
        x = ox + n(); y = oy + n()
        out.cmd(`Q`, cx, cy, x, y); lq = [cx, cy]; lc = null
        break
      }
      case `A`: {
        const rx = n(), ry = n(), phi = n(), large = flag(), sweep = flag()
        const ex = ox + n(), ey = oy + n()
        arcToCubics(out, x, y, rx, ry, phi, large, sweep, ex, ey)
        x = ex; y = ey; lc = lq = null
        break
      }
      case `Z`:
        out.cmd(`Z`); x = sx; y = sy; lc = lq = null
        break
      default:
        throw new Error(`unsupported path command ${cmd} in ${d}`)
    }
  }
}

const num = (a: Record<string, string>, k: string, d = 0) => (a[k] === undefined ? d : parseFloat(a[k]))

function ellipse(out: Out, cx: number, cy: number, rx: number, ry: number) {
  out.cmd(`M`, cx + rx, cy)
  arcToCubics(out, cx + rx, cy, rx, ry, 0, false, true, cx, cy + ry)
  arcToCubics(out, cx, cy + ry, rx, ry, 0, false, true, cx - rx, cy)
  arcToCubics(out, cx - rx, cy, rx, ry, 0, false, true, cx, cy - ry)
  arcToCubics(out, cx, cy - ry, rx, ry, 0, false, true, cx + rx, cy)
  out.cmd(`Z`)
}

function element([tag, a]: Node): string {
  const out = new Out()
  switch (tag) {
    case `path`: pathD(a.d, out); break
    case `circle`: ellipse(out, num(a, `cx`), num(a, `cy`), num(a, `r`), num(a, `r`)); break
    case `ellipse`: ellipse(out, num(a, `cx`), num(a, `cy`), num(a, `rx`), num(a, `ry`)); break
    case `line`: out.cmd(`M`, num(a, `x1`), num(a, `y1`)); out.cmd(`L`, num(a, `x2`), num(a, `y2`)); break
    case `polyline`:
    case `polygon`: {
      const p = (a.points.match(/[-+]?(?:\d*\.\d+|\d+\.?)/g) ?? []).map(parseFloat)
      for (let i = 0; i + 1 < p.length; i += 2) out.cmd(i === 0 ? `M` : `L`, p[i], p[i + 1])
      if (tag === `polygon`) out.cmd(`Z`)
      break
    }
    case `rect`: {
      const x = num(a, `x`), y = num(a, `y`), w = num(a, `width`), h = num(a, `height`)
      let rx = a.rx !== undefined ? num(a, `rx`) : num(a, `ry`)
      let ry = a.ry !== undefined ? num(a, `ry`) : rx
      rx = Math.min(rx, w / 2); ry = Math.min(ry, h / 2)
      if (rx <= 0 || ry <= 0) {
        out.cmd(`M`, x, y); out.cmd(`L`, x + w, y); out.cmd(`L`, x + w, y + h); out.cmd(`L`, x, y + h); out.cmd(`Z`)
      } else {
        out.cmd(`M`, x + rx, y)
        out.cmd(`L`, x + w - rx, y)
        arcToCubics(out, x + w - rx, y, rx, ry, 0, false, true, x + w, y + ry)
        out.cmd(`L`, x + w, y + h - ry)
        arcToCubics(out, x + w, y + h - ry, rx, ry, 0, false, true, x + w - rx, y + h)
        out.cmd(`L`, x + rx, y + h)
        arcToCubics(out, x + rx, y + h, rx, ry, 0, false, true, x, y + h - ry)
        out.cmd(`L`, x, y + ry)
        arcToCubics(out, x, y + ry, rx, ry, 0, false, true, x + rx, y)
        out.cmd(`Z`)
      }
      break
    }
    default:
      throw new Error(`unsupported element ${tag}`)
  }
  const filled = a.fill !== undefined && a.fill !== `none`
  const stroked = a.stroke !== `none`
  const prefix = filled ? (stroked ? `B ` : `F `) : ``
  return prefix + out.parts.join(` `)
}

function glyph(name: string): string[] {
  const node = lucideNode(name)
  if (!node) throw new Error(`no Lucide glyph "${name}"`)
  return node.map(element)
}

const swiftString = (s: string) => JSON.stringify(s)
function table(name: string, keys: string[], indent = `    `): string {
  const rows = keys.map((k) => `${indent}    ${swiftString(k)}: [${glyph(k).map(swiftString).join(`, `)}],`)
  return `${indent}static let ${name}: [String: [String]] = [\n${rows.join(`\n`)}\n${indent}]`
}

// 1. The painter's built-in glyphs: every name `BuiltinIcons.lucide` returns.
const conceptIcon = readFileSync(join(pkg, `Sources/ExponentialUI/Views/Leaves/ConceptIcon.swift`), `utf8`)
const lucideFn = conceptIcon.match(/static func lucide\([\s\S]*?\n    }\n/)
if (!lucideFn) throw new Error(`BuiltinIcons.lucide not found in ConceptIcon.swift`)
const builtin = new Set<string>([`circle-dashed`])
for (const m of lucideFn[0].matchAll(/: "([a-z0-9-]+)"\s*$/gm)) builtin.add(m[1])
const builtinKeys = [...builtin].sort()
writeFileSync(
  join(pkg, `Sources/ExponentialUI/Paint/BuiltinGlyphs.generated.swift`),
  `// GENERATED by Example/scripts/generate-lucide.ts (lucide-react ${JSON.parse(readFileSync(join(root, `node_modules/lucide-react/package.json`), `utf8`)).version}); do not edit.
// The Lucide glyphs the painter draws for its built-in names (EXP-1555):
// absolute M/L/C/Q/Z in the 24-unit box; \`F\` = filled, \`B\` = filled and stroked.

enum BuiltinGlyphs {
${table(`paths`, builtinKeys)}
}
`,
)

// 2. The example host's registry.
const concepts: Record<string, string> = icons.semantic
const catalog = readFileSync(join(root, `packages/exponential-ui/src/catalog.generated.ts`), `utf8`)
const iconNames: string[] = JSON.parse(catalog.match(/export const ICON_NAMES = (\[.*?\]) as const/)![1])
const specimens = JSON.parse(readFileSync(join(root, `packages/exponential-ui/fixtures/specimens.json`), `utf8`))
const named = new Set<string>()
const walk = (o: unknown, key = ``) => {
  if (typeof o === `string`) {
    if (key === `icon` || key === `name`) named.add(o)
  } else if (Array.isArray(o)) o.forEach((x) => walk(x, key))
  else if (o && typeof o === `object`) for (const [k, v] of Object.entries(o)) walk(v, k)
}
walk(specimens)
const conceptMap: Record<string, string> = {}
const glyphs = new Set<string>()
const consider = (name: string) => {
  const target = concepts[name] ?? name
  if (!lucideNode(target)) return
  glyphs.add(target)
  if (target !== name) conceptMap[name] = target
}
for (const name of [...Object.keys(concepts), ...iconNames, ...named, ...Object.keys(icons.custom ?? {})]) consider(name)
const glyphKeys = [...glyphs].sort()
writeFileSync(
  join(pkg, `Example/KitchenSink/LucideIcons.swift`),
  `// GENERATED by Example/scripts/generate-lucide.ts; do not edit.
// The example host's icon registry (EXP-1555): every icons.json concept, the
// catalog's ICON_NAMES and the specimens' icons as Lucide geometry (what the
// Exponential app's registry draws), so the kitchen sink looks like web.

enum LucideIcons {
    /// A concept → its Lucide (or icons.json \`custom\`) glyph name.
    static let concepts: [String: String] = [
${Object.keys(conceptMap).sort().map((k) => `        ${swiftString(k)}: ${swiftString(conceptMap[k])},`).join(`\n`)}
    ]

    /// The paths of a concept or glyph name.
    static func paths(_ name: String) -> [String]? { glyphs[concepts[name] ?? name] }

${table(`glyphs`, glyphKeys)}
}
`,
)
console.log(`built-in glyphs: ${builtinKeys.length}; kitchen sink: ${Object.keys(conceptMap).length} concepts, ${glyphKeys.length} glyphs`)
