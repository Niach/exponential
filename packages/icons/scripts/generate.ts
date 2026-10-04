#!/usr/bin/env bun
// EXP-273 — emits the shared icon registry into all four clients.
// Run from the repo root with: `bun run --filter @exp/icons generate`.
//
// Source of truth: `icons.json` (concept -> Lucide name) + the GEOMETRY that
// ships inside `lucide-react`. Every icon's `__iconNode` (the same structured
// data the web's React components render) is read straight out of
// node_modules, so the native clients can never drift from the web's art:
// there is exactly one copy of every path in the repo's dependency graph and
// four mechanical projections of it.
//
// Outputs (all committed, all regenerated wholesale):
//   web      packages/icons/src/generated.ts        name lists + typed maps
//            packages/ui/src/icons.generated.ts    IconName -> LucideIcon
//   iOS      apps/ios/Exponential/Assets.xcassets/<name>.imageset/…  (SVG + Contents.json)
//            apps/ios/ExpUI/Sources/AppIcons.generated.swift
//   Android  apps/android/…/ui/icons/ExpIcons.generated.kt  (Compose ImageVector)
//   desktop  apps/desktop/assets/icons/<name>.svg   (picked up by icon_named!)
//            apps/desktop/crates/ui/src/icons.generated.rs
//
// The desktop asset dir also holds hand-maintained brand marks (claude.svg,
// logo.svg, …). This generator only ever WRITES its own registry names — it
// never deletes anything else in that directory.

import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { splitSprite } from "./sprite"

const __dirname = dirname(fileURLToPath(import.meta.url))
const pkgRoot = join(__dirname, "..")
const repoRoot = join(pkgRoot, "..", "..")
const LUCIDE_ICONS_DIR = join(
  repoRoot,
  "node_modules/lucide-react/dist/esm/icons"
)

interface Registry {
  pickable: string[]
  semantic: Record<string, string>
  /**
   * EXP-314 — hand-authored glyphs Lucide does not ship (the fractional
   * pie-clock status icons), in the same IconNode geometry format as
   * lucide-react's `__iconNode`. They flow through the identical 4-platform
   * emit; the web output builds them with `createLucideIcon` instead of a
   * lucide-react import.
   */
  custom?: Record<string, IconNode[]>
  /**
   * EXP-924 — the device icon picker's set (device types + OS marks), byte-
   * equal to contract.json's deviceIcon.values. Append-only like `pickable`.
   */
  devicePickable: string[]
  /**
   * EXP-924 — single-path FILLED marks Lucide does not ship (the OS logos).
   * `file` is an SVG under packages/icons/ holding exactly one `<path>`; it is
   * scaled from its own viewBox into the 24-unit grid below and then flows
   * through the same 4-platform emit as a `custom` glyph.
   */
  imported?: Record<string, { file: string; source: string }>
  /**
   * The MCP catalog's REAL brand marks: selfh.st/icons LIGHT SVGs (CC-BY-4.0)
   * vendored verbatim under packages/icons/brand/. They are trademarks, so
   * they never enter the Lucide grid and are never recoloured or redrawn:
   * the web gets one React component per slug that inlines the file's own
   * elements (white fills kept), the desktop gets the file byte-for-byte as
   * `brand-<slug>.svg` + a `registry::brand` module. iOS and Android have no
   * MCP surface and receive nothing. contract.json's `mcpCatalog.servers[]
   * .mark` names these slugs.
   */
  brand?: Record<string, { file: string; source: string; owner: string }>
}

const registry: Registry = JSON.parse(
  readFileSync(join(pkgRoot, "icons.json"), "utf8")
)

// ---------------------------------------------------------------------------
// Lucide geometry
// ---------------------------------------------------------------------------

/** One SVG child element of a Lucide icon: `["path", { d: "…" }]`. */
type IconNode = [string, Record<string, string>]

