// VAPP-92: the theme builder's engine (ui.exponential.at mounts the UI,
// `builder/` in this package is the standalone page). Pure data in, data
// out: import a shadcn `globals.css` / tweakcn export as a starting point,
// express a draft as the smallest `extends` theme, and export JSON.

import { TOKEN_GROUPS } from "./catalog"
import { toThemeHex } from "./color"
import { MODES, loadTheme } from "./theme"
import type { ModeName, ResolvedTheme, Shadow, ThemeIssue, ThemeSource } from "./theme-types"

/** shadcn / tweakcn variable → theme colour name. */
export const SHADCN_COLOR_VARS: Record<string, string> = {
  background: `background`,
  foreground: `foreground`,
  card: `card`,
  "card-foreground": `cardForeground`,
  popover: `popover`,
  "popover-foreground": `popoverForeground`,
  primary: `primary`,
  "primary-foreground": `primaryForeground`,
  secondary: `secondary`,
  "secondary-foreground": `secondaryForeground`,
  muted: `muted`,
  "muted-foreground": `mutedForeground`,
  accent: `accent`,
  "accent-foreground": `accentForeground`,
  destructive: `destructive`,
  "destructive-foreground": `destructiveForeground`,
  success: `success`,
  "success-foreground": `successForeground`,
  warning: `warning`,
  "warning-foreground": `warningForeground`,
  info: `info`,
  "info-foreground": `infoForeground`,
  border: `border`,
  input: `input`,
  ring: `ring`,
  "chart-1": `chart1`,
  "chart-2": `chart2`,
  "chart-3": `chart3`,
  "chart-4": `chart4`,
  "chart-5": `chart5`,
}

export interface ThemeImport {
  modes: Partial<Record<ModeName, { color: Record<string, `#${string}`>; shadow?: Record<string, Shadow[]> }>>
  tokens: { radius?: Record<string, number>; type?: { family?: Record<string, string> } }
  fonts: Record<string, { fallback?: string; source: `host` }>
  /** Variables the importer did not understand (`--sidebar`, `var()` refs…). */
  unmapped: string[]
}

const BLOCK = /([^{}]+)\{([^{}]*)\}/g
const DECL = /--([a-zA-Z0-9-]+)\s*:\s*([^;]+);?/g

