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
//            apps/web/src/lib/icons.generated.ts    IconName -> LucideIcon
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
const customIcons = registry.custom ?? {}
const customNames = Object.keys(customIcons).sort()
// A custom name shadowing a real Lucide icon (or a hand-maintained desktop
// brand mark) would make the shipped art ambiguous — refuse loudly.
const BRAND_MARKS = [`claude`, `codex`, `pi`, `logo`, `apple`, `google`]
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
    ...Object.values(registry.semantic),
    ...customNames,
  ]),
].sort()

const geometry = new Map<string, IconNode[]>()
for (const name of allNames) {
  geometry.set(name, customIcons[name] ?? loadIconNode(name))
}

const HEADER = `AUTO-GENERATED by packages/icons/scripts/generate.ts — do not edit.
Source of truth: packages/icons/icons.json + lucide-react's shipped geometry.`

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
  `// ${HEADER.split("\n").join("\n// ")}

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

/** Hand-authored non-Lucide glyph names (icons.json \`custom\` — EXP-314). */
export const CUSTOM_ICONS = [
${customNames.map((n) => `  \`${n}\`,`).join("\n")}
] as const
export type CustomIcon = (typeof CUSTOM_ICONS)[number]

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
`
)

// ---------------------------------------------------------------------------
// 2. Web — apps/web/src/lib/icons.generated.ts (name -> lucide-react component)
// ---------------------------------------------------------------------------

// lucide-react exports PascalCase component names; `gamepad-2` -> `Gamepad2`.
const componentName = (name: string): string => pascal(name)

const lucideNames = allNames.filter((n) => !(n in customIcons))

write(
  join(repoRoot, "apps/web/src/lib/icons.generated.ts"),
  `// ${HEADER.split("\n").join("\n// ")}
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
// 3. Desktop — assets/icons/<name>.svg + crates/ui/src/icons.generated.rs
// ---------------------------------------------------------------------------

const desktopIcons = join(repoRoot, "apps/desktop/assets/icons")
for (const name of allNames) {
  write(join(desktopIcons, `${name}.svg`), toSvg(geometry.get(name)!, "currentColor"))
}

write(
  join(repoRoot, "apps/desktop/crates/ui/src/icons.generated.rs"),
  `// ${HEADER.split("\n").join("\n// ")}
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
`
)

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
  `// ${HEADER.split("\n").join("\n// ")}
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
  `// ${HEADER.split("\n").join("\n// ")}
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

console.log(
  `Wrote ${written.length} files for ${allNames.length} icons ` +
    `(${registry.pickable.length} pickable, ${semanticKeys.length} concepts).`
)