function loadIconNode(name: string): IconNode[] {
  const file = join(LUCIDE_ICONS_DIR, `${name}.js`)
  if (!existsSync(file)) {
    throw new Error(
      `icons.json references "${name}", which lucide-react does not ship. ` +
        `Check the spelling against node_modules/lucide-react/dist/esm/icons/.`
    )
  }
  const src = readFileSync(file, "utf8")
  const match = src.match(/const __iconNode = (\[[\s\S]*?\]);/)
  if (!match) {
    // Deprecated Lucide names are re-export shims with no geometry of their
    // own. Registering one would give the clients an asset whose filename
    // does not match any real icon, so point the author at the canonical name
    // instead of silently following the alias.
    const alias = src.match(/from '\.\/([a-z0-9-]+)\.js'/)
    if (alias) {
      throw new Error(
        `icons.json uses "${name}", which lucide-react has deprecated. ` +
          `Use "${alias[1]}" instead.`
      )
    }
    throw new Error(`could not parse __iconNode out of ${file}`)
  }
  // The captured text is a plain JS array literal from a published package.
  return new Function(`return ${match[1]}`)() as IconNode[]
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

const num = (v: string | undefined, fallback = 0): number =>
  v === undefined ? fallback : Number.parseFloat(v)

/** Trim float noise so generated path data stays readable and stable. */
function fmt(n: number): string {
  const rounded = Number.parseFloat(n.toFixed(4))
  return String(rounded)
}

/**
 * Flatten one Lucide node to SVG path data. VectorDrawable/Compose only
 * understand `pathData`, so `<circle>`/`<line>`/`<rect>`/`<polyline>`/
 * `<ellipse>` have to become paths — `<path>` passes through untouched.
 */
function nodeToPathData([tag, attrs]: IconNode): string {
  switch (tag) {
    case "path":
      return attrs.d
    case "line":
      return `M${fmt(num(attrs.x1))} ${fmt(num(attrs.y1))}L${fmt(num(attrs.x2))} ${fmt(num(attrs.y2))}`
    case "polyline":
    case "polygon": {
      const pts = attrs.points.trim().split(/[\s,]+/).map(Number)
      let d = ""
      for (let i = 0; i < pts.length; i += 2) {
        d += `${i === 0 ? "M" : "L"}${fmt(pts[i])} ${fmt(pts[i + 1])}`
      }
      return tag === "polygon" ? `${d}Z` : d
    }
    case "circle": {
      const cx = num(attrs.cx)
      const cy = num(attrs.cy)
      const r = num(attrs.r)
      // Two half-arcs — a single arc to the same point is undefined.
      return (
        `M${fmt(cx - r)} ${fmt(cy)}` +
        `A${fmt(r)} ${fmt(r)} 0 1 0 ${fmt(cx + r)} ${fmt(cy)}` +
        `A${fmt(r)} ${fmt(r)} 0 1 0 ${fmt(cx - r)} ${fmt(cy)}Z`
      )
    }
    case "ellipse": {
      const cx = num(attrs.cx)
      const cy = num(attrs.cy)
      const rx = num(attrs.rx)
      const ry = num(attrs.ry, num(attrs.rx))
      return (
        `M${fmt(cx - rx)} ${fmt(cy)}` +
        `A${fmt(rx)} ${fmt(ry)} 0 1 0 ${fmt(cx + rx)} ${fmt(cy)}` +
        `A${fmt(rx)} ${fmt(ry)} 0 1 0 ${fmt(cx - rx)} ${fmt(cy)}Z`
      )
    }
    case "rect": {
      const x = num(attrs.x)
      const y = num(attrs.y)
      const w = num(attrs.width)
      const h = num(attrs.height)
      // SVG: a lone rx implies ry (and vice versa); both clamp to half-extent.
      const hasRx = attrs.rx !== undefined
      const hasRy = attrs.ry !== undefined
      let rx = hasRx ? num(attrs.rx) : hasRy ? num(attrs.ry) : 0
      let ry = hasRy ? num(attrs.ry) : hasRx ? num(attrs.rx) : 0
      rx = Math.min(rx, w / 2)
      ry = Math.min(ry, h / 2)
      if (rx <= 0 || ry <= 0) {
        return `M${fmt(x)} ${fmt(y)}H${fmt(x + w)}V${fmt(y + h)}H${fmt(x)}Z`
      }
      return (
        `M${fmt(x + rx)} ${fmt(y)}` +
        `H${fmt(x + w - rx)}` +
        `A${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(x + w)} ${fmt(y + ry)}` +
        `V${fmt(y + h - ry)}` +
        `A${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(x + w - rx)} ${fmt(y + h)}` +
        `H${fmt(x + rx)}` +
        `A${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(x)} ${fmt(y + h - ry)}` +
        `V${fmt(y + ry)}` +
        `A${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(x + rx)} ${fmt(y)}Z`
      )
    }
    default:
      throw new Error(`unhandled Lucide element <${tag}>`)
  }
}

/** Lucide's own SVG wrapper, verbatim apart from the stroke color. */
function toSvg(nodes: IconNode[], stroke: string): string {
  const children = nodes
    .map(([tag, attrs]) => {
      const rendered = Object.entries(attrs)
        .filter(([k]) => k !== "key")
        .map(([k, v]) => `${k}="${v}"`)
        .join(" ")
      return `<${tag} ${rendered}/>`
    })
    .join("")
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" ` +
    `viewBox="0 0 24 24" fill="none" stroke="${stroke}" stroke-width="2" ` +
    `stroke-linecap="round" stroke-linejoin="round">${children}</svg>\n`
  )
}

// ---------------------------------------------------------------------------
// Imported marks (EXP-924)
// ---------------------------------------------------------------------------

// A filled mark reads heavier than a 2px stroke, so it gets a smaller live
// area than Lucide's 20 units: 18 units, centred in the 24-unit grid.
const IMPORT_LIVE_AREA = 18

/** How many numbers each SVG path command consumes per repetition. */
const PATH_ARITY: Record<string, number> = {
  m: 2, l: 2, h: 1, v: 1, c: 6, s: 4, q: 4, t: 2, a: 7, z: 0,
}

/**
 * Scale path data by `scale` and move its origin to (`dx`, `dy`). Relative
 * commands only scale; absolute ones scale and translate. Arc flags are read
 * as single characters, because minified SVG packs them (`a1 1 0 011 2`).
 */