function selectorMode(selector: string): ModeName | null {
  const s = selector.trim()
  if (/\.dark\b|\[data-theme=["']?dark|prefers-color-scheme:\s*dark/.test(s)) return `dark`
  if (/^(:root|:host|html|body)(\s*,\s*(:root|:host|html|body))*$/.test(s) || /\.light\b/.test(s)) return `light`
  return null
}

function px(value: string): number | null {
  const v = value.trim()
  if (v.endsWith(`rem`)) return Math.round(parseFloat(v) * 16 * 100) / 100
  if (v.endsWith(`px`)) return parseFloat(v)
  if (v.endsWith(`em`)) return Math.round(parseFloat(v) * 16 * 100) / 100
  const n = parseFloat(v)
  return Number.isFinite(n) && /^-?[\d.]+$/.test(v) ? n : null
}

/** `0 1px 2px 0 hsl(0 0% 0% / 0.05), …` → layers; null when not parseable. */
export function parseShadow(value: string): Shadow[] | null {
  const v = value.trim()
  if (v === `none`) return []
  const layers: Shadow[] = []
  // split on commas outside parentheses
  let depth = 0
  let start = 0
  const parts: string[] = []
  for (let i = 0; i < v.length; i++) {
    if (v[i] === `(`) depth++
    else if (v[i] === `)`) depth--
    else if (v[i] === `,` && depth === 0) {
      parts.push(v.slice(start, i))
      start = i + 1
    }
  }
  parts.push(v.slice(start))
  for (const part of parts) {
    const m = /^\s*(-?[\d.]+(?:px)?)\s+(-?[\d.]+(?:px)?)\s+(-?[\d.]+(?:px)?)(?:\s+(-?[\d.]+(?:px)?))?\s+(.+?)\s*$/.exec(part)
    if (!m) return null
    const color = toThemeHex(m[5])
    if (!color) return null
    layers.push({ x: parseFloat(m[1]), y: parseFloat(m[2]), blur: parseFloat(m[3]), spread: m[4] ? parseFloat(m[4]) : 0, color })
  }
  return layers
}

function firstFamily(value: string): { family: string; fallback?: string } {
  const parts = value.split(`,`).map((s) => s.trim().replace(/^["']|["']$/g, ``)).filter(Boolean)
  return { family: parts[0] ?? value.trim(), fallback: parts.length > 1 ? parts.slice(1).join(`, `) : undefined }
}

/** A shadcn `globals.css` (v4 oklch, v3 bare-HSL triplets) or a tweakcn
 *  export → the colours per mode, the radius ladder, the families and the
 *  shadows it carried. Everything else is listed under `unmapped`. */
export function importShadcnCss(css: string): ThemeImport {
  const out: ThemeImport = { modes: {}, tokens: {}, fonts: {}, unmapped: [] }
  const unmapped = new Set<string>()
  const text = css.replace(/\/\*[\s\S]*?\*\//g, ``)
  const addFamily = (slot: string, value: string) => {
    const { family, fallback } = firstFamily(value)
    ;((out.tokens.type ??= {}).family ??= {})[slot] = family
    out.fonts[family] = { ...(fallback ? { fallback } : {}), source: `host` }
  }
  for (const block of text.matchAll(BLOCK)) {
    const mode = selectorMode(block[1])
    for (const decl of block[2].matchAll(DECL)) {
      const name = decl[1]
      const value = decl[2].trim()
      const colorName = SHADCN_COLOR_VARS[name]
      if (colorName) {
        const hex = toThemeHex(value)
        if (!hex || !mode) {
          unmapped.add(`--${name}`)
          continue
        }
        ;(out.modes[mode] ??= { color: {} }).color[colorName] = hex
        continue
      }
      if (name === `radius`) {
        const base = px(value)
        if (base === null) {
          unmapped.add(`--${name}`)
          continue
        }
        // Tailwind's calc ladder around --radius (= lg).
        out.tokens.radius = {
          none: 0,
          sm: Math.max(0, base - 4),
          md: Math.max(0, base - 2),
          lg: base,
          xl: base + 4,
          xl2: base + 8,
          xl3: base + 12,
          full: 9999,
        }
        continue
      }
      if (name === `font-sans` || name === `font-mono`) {
        if (value.includes(`var(`)) unmapped.add(`--${name}`)
        else addFamily(name.slice(5), value)
        continue
      }
      const shadow = /^shadow-(sm|md|lg)$/.exec(name)
      if (shadow && mode) {
        const layers = parseShadow(value)
        if (!layers) {
          unmapped.add(`--${name}`)
          continue
        }
        const m = (out.modes[mode] ??= { color: {} })
        ;(m.shadow ??= {})[shadow[1]] = layers
        continue
      }
      unmapped.add(`--${name}`)
    }
  }
  out.unmapped = [...unmapped].sort()
  return out
}

/** The import as a theme file. With a `base` it becomes an `extends` theme
 *  carrying only what the CSS said; without one it is a root and the
 *  caller fills the rest (the builder starts from a built-in anyway). */
export function themeFromImport(imp: ThemeImport, meta: { id: string; name: string; extends?: string }): ThemeSource {
  const theme: ThemeSource = { id: meta.id, name: meta.name }
  if (meta.extends) theme.extends = meta.extends
  const modes: ThemeSource[`modes`] = {}
  for (const mode of MODES) {
    const m = imp.modes[mode]
    if (!m) continue
    modes[mode] = { color: m.color, ...(m.shadow ? { shadow: m.shadow } : {}) }
    if (m.shadow && !m.shadow.none) modes[mode]!.shadow = { none: [], ...m.shadow }
  }
  if (Object.keys(modes).length > 0) theme.modes = modes
  const tokens: NonNullable<ThemeSource[`tokens`]> = {}
  if (imp.tokens.radius) tokens.radius = imp.tokens.radius
  if (imp.tokens.type?.family) tokens.type = { family: imp.tokens.type.family }
  if (Object.keys(tokens).length > 0) theme.tokens = tokens
  if (Object.keys(imp.fonts).length > 0) theme.fonts = imp.fonts
  return theme
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)

/** The smallest theme that, extending `base`, resolves to `full`: only the
 *  values that differ. Recipes pass through as the draft's own rules (they
 *  are already overrides). */
export function diffTheme(full: ResolvedTheme | ThemeSource, base: ResolvedTheme, meta?: { id?: string; name?: string }): ThemeSource {
  const resolved = `chain` in full ? full : loadTheme(full, { themes: [base] })
  const out: ThemeSource = { id: meta?.id ?? resolved.id, name: meta?.name ?? resolved.name, extends: base.id }
  const modes: ThemeSource[`modes`] = {}
  for (const mode of MODES) {
    const color: Record<string, `#${string}`> = {}
    const shadow: Record<string, Shadow[]> = {}
    for (const [k, v] of Object.entries(resolved.modes[mode].color)) if (base.modes[mode].color[k] !== v) color[k] = v
    for (const [k, v] of Object.entries(resolved.modes[mode].shadow)) if (!same(base.modes[mode].shadow[k], v)) shadow[k] = v
    const m: { color?: Record<string, `#${string}`>; shadow?: Record<string, Shadow[]> } = {}
    if (Object.keys(color).length > 0) m.color = color
    if (Object.keys(shadow).length > 0) m.shadow = shadow
    if (Object.keys(m).length > 0) modes[mode] = m
  }
  if (Object.keys(modes).length > 0) out.modes = modes
  const tokens: Record<string, unknown> = {}
  for (const group of Object.keys(TOKEN_GROUPS)) {
    if (group === `color` || group === `shadow`) continue
    const [top, sub] = group.split(`.`)
    const mine = sub ? (resolved.tokens.type as Record<string, Record<string, unknown>>)[sub] : (resolved.tokens as unknown as Record<string, Record<string, unknown>>)[top]
    const theirs = sub ? (base.tokens.type as Record<string, Record<string, unknown>>)[sub] : (base.tokens as unknown as Record<string, Record<string, unknown>>)[top]
    const diff: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(mine ?? {})) if (!same(theirs?.[k], v)) diff[k] = v
    if (Object.keys(diff).length === 0) continue
    if (sub) ((tokens.type ??= {}) as Record<string, unknown>)[sub] = diff
    else tokens[top] = diff
  }
  if (Object.keys(tokens).length > 0) out.tokens = tokens as ThemeSource[`tokens`]
  const fonts: ThemeSource[`fonts`] = {}
  for (const [k, v] of Object.entries(resolved.fonts)) if (!same(base.fonts[k], v)) fonts[k] = v
  if (Object.keys(fonts).length > 0) out.fonts = fonts
  if (`chain` in full) {
    // A resolved theme carries the base's rules too: keep only the tail each part added.
    const recipes: NonNullable<ThemeSource[`recipes`]> = {}
    for (const [component, byPart] of Object.entries(resolved.recipes)) {
      for (const [part, rules] of Object.entries(byPart)) {
        const baseRules = base.recipes[component]?.[part] ?? []
        const tail = rules.slice(baseRules.length)
        if (tail.length > 0) ((recipes[component] ??= {})[part] = tail)
      }
    }
    if (Object.keys(recipes).length > 0) out.recipes = recipes
  } else if (full.recipes && Object.keys(full.recipes).length > 0) out.recipes = full.recipes
  return out
}

/** Pretty JSON with `$schema` first, the export the builder downloads. */
export function exportThemeJson(theme: ThemeSource): string {
  const { $schema: _ignored, ...rest } = theme
  return `${JSON.stringify({ $schema: `https://ui.exponential.at/schemas/theme/v1.json`, ...rest }, null, 2)}\n`
}

/** `JSON.parse` + load, never throwing: the builder's "paste a theme" path. */
export function parseThemeJson(text: string, themes: readonly (ThemeSource | ResolvedTheme)[]): { theme: ResolvedTheme | null; source: ThemeSource | null; issues: ThemeIssue[] } {
  let source: unknown
  try {
    source = JSON.parse(text)
  } catch (error) {
    return { theme: null, source: null, issues: [{ path: `theme`, message: `not JSON: ${(error as Error).message}` }] }
  }
  try {
    return { theme: loadTheme(source, { themes }), source: source as ThemeSource, issues: [] }
  } catch (error) {
    const issues = (error as { issues?: ThemeIssue[] }).issues ?? [{ path: `theme`, message: (error as Error).message }]
    return { theme: null, source: source as ThemeSource, issues }
  }
}