function transformPath(d: string, scale: number, dx: number, dy: number): string {
  let i = 0
  const out: string[] = []
  const skip = (): void => {
    while (i < d.length && /[\s,]/.test(d[i])) i++
  }
  const readNumber = (): number => {
    skip()
    const match = /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(d.slice(i))
    if (!match) throw new Error(`bad number in path data at ${i}`)
    i += match[0].length
    return Number.parseFloat(match[0])
  }
  const readFlag = (): number => {
    skip()
    const flag = d[i++]
    if (flag !== `0` && flag !== `1`) throw new Error(`bad arc flag at ${i}`)
    return Number(flag)
  }
  let first = true
  while ((skip(), i < d.length)) {
    const command = d[i++]
    const arity = PATH_ARITY[command.toLowerCase()]
    if (arity === undefined) throw new Error(`unhandled path command "${command}"`)
    if (arity === 0) {
      out.push(`Z`)
      continue
    }
    const absolute = command === command.toUpperCase()
    let letter = command
    do {
      // A leading relative moveto is absolute by definition.
      const moves = absolute || (first && command === `m`)
      const x = (v: number): number => v * scale + (moves ? dx : 0)
      const y = (v: number): number => v * scale + (moves ? dy : 0)
      let values: number[]
      switch (command.toLowerCase()) {
        case `h`:
          values = [x(readNumber())]
          break
        case `v`:
          values = [y(readNumber())]
          break
        case `a`:
          values = [
            readNumber() * scale,
            readNumber() * scale,
            readNumber(),
            readFlag(),
            readFlag(),
            x(readNumber()),
            y(readNumber()),
          ]
          break
        default:
          values = []
          for (let n = 0; n < arity; n += 2) {
            values.push(x(readNumber()), y(readNumber()))
          }
      }
      out.push(`${first && command === `m` ? `M` : letter}${values.map(fmtImported).join(` `)}`)
      first = false
      // Coordinates repeating after a moveto are implicit linetos.
      if (command.toLowerCase() === `m`) letter = absolute ? `L` : `l`
      skip()
    } while (i < d.length && /[\d.+-]/.test(d[i]))
  }
  return out.join(``)
}

/** Three decimals = 1/1000 of a 24-unit grid — below a pixel at any size we draw. */
const fmtImported = (n: number): string => String(Number.parseFloat(n.toFixed(3)))

function loadImported(name: string, file: string): IconNode[] {
  const svg = readFileSync(join(pkgRoot, file), "utf8")
  const viewBox = svg.match(/viewBox="([^"]+)"/)?.[1].trim().split(/[\s,]+/).map(Number)
  const paths = [...svg.matchAll(/<path[^>]*\sd="([^"]+)"/g)]
  if (!viewBox || viewBox.length !== 4 || paths.length !== 1) {
    throw new Error(`imported icon "${name}" (${file}) must be one <path> in a viewBox`)
  }
  const [minX, minY, width, height] = viewBox
  const scale = IMPORT_LIVE_AREA / Math.max(width, height)
  const d = transformPath(
    paths[0][1],
    scale,
    (24 - width * scale) / 2 - minX * scale,
    (24 - height * scale) / 2 - minY * scale
  )
  return [["path", { d, fill: "currentColor", stroke: "none", key: "mark" }]]
}

// ---------------------------------------------------------------------------
// Brand marks (the MCP catalog)
// ---------------------------------------------------------------------------

/** The light variant's one colour. A file carrying anything else is not the
 * light SVG the registry promises (the user's rule: light SVGs only). */
const BRAND_FILL = `#fff`

interface BrandMark {
  slug: string
  svg: string
  viewBox: string
  /** The file's drawable children, Illustrator wrappers removed. */
  body: string
}

/** Read one vendored brand SVG and check it is what the registry promises:
 * a single `<svg viewBox>` root, every fill the light white, no gradients,
 * masks, scripts or raster payloads (the files ship into two clients as-is). */
function loadBrand(slug: string, file: string): BrandMark {
  const svg = readFileSync(join(pkgRoot, file), "utf8")
  const viewBox = svg.match(/<svg[^>]*\sviewBox="([^"]+)"/)?.[1]
  if (!viewBox) throw new Error(`brand mark "${slug}" (${file}) has no viewBox`)
  if ((svg.match(/<svg[\s>]/g) ?? []).length !== 1) {
    throw new Error(`brand mark "${slug}" (${file}) must hold one <svg> root`)
  }
  for (const banned of [`<script`, `<image`, `<style`, `url(#`, `<linearGradient`, `<radialGradient`, `<mask`, `<clipPath`]) {
    if (svg.includes(banned)) {
      throw new Error(`brand mark "${slug}" (${file}) contains ${banned}; vendor a plain light SVG`)
    }
  }
  const fills = [...svg.matchAll(/fill\s*[:=]\s*"?([^;"\s]+)/g)].map((m) => m[1])
  for (const fill of fills) {
    if (fill.toLowerCase() !== BRAND_FILL) {
      throw new Error(
        `brand mark "${slug}" (${file}) fills with ${fill}; only the light (${BRAND_FILL}) variant is vendored`
      )
    }
  }
  const inner = svg.slice(svg.indexOf(`>`) + 1, svg.lastIndexOf(`</svg>`))
  // Illustrator exports wrap the art in `<switch><foreignObject …/>…</switch>`
  // so its own extension can claim the render; every SVG renderer falls
  // through to the plain sibling, so the wrapper is a no-op we drop.
  const body = inner
    .replace(/<foreignObject\b[^>]*\/>/g, ``)
    .replace(/<\/?switch>/g, ``)
    .trim()
  if (!body) throw new Error(`brand mark "${slug}" (${file}) draws nothing`)
  return { slug, svg, viewBox, body }
}

/** SVG presentation attribute → its React prop name. */
const JSX_ATTR: Record<string, string> = {
  "fill-rule": `fillRule`,
  "clip-rule": `clipRule`,
  "stroke-width": `strokeWidth`,
  "stroke-linecap": `strokeLinecap`,
  "stroke-linejoin": `strokeLinejoin`,
  "shape-rendering": `shapeRendering`,
  "fill-opacity": `fillOpacity`,
  "stroke-opacity": `strokeOpacity`,
}
const JSX_PASSTHROUGH = new Set([
  `d`, `cx`, `cy`, `r`, `rx`, `ry`, `x`, `y`, `x1`, `y1`, `x2`, `y2`, `width`,
  `height`, `points`, `transform`, `fill`, `stroke`, `opacity`,
])

/**
 * The vendored markup as JSX. The tags are the SVG drawing primitives; the
 * `style="…"` attribute Illustrator emits becomes the equivalent presentation
 * props so the mark stays a plain element tree (no style objects, no
 * `dangerouslySetInnerHTML`). Anything this converter has not seen is an
 * error, never a silent drop — the registry only vendors what it can render.
 */
function brandJsx(mark: BrandMark): string {
  const ALLOWED_TAGS = new Set([`path`, `circle`, `ellipse`, `rect`, `polygon`, `polyline`, `line`, `g`])
  return mark.body.replace(/<(\/?)([a-zA-Z]+)([^>]*?)(\/?)>/g, (_, close, tag, rawAttrs, selfClose) => {
    if (!ALLOWED_TAGS.has(tag)) {
      throw new Error(`brand mark "${mark.slug}" uses <${tag}>, which the JSX emitter does not handle`)
    }
    if (close) return `</${tag}>`
    const props: string[] = []
    for (const m of rawAttrs.matchAll(/\s([a-zA-Z:-]+)="([^"]*)"/g)) {
      const [, name, value] = m
      if (name === `style`) {
        for (const decl of value.split(`;`)) {
          const [k, v] = decl.split(`:`).map((s) => s.trim())
          if (!k) continue
          const prop = JSX_ATTR[k] ?? (JSX_PASSTHROUGH.has(k) ? k : undefined)
          if (!prop) throw new Error(`brand mark "${mark.slug}": unhandled style "${k}"`)
          props.push(`${prop}="${v}"`)
        }
        continue
      }
      const prop = JSX_ATTR[name] ?? (JSX_PASSTHROUGH.has(name) ? name : undefined)
      if (!prop) throw new Error(`brand mark "${mark.slug}": unhandled attribute "${name}"`)
      props.push(`${prop}="${value}"`)
    }
    const attrs = props.length ? ` ${props.join(` `)}` : ``
    return `<${tag}${attrs}${selfClose ? ` /` : ``}>`
  })
}

// ---------------------------------------------------------------------------
// Naming
// ---------------------------------------------------------------------------

/**
 * MUST match gpui-component's `icon_named!` macro (crates/macros/src/lib.rs
 * `pascal_case`): split on -/_/., a digit-leading segment is kept verbatim,
 * every other segment is capitalised and lowercased. `gamepad-2` ->
 * `Gamepad2`, `heading-1` -> `Heading1`. The Rust emitter below produces
 * `ExpIcon::` variants that the macro generates from the SVG filenames, so a
 * mismatch here is a compile error in the desktop crate.
 */
function pascal(name: string): string {
  return name
    .split(/[-_.]/)
    .filter(Boolean)
    .map((word) =>
      /^\d/.test(word)
        ? word
        : word[0].toUpperCase() + word.slice(1).toLowerCase()
    )
    .join("")
}

/** `nav-search` -> `navSearch` (Swift/Kotlin/TS member names). */
function camel(name: string): string {
  const p = pascal(name)
  return p[0].toLowerCase() + p.slice(1)
}

/** `nav-search` -> `NAV_SEARCH` (Rust consts). */
const screamingSnake = (name: string): string =>
  name.replace(/[^A-Za-z0-9]+/g, "_").toUpperCase()

// ---------------------------------------------------------------------------
// Resolve the icon set
// ---------------------------------------------------------------------------

const semanticKeys = Object.keys(registry.semantic).sort()
const customIcons: Record<string, IconNode[]> = { ...(registry.custom ?? {}) }
for (const [name, { file }] of Object.entries(registry.imported ?? {})) {
  if (name in customIcons) {
    throw new Error(`imported icon "${name}" collides with a custom icon.`)
  }
  customIcons[name] = loadImported(name, file)
}
const customNames = Object.keys(customIcons).sort()
// A custom name shadowing a real Lucide icon (or a hand-maintained desktop
// brand mark) would make the shipped art ambiguous — refuse loudly.
const BRAND_MARKS = [`claude`, `codex`, `logo`, `apple`, `google`]
for (const name of customNames) {
  if (existsSync(join(LUCIDE_ICONS_DIR, `${name}.js`))) {
    throw new Error(
      `custom icon "${name}" collides with a lucide-react icon of the same name.`
    )
  }
  if (BRAND_MARKS.includes(name)) {
    throw new Error(
      `custom icon "${name}" collides with a hand-maintained desktop brand mark.`
    )
  }
}
// Every distinct icon name the registry needs, deduped: an icon used by both
// a pickable entry and a concept ships exactly once per client.
const allNames = [
  ...new Set([
    ...registry.pickable,
    ...registry.devicePickable,
    ...Object.values(registry.semantic),
    ...customNames,
  ]),
].sort()

const geometry = new Map<string, IconNode[]>()
for (const name of allNames) {
  geometry.set(name, customIcons[name] ?? loadIconNode(name))
}

// The brand marks are a set of their own: never in `allNames` (they are not
// glyphs, they get no iOS/Android emit and no `icon_by_name` arm), and the
// desktop file name is prefixed `brand-` so `icon_named!` can never confuse a
// mark with a Lucide icon of the same word (`github`, `figma`, `slack` all
// exist as deprecated Lucide brand glyphs).
const brandSlugs = Object.keys(registry.brand ?? {}).sort()
const brandMarks = new Map<string, BrandMark>()
for (const slug of brandSlugs) {
  if (!/^[a-z][a-z0-9-]*$/.test(slug)) {
    throw new Error(`brand slug "${slug}" must be lowercase kebab-case`)
  }
  if (BRAND_MARKS.includes(slug) || allNames.includes(`brand-${slug}`)) {
    throw new Error(`brand mark "${slug}" collides with an existing icon name.`)
  }
  brandMarks.set(slug, loadBrand(slug, registry.brand![slug].file))
}

// EXP-379 — the outputs below reproduce Lucide's actual path geometry, so every
// one of them has to carry Lucide's copyright and licence, not just a "do not
// edit" banner. Wording is lifted from apps/desktop/assets/icons/LICENSE.txt,
// which already splits the Lucide Contributors and Feather/Cole Bemis portions
// correctly. The hand-authored `custom` glyphs in icons.json are our own work.
const HEADER = `AUTO-GENERATED by packages/icons/scripts/generate.ts — do not edit.
Source of truth: packages/icons/icons.json + lucide-react's shipped geometry.

The icon geometry reproduced in this file is from Lucide
(https://lucide.dev) and is licensed ISC. Glyphs listed under icons.json
\`custom\` are Exponential's own work, licensed Apache-2.0 with the rest of
this repository (see the top-level LICENSE). Marks listed under \`imported\`
(the OS logos) are from selfh.st/icons (https://github.com/selfhst/icons),
licensed CC-BY-4.0, scaled into Lucide's 24-unit grid; the logos themselves
are trademarks of their respective owners.

ISC License

Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as
part of Feather (MIT). All other copyright (c) for Lucide are held by
Lucide Contributors 2022.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.`

// Blank header lines emit as a bare `//` — a trailing space after the slashes
// would be the only whitespace-only content in five generated files.
const HEADER_COMMENT = HEADER.split("\n")
  .map((line) => (line ? `// ${line}` : `//`))
  .join("\n")

const written: string[] = []
function write(path: string, contents: string): void {
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, contents)
  written.push(path)
}

// ---------------------------------------------------------------------------
// 1. Web — packages/icons/src/generated.ts
// ---------------------------------------------------------------------------

const tsUnion = (values: string[]): string =>
  values.map((v) => `\n  | \`${v}\``).join("")

write(
  join(pkgRoot, "src/generated.ts"),
  `${HEADER_COMMENT}

/** Every Lucide name the registry ships, on every client. */
export const ICON_NAMES = [
${allNames.map((n) => `  \`${n}\`,`).join("\n")}
] as const
export type IconName = (typeof ICON_NAMES)[number]

/** The user-facing pickable set (board icons + action icons), display order. */
export const PICKABLE_ICONS = [
${registry.pickable.map((n) => `  \`${n}\`,`).join("\n")}
] as const
export type PickableIcon = (typeof PICKABLE_ICONS)[number]

/** The device icon picker's set (EXP-924), display order. */
export const DEVICE_ICONS = [
${registry.devicePickable.map((n) => `  \`${n}\`,`).join("\n")}
] as const
export type DeviceIconName = (typeof DEVICE_ICONS)[number]

/** Hand-authored non-Lucide glyph names (icons.json \`custom\` — EXP-314). */
export const CUSTOM_ICONS = [
${customNames.map((n) => `  \`${n}\`,`).join("\n")}
] as const
export type CustomIcon = (typeof CUSTOM_ICONS)[number]

/** The MCP catalog's brand marks (icons.json \`brand\`): selfh.st/icons light
 * SVGs, a set apart from the Lucide names above. Web renders them through
 * \`@exp/ui\`'s \`BRAND_ICONS\`/\`brandIcon()\`, desktop through
 * \`registry::brand\`; contract.json's \`mcpCatalog\` names them by slug. */
export const BRAND_ICON_NAMES = [
${brandSlugs.map((n) => `  \`${n}\`,`).join("\n")}
] as const
export type BrandIconName = (typeof BRAND_ICON_NAMES)[number]

/** Stable concept id -> icon name. Call sites reference the concept. */
export const SEMANTIC_ICONS = {
${semanticKeys.map((k) => `  "${k}": \`${registry.semantic[k]}\`,`).join("\n")}
} as const
export type IconConcept = keyof typeof SEMANTIC_ICONS

/** Narrowing guard for values that arrive as plain strings (boards.icon). */
export function isIconName(value: string): value is IconName {
  return (ICON_NAMES as readonly string[]).includes(value)
}

export function isPickableIcon(value: string): value is PickableIcon {
  return (PICKABLE_ICONS as readonly string[]).includes(value)
}

export function isDeviceIcon(value: string): value is DeviceIconName {
  return (DEVICE_ICONS as readonly string[]).includes(value)
}

export function isBrandIcon(value: string): value is BrandIconName {
  return (BRAND_ICON_NAMES as readonly string[]).includes(value)
}
`
)

// ---------------------------------------------------------------------------
// 1b. Web (server) — packages/icons/src/pickable-svg.generated.ts
// ---------------------------------------------------------------------------

// EXP-569 — the pickable set rendered to standalone SVG strings, for servers
// that embed the markup directly (the widget config endpoint serves the
// launcher icon as resolved SVG so the size-budgeted widget loader never
// bundles the icon set). Kept out of src/generated.ts on purpose: that module
// is imported by client bundles, which must not pull 60 SVG strings along
// with the name lists. `aria-hidden` matches the widget's hardcoded fallback
// glyph (packages/widget/src/theme.ts `megaphoneIconSvg`).
write(
  join(pkgRoot, "src/pickable-svg.generated.ts"),
  `${HEADER_COMMENT}

import type { PickableIcon } from "./generated"

/** Pickable icon name -> standalone \`currentColor\` SVG markup. */
export const PICKABLE_ICON_SVG: Record<PickableIcon, string> = {
${registry.pickable
  .map(
    (n) =>
      `  "${n}": \`${toSvg(geometry.get(n)!, "currentColor")
        .trim()
        .replace(`<svg `, `<svg aria-hidden="true" `)}\`,`
  )
  .join("\n")}
}
`
)

// ---------------------------------------------------------------------------
// 2. Web — packages/ui/src/icons.generated.ts (name -> lucide-react component)
// ---------------------------------------------------------------------------

// lucide-react exports PascalCase component names; `gamepad-2` -> `Gamepad2`.
const componentName = (name: string): string => pascal(name)

const lucideNames = allNames.filter((n) => !(n in customIcons))

write(
  join(repoRoot, "packages/ui/src/icons.generated.ts"),
  `${HEADER_COMMENT}
//
// The concrete lucide-react components behind the registry, so a stored icon
// name or a concept id resolves to a real component without a per-call-site
// import. Tree-shaking still applies: only these named imports are pulled in.
// Custom (non-Lucide) glyphs are built with the same \`createLucideIcon\`
// factory lucide-react uses internally, so they behave like any other icon.

import {
${lucideNames.map((n) => `  ${componentName(n)},`).join("\n")}
  createLucideIcon,
} from "lucide-react"
import type { IconNode, LucideIcon } from "lucide-react"
import {
  type IconConcept,
  type IconName,
  SEMANTIC_ICONS,
} from "@exp/icons"

${customNames
  .map(
    (n) =>
      `const ${componentName(n)} = createLucideIcon(\`${n}\`, ${JSON.stringify(customIcons[n])} as IconNode)`
  )
  .join("\n")}

export const ICON_COMPONENTS: Record<IconName, LucideIcon> = {
${allNames.map((n) => `  "${n}": ${componentName(n)},`).join("\n")}
}

/** The component for a registry concept (compile-time checked). */
export function conceptIcon(concept: IconConcept): LucideIcon {
  return ICON_COMPONENTS[SEMANTIC_ICONS[concept]]
}
`
)

// ---------------------------------------------------------------------------
// 2b. Web — packages/ui/src/brand-icons.generated.tsx (the MCP catalog marks)
// ---------------------------------------------------------------------------

// One React component per vendored mark, the file's own elements inlined at
// its own viewBox. Like the lucide components it takes SVG props and carries
// width/height 24 so `className="size-4"` sizes it exactly like a glyph;
// unlike them its fills are the mark's own white, so a `color` on the row
// never tints it (a trademark is reproduced, never recoloured).
const BRAND_HEADER = `${HEADER_COMMENT}
//
// The brand marks behind contract.json's \`mcpCatalog\` (icons.json \`brand\`):
// the LIGHT SVGs of selfh.st/icons (https://github.com/selfhst/icons,
// CC-BY-4.0), vendored verbatim under packages/icons/brand/ and inlined here
// element for element. The marks are trademarks of their respective owners,
// reproduced nominatively to label the service a server belongs to; sizing
// is the only thing a call site may change. The Lucide licence above does
// not apply to this file's geometry.`

const brandComponent = (slug: string): string => `Brand${pascal(slug)}Icon`

write(
  join(repoRoot, "packages/ui/src/brand-icons.generated.tsx"),
  `${BRAND_HEADER}

import type { ComponentType, SVGProps } from "react"
import type { BrandIconName } from "@exp/icons"

export type BrandIcon = ComponentType<SVGProps<SVGSVGElement>>

${brandSlugs
  .map((slug) => {
    const mark = brandMarks.get(slug)!
    const owner = registry.brand![slug].owner
    return (
      `/** ${owner} — ${registry.brand![slug].source} */\n` +
      `export function ${brandComponent(slug)}(props: SVGProps<SVGSVGElement>) {\n` +
      `  return (\n` +
      `    <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="${mark.viewBox}" aria-hidden="true" {...props}>\n` +
      `      ${brandJsx(mark)}\n` +
      `    </svg>\n` +
      `  )\n` +
      `}`
    )
  })
  .join("\n\n")}

/** Slug -> component, for every mark the registry vendors. */
export const BRAND_ICONS: Record<BrandIconName, BrandIcon> = {
${brandSlugs.map((slug) => `  "${slug}": ${brandComponent(slug)},`).join("\n")}
}

/** The component for a brand slug (compile-time checked). */
export function brandIcon(name: BrandIconName): BrandIcon {
  return BRAND_ICONS[name]
}
`
)

// ---------------------------------------------------------------------------
// 3. Desktop — assets/icons/<name>.svg + crates/ui/src/icons.generated.rs
// ---------------------------------------------------------------------------

const desktopIcons = join(repoRoot, "apps/desktop/assets/icons")
for (const name of allNames) {
  write(join(desktopIcons, `${name}.svg`), toSvg(geometry.get(name)!, "currentColor"))
}

write(
  join(repoRoot, "apps/desktop/crates/ui/src/icons.generated.rs"),
  `${HEADER_COMMENT}
//
// \`ExpIcon\` variants come from the \`icon_named!\` macro scanning
// \`apps/desktop/assets/icons\`, which this generator also populates — so every
// arm below is guaranteed to have its SVG on disk.
//
// Included via \`include!\` from icons.rs, which carries the \`allow(dead_code)\`
// — the registry is shared by four clients, so most concepts have no desktop
// call site yet.

use crate::icons::ExpIcon;

/// One pickable icon name (board icons + action icons), in display order.
pub const PICKABLE_ICONS: &[&str] = &[
${registry.pickable.map((n) => `    "${n}",`).join("\n")}
];

/// The device icon picker's set (EXP-924), in display order.
pub const DEVICE_ICONS: &[&str] = &[
${registry.devicePickable.map((n) => `    "${n}",`).join("\n")}
];

/// A pickable/stored icon name -> its glyph. \`None\` for an unknown name so
/// callers can apply their own fallback.
pub fn icon_by_name(name: &str) -> Option<ExpIcon> {
    Some(match name {
${allNames.map((n) => `        "${n}" => ExpIcon::${pascal(n)},`).join("\n")}
        _ => return None,
    })
}

${semanticKeys
  .map(
    (k) =>
      `/// Registry concept \`${k}\` -> Lucide \`${registry.semantic[k]}\`.\n` +
      `pub const ${screamingSnake(k)}: ExpIcon = ExpIcon::${pascal(registry.semantic[k])};`
  )
  .join("\n")}

/// The MCP catalog's brand marks (icons.json \`brand\`): selfh.st/icons LIGHT
/// SVGs shipped verbatim as \`assets/icons/brand-<slug>.svg\` (CC-BY-4.0; the
/// marks are their owners' trademarks, reproduced nominatively). gpui
/// rasterizes them to a one-tint mask like every other asset, so a call site
/// picks the tint; \`contract::MCP_CATALOG_MARKS\` names them by slug.
pub mod brand {
    use crate::icons::ExpIcon;

    /// Every vendored slug, sorted.
    pub const SLUGS: &[&str] = &[
${brandSlugs.map((n) => `        "${n}",`).join("\n")}
    ];

    /// A catalog \`mark\` slug -> its asset. \`None\` for a slug this build
    /// does not vendor.
    pub fn by_slug(slug: &str) -> Option<ExpIcon> {
        Some(match slug {
${brandSlugs.map((n) => `            "${n}" => ExpIcon::Brand${pascal(n)},`).join("\n")}
            _ => return None,
        })
    }

${brandSlugs
  .map(
    (n) =>
      `    /// ${registry.brand![n].owner}.\n` +
      `    pub const ${screamingSnake(n)}: ExpIcon = ExpIcon::Brand${pascal(n)};`
  )
  .join("\n")}
}
`
)

// The marks ship byte-for-byte: `icon_named!` turns `brand-<slug>.svg` into
// `ExpIcon::Brand<Slug>`, which the module above names.
for (const slug of brandSlugs) {
  write(join(desktopIcons, `brand-${slug}.svg`), brandMarks.get(slug)!.svg)
}

// ---------------------------------------------------------------------------
// 4. iOS — asset-catalog imagesets + AppIcons.generated.swift
// ---------------------------------------------------------------------------

const xcassets = join(repoRoot, "apps/ios/Exponential/Assets.xcassets")
const IMAGESET_CONTENTS = (file: string): string =>
  JSON.stringify(
    {
      images: [{ filename: file, idiom: "universal" }],
      info: { author: "xcode", version: 1 },
      properties: {
        "preserves-vector-representation": true,
        "template-rendering-intent": "template",
      },
    },
    null,
    2
  ) + "\n"

for (const name of allNames) {
  const asset = `lucide-${name}`
  const dir = join(xcassets, `${asset}.imageset`)
  // Xcode's catalog compiler wants a concrete stroke color; the template
  // rendering intent above is what makes `.foregroundStyle` tint it (this is
  // exactly how the pre-existing tab-robot.imageset works).
  write(join(dir, `${asset}.svg`), toSvg(geometry.get(name)!, "#000000"))
  write(join(dir, "Contents.json"), IMAGESET_CONTENTS(`${asset}.svg`))
}

write(
  join(repoRoot, "apps/ios/ExpUI/Sources/AppIcons.generated.swift"),
  `${HEADER_COMMENT}
//
// Asset names for the bundled Lucide imagesets. Render them through
// \`AppIcon\` (AppIcon.swift), never \`Image(systemName:)\` — these are template
// images in the app bundle's asset catalog, not SF Symbols.

import Foundation

public enum AppIcons {
    /// The user-facing pickable set (board icons + action icons), display order.
    public static let pickable: [String] = [
${registry.pickable.map((n) => `        "${n}"`).join(",\n")}
    ]

    /// The device icon picker's set (EXP-924), display order.
    public static let devicePickable: [String] = [
${registry.devicePickable.map((n) => `        "${n}"`).join(",\n")}
    ]

    /// Every registry name that ships as an imageset.
    public static let allNames: Set<String> = [
${allNames.map((n) => `        "${n}"`).join(",\n")}
    ]

    /// Registry name -> asset-catalog imageset name.
    public static func assetName(_ name: String) -> String? {
        allNames.contains(name) ? "lucide-\\(name)" : nil
    }

${semanticKeys
  .map(
    (k) =>
      `    /// Concept \`${k}\`.\n` +
      `    public static let ${camel(k)}: String = "${registry.semantic[k]}"`
  )
  .join("\n")}
}
`
)

// ---------------------------------------------------------------------------
// 5. Android — Compose ImageVector source
// ---------------------------------------------------------------------------

// Emitting ImageVectors (rather than res/drawable XML) keeps every existing
// `Icon(imageVector = …)` call site's signature intact — the Android migration
// is then a pure identifier swap. `addPathNodes` parses the same `d` strings
// the web renders, so the geometry is shared rather than re-drawn.
function kotlinIcon(name: string): string {
  const nodes = geometry.get(name)!
  const paths = nodes
    .map(([tag, attrs]) => {
      const d = nodeToPathData([tag, attrs])
      // A handful of Lucide glyphs fill a sub-path instead of stroking it.
      const filled = attrs.fill !== undefined && attrs.fill !== "none"
      return filled
        ? `            addPath(addPathNodes("${d}"), fill = SolidColor(Color.Black))`
        : `            addPath(\n                addPathNodes("${d}"),\n                stroke = SolidColor(Color.Black),\n                strokeLineWidth = 2f,\n                strokeLineCap = StrokeCap.Round,\n                strokeLineJoin = StrokeJoin.Round,\n            )`
    })
    .join("\n")
  return `    public val \`${name}\`: ImageVector by lazy {
        ImageVector.Builder(
            name = "${name}",
            defaultWidth = 24.dp,
            defaultHeight = 24.dp,
            viewportWidth = 24f,
            viewportHeight = 24f,
        ).apply {
${paths}
        }.build()
    }`
}

write(
  join(
    repoRoot,
    "apps/android/app/src/main/java/com/exponential/app/ui/icons/ExpIcons.generated.kt"
  ),
  `${HEADER_COMMENT}
//
// Compose ImageVectors built from Lucide's own path data. Every value is an
// ImageVector, so \`Icon(imageVector = …, tint = …)\` call sites are unchanged
// from the Material icons these replaced. The declared stroke color is
// irrelevant — \`Icon\` tints the whole vector.

package com.exponential.app.ui.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

public object ExpIcons {
${allNames.map(kotlinIcon).join("\n\n")}

    /** The user-facing pickable set (board icons + action icons), display order. */
    public val pickable: List<String> = listOf(
${registry.pickable.map((n) => `        "${n}",`).join("\n")}
    )

    /** The device icon picker's set (EXP-924), display order. */
    public val devicePickable: List<String> = listOf(
${registry.devicePickable.map((n) => `        "${n}",`).join("\n")}
    )

    /** Registry name -> glyph. Null for an unknown name (caller falls back). */
    public fun byName(name: String): ImageVector? = when (name) {
${allNames.map((n) => `        "${n}" -> \`${n}\``).join("\n")}
        else -> null
    }

${semanticKeys
  .map(
    (k) =>
      `    /** Concept \`${k}\`. */\n` +
      `    public val ${camel(k)}: ImageVector get() = \`${registry.semantic[k]}\``
  )
  .join("\n")}
}
`
)

// ---------------------------------------------------------------------------
// 6. EXP-1184 — the agent "working" spinner: Claude's own hand-drawn
//    "writing" spark (8 frames, 90 ms each, hard cuts), vendored once as the
//    sprite sheet `agent/claude-writing.svg` and split here into one frame per
//    file on the 0 0 100 100 grid every claude mark already uses. Each client
//    steps the frame index on a timer; reduced motion holds the static mark.
// ---------------------------------------------------------------------------

const CLAUDE_WRITING = { frames: 8, frameMs: 90 } as const
const CLAUDE_HEX = `#D97757`
const claudeFrames = splitSprite(
  readFileSync(join(pkgRoot, "agent/claude-writing.svg"), "utf8"),
  CLAUDE_WRITING.frames
)

write(
  join(repoRoot, "packages/ui/src/claude-spinner.generated.ts"),
  `${HEADER_COMMENT}

/** EXP-1184: one loop step of Claude's "writing" spark, in ms. */
export const CLAUDE_SPINNER_FRAME_MS = ${CLAUDE_WRITING.frameMs}

/** EXP-1184: the frames (path data on a 0 0 100 100 grid), in play order. */
export const CLAUDE_SPINNER_FRAMES = [
${claudeFrames.map((d) => `  \`${d}\`,`).join("\n")}
] as const
`
)

claudeFrames.forEach((d, i) => {
  write(
    join(desktopIcons, `claude-writing-${i}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" fill="hsl(14.8, 63.1%, 59.6%)"><path d="${d}"/></svg>\n`
  )
  const set = join(repoRoot, `apps/ios/Exponential/Assets.xcassets/agent-claude-writing-${i}.imageset`)
  write(
    join(set, `agent-claude-writing-${i}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100" fill="${CLAUDE_HEX}"><path d="${d}"/></svg>\n`
  )
  write(
    join(set, "Contents.json"),
    JSON.stringify(
      {
        images: [{ filename: `agent-claude-writing-${i}.svg`, idiom: "universal" }],
        info: { author: "xcode", version: 1 },
        properties: {
          "preserves-vector-representation": true,
          "template-rendering-intent": "original",
        },
      },
      null,
      2
    ).replace(/": /g, '" : ') + "\n"
  )
  write(
    join(repoRoot, `apps/android/app/src/main/res/drawable/ic_agent_claude_writing_${i}.xml`),
    `<?xml version="1.0" encoding="utf-8"?>
<!-- Generated by packages/icons (EXP-1184): frame ${i} of Claude's "writing" spark. -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="24dp"
    android:height="24dp"
    android:viewportWidth="100"
    android:viewportHeight="100">
    <path
        android:fillColor="#FF${CLAUDE_HEX.slice(1)}"
        android:pathData="${d}" />
</vector>
`
  )
})

// ---------------------------------------------------------------------------

console.log(
  `Wrote ${written.length} files for ${allNames.length} icons ` +
    `(${registry.pickable.length} pickable, ${registry.devicePickable.length} device, ` +
    `${semanticKeys.length} concepts) + ${brandSlugs.length} brand marks + ${claudeFrames.length} spinner frames.`
)
