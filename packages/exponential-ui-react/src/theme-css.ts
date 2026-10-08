// VAPP-87: a RESOLVED theme → one scoped stylesheet. The theme compiles to
//   1. a CSS-variable block on the surface root (`--xui-*` for every token,
//      per mode under `data-xui-mode`, plus the shadcn aliases `--primary`…
//      and the app's `--glass-*` set so the Tailwind-built primitives inside a
//      surface wear the same theme), and
//   2. one class per component PART (`xui-<Component>-<part>`) whose rules
//      are the theme's recipes: a `when` on recipe props becomes a
//      `[data-r-<prop>="…"]` attribute selector, a `when` on `state` the
//      pseudo-class (`:hover`, `:active`, `:focus-visible`, `:disabled`) or
//      the Radix state attribute, and in every case the forced-state marker
//      `[data-xs~="<state>"]` the recipe sheet uses.
// Token references stay `var(--xui-…)`, so a mode switch is one attribute
// flip and never a recompile; the three cascade layers (`xui-recipe` <
// `xui-node` < `xui-part`) ARE the painter precedence of
// `resolveNodeStyle` (native recipe < node style < macro part recipe).
// Everything is scoped by a class derived from the theme's content, so two
// surfaces on one page can wear two themes and a host page's own
// Tailwind/shadcn variables are never touched.

import { coreCatalog, parseTokenRef, shadowCss } from "@exponential-at/ui"
import type { FontSpec, RecipeRule, ResolvedTheme, Shadow } from "@exponential-at/ui"

export const LAYERS = `@layer xui-base, xui-recipe, xui-node, xui-part;`

/** Keys whose numbers carry no unit. */
const UNITLESS = new Set([`flexGrow`, `flexShrink`, `opacity`, `fontWeight`, `aspectRatio`, `zIndex`])

const kebab = (key: string) => key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)

/** `$color.primary` → `--xui-color-primary`; `$type.lineHeight.sm` →
 *  `--xui-type-lineHeight-sm` (the group path verbatim); `$type.family.sans`
 *  → `--xui-font-sans`. */
export function tokenVarName(ref: string): string | null {
  const parsed = parseTokenRef(ref)
  if (!parsed) return null
  const { group, name } = parsed
  if (group === `type.family`) return `--xui-font-${name}`
  return `--xui-${group.replace(/\./g, `-`)}-${name}`
}

export function tokenVar(ref: string): string | null {
  const name = tokenVarName(ref)
  return name ? `var(${name})` : null
}

function fontStack(family: string, fonts: Record<string, FontSpec>): string {
  const fallback = fonts[family]?.fallback
  return fallback ? `"${family}", ${fallback}` : `"${family}"`
}

/** One style value → its CSS text; `null` = nothing to emit. */
export function cssValue(key: string, value: unknown, fonts: Record<string, FontSpec> = {}): string | null {
  if (value === undefined || value === null || typeof value === `boolean`) return null
  if (typeof value === `string` && value.startsWith(`$`)) {
    const v = tokenVar(value)
    if (v) return v
  }
  if (key === `boxShadow`) return Array.isArray(value) ? shadowCss(value as Shadow[]) : String(value)
  if (key === `fontFamily`) return fontStack(String(value), fonts)
  if (key === `gridTemplateAreas` && Array.isArray(value)) return value.map((row) => `"${row}"`).join(` `)
  if (typeof value === `number`) return UNITLESS.has(key) ? String(value) : `${value}px`
  if (key === `overflow` && value === `hidden`) return `clip`
  return String(value)
}

/** The logical/shorthand keys the Box whitelist has that CSS does not. */
const SHORTHANDS: Record<string, string[]> = {
  paddingHorizontal: [`padding-left`, `padding-right`],
  paddingVertical: [`padding-top`, `padding-bottom`],
  marginHorizontal: [`margin-left`, `margin-right`],
  marginVertical: [`margin-top`, `margin-bottom`],
}

/** Flat declarations (`prop:value` pairs) for one condition-free style. */
export function declarations(style: Record<string, unknown>, fonts: Record<string, FontSpec> = {}): [string, string][] {
  const out: [string, string][] = []
  for (const [key, value] of Object.entries(style)) {
    if (key === `native` || key.startsWith(`@`) || key.startsWith(`:`)) continue
    if (typeof value === `object` && value !== null && !Array.isArray(value)) continue
    const css = cssValue(key, value, fonts)
    if (css === null) continue
    for (const target of SHORTHANDS[key] ?? [kebab(key)]) out.push([target, css])
  }
  return out
}

export const rule = (selector: string, decls: [string, string][]): string =>
  decls.length === 0 ? `` : `${selector}{${decls.map(([p, v]) => `${p}:${v}`).join(`;`)}}`

// ---------------------------------------------------------------------------
// Interaction states → selectors
// ---------------------------------------------------------------------------

/** Every way an element can be in a state: the pseudo-class or the attribute
 *  Radix sets, plus the forced marker the recipe sheet writes. */
export const STATE_SELECTORS: Record<string, string[]> = {
  hover: [`:hover`, `[data-xs~="hover"]`],
  pressed: [`:active`, `[data-xs~="pressed"]`],
  focus: [`:focus-visible`, `[data-xs~="focus"]`],
  disabled: [`:disabled`, `[data-disabled]`, `[aria-disabled="true"]`, `[data-xs~="disabled"]`],
  checked: [`[data-state="checked"]`, `[data-xs~="checked"]`],
  open: [`[data-state="open"]`, `[data-xs~="open"]`],
  selected: [`[data-state="active"]`, `[data-state="on"]`, `[data-highlighted]`, `[data-xs~="selected"]`],
}

function stateSuffixes(states: string[]): string[] {
  let out = [``]
  for (const state of states) {
    const alts = STATE_SELECTORS[state] ?? [`[data-xs~="${state}"]`]
    out = out.flatMap((prefix) => alts.map((alt) => prefix + alt))
  }
  return out
}

/** A native's recipe props ride as `data-r-<prop>`, a macro part's as
 *  `data-m-<prop>`: a Badge's label is a Text, and both key on `variant`. */
export const propAttribute = (macro: boolean, prop: string): string => `data-${macro ? `m` : `r`}-${kebab(prop)}`

function propSelector(when: Record<string, unknown> | undefined, macro: boolean): string[] {
  if (!when) return [``]
  let out = [``]
  for (const [key, want] of Object.entries(when)) {
    if (key === `state`) continue
    const values = Array.isArray(want) ? want : [want]
    out = out.flatMap((prefix) => values.map((v) => `${prefix}[${propAttribute(macro, key)}="${String(v)}"]`))
  }
  return out
}

/** The selectors one recipe rule applies to, for the part's class. */
export function ruleSelectors(base: string, rule: RecipeRule, macro = false): string[] {
  const states = rule.when?.state === undefined ? [] : Array.isArray(rule.when.state) ? rule.when.state.map(String) : [String(rule.when.state)]
  const props = propSelector(rule.when, macro)
  const suffixes = stateSuffixes(states)
  return props.flatMap((p) => suffixes.map((s) => `${base}${p}${s}`))
}

// ---------------------------------------------------------------------------
// The compiled theme
// ---------------------------------------------------------------------------

export interface CompiledTheme {
  /** The class on the surface root AND on every element it paints. */
  scope: string
  /** The whole sheet: layer order, variables (both modes), recipes. */
  css: string
  theme: ResolvedTheme
}

/** FNV-1a over the resolved theme, so two different themes with one id get
 *  two scopes and the same theme always gets the same one (stable DOM). */
function hash(text: string): string {
  let h = 0x811c9dc5
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return (h >>> 0).toString(36)
}

export function themeScope(theme: ResolvedTheme): string {
  return `xui-t-${theme.id.replace(/[^a-z0-9-]/gi, `-`)}-${hash(JSON.stringify(theme))}`
}

/** The shadcn variable names a Tailwind-built primitive reads, mapped onto
 *  the theme's colour tokens (`--primary: var(--xui-color-primary)`). */
const SHADCN_ALIASES: [string, string][] = [
  [`--background`, `background`],
  [`--foreground`, `foreground`],
  [`--card`, `card`],
  [`--card-foreground`, `cardForeground`],
  [`--popover`, `popover`],
  [`--popover-foreground`, `popoverForeground`],
  [`--primary`, `primary`],
  [`--primary-foreground`, `primaryForeground`],
  [`--secondary`, `secondary`],
  [`--secondary-foreground`, `secondaryForeground`],
  [`--muted`, `muted`],
  [`--muted-foreground`, `mutedForeground`],
  [`--accent`, `accent`],
  [`--accent-foreground`, `accentForeground`],
  [`--destructive`, `destructive`],
  [`--destructive-foreground`, `destructiveForeground`],
  [`--success`, `success`],
  [`--success-foreground`, `successForeground`],
  [`--warning`, `warning`],
  [`--warning-foreground`, `warningForeground`],
  [`--info`, `info`],
  [`--info-foreground`, `infoForeground`],
  [`--border`, `border`],
  [`--input`, `input`],
  [`--ring`, `ring`],
  [`--chart-1`, `chart1`],
  [`--chart-2`, `chart2`],
  [`--chart-3`, `chart3`],
  [`--chart-4`, `chart4`],
  [`--chart-5`, `chart5`],
  [`--sidebar`, `background`],
  [`--sidebar-foreground`, `foreground`],
  [`--sidebar-primary`, `primary`],
  [`--sidebar-primary-foreground`, `primaryForeground`],
  [`--sidebar-accent`, `accent`],
  [`--sidebar-accent-foreground`, `accentForeground`],
  [`--sidebar-border`, `border`],
  [`--sidebar-ring`, `ring`],
  // The app's glass set (packages/ui/src/styles.css), so the moved
  // primitives keep their look from the theme's semantic colours.
  [`--glass-fill-section`, `muted`],
  [`--glass-fill-row`, `muted`],
  [`--glass-fill-card`, `card`],
  [`--glass-fill-panel`, `popover`],
  [`--glass-fill-active`, `accent`],
  [`--glass-stroke-row`, `border`],
  [`--glass-stroke-section`, `border`],
  [`--glass-stroke-card`, `border`],
  [`--glass-stroke-strong`, `input`],
  [`--glass-stroke-active`, `ring`],
  [`--glass-background-top`, `background`],
  [`--glass-background-bottom`, `background`],
]

function tokenBlock(theme: ResolvedTheme): string {
  const decls: [string, string][] = []
  const tokens = theme.tokens as unknown as Record<string, Record<string, number | string>>
  for (const group of [`spacing`, `radius`, `control`, `opacity`, `border`, `motion`]) {
    for (const [name, value] of Object.entries(tokens[group] ?? {})) {
      const unit = group === `opacity` ? `` : group === `motion` ? `ms` : `px`
      decls.push([`--xui-${group}-${name}`, `${value}${unit}`])
    }
  }
  for (const [name, value] of Object.entries(theme.tokens.type.size)) decls.push([`--xui-type-size-${name}`, `${value}px`])
  for (const [name, value] of Object.entries(theme.tokens.type.lineHeight)) decls.push([`--xui-type-lineHeight-${name}`, `${value}px`])
  for (const [name, value] of Object.entries(theme.tokens.type.weight)) decls.push([`--xui-type-weight-${name}`, String(value)])
  for (const [name, family] of Object.entries(theme.tokens.type.family)) decls.push([`--xui-font-${name}`, fontStack(family, theme.fonts)])
  decls.push([`--radius`, `var(--xui-radius-lg)`], [`--font-sans`, `var(--xui-font-sans)`], [`--font-mono`, `var(--xui-font-mono)`])
  decls.push([`--xui-duration-fast`, `var(--xui-motion-fast)`], [`--xui-duration-standard`, `var(--xui-motion-standard)`])
  return decls.map(([p, v]) => `${p}:${v}`).join(`;`)
}

function modeBlock(theme: ResolvedTheme, mode: `light` | `dark`): string {
  const decls: [string, string][] = []
  for (const [name, hex] of Object.entries(theme.modes[mode].color)) decls.push([`--xui-color-${name}`, hex])
  for (const [name, layers] of Object.entries(theme.modes[mode].shadow)) decls.push([`--xui-shadow-${name}`, shadowCss(layers)])
  for (const [alias, token] of SHADCN_ALIASES) decls.push([alias, `var(--xui-color-${token})`])
  decls.push([`color-scheme`, mode])
  return decls.map(([p, v]) => `${p}:${v}`).join(`;`)
}

const MACROS = new Set(Object.entries(coreCatalog.components).filter(([, d]) => d.kind === `macro`).map(([n]) => n))

/** Whether a component's parts go in the `xui-part` layer (macro parts win
 *  over an author's style) or the `xui-recipe` layer (a native's own look
 *  yields to it). Extension macros are listed by the caller. */
export function isMacroComponent(component: string, extensionMacros: ReadonlySet<string> = new Set()): boolean {
  return MACROS.has(component) || extensionMacros.has(component)
}

export const partClass = (component: string, part: string): string => `xui-${component}-${part}`

export function compileTheme(theme: ResolvedTheme, options: { extensionMacros?: ReadonlySet<string> } = {}): CompiledTheme {
  const scope = themeScope(theme)
  const root = `.xui-surface.${scope}`
  const chunks: string[] = [LAYERS]
  chunks.push(`${root}{${tokenBlock(theme)}}`)
  chunks.push(`${root}[data-xui-mode="light"]{${modeBlock(theme, `light`)}}`)
  chunks.push(`${root}[data-xui-mode="dark"]{${modeBlock(theme, `dark`)}}`)
  const recipe: string[] = []
  const part: string[] = []
  for (const [component, parts] of Object.entries(theme.recipes)) {
    const macro = isMacroComponent(component, options.extensionMacros)
    const target = macro ? part : recipe
    for (const [partName, rules] of Object.entries(parts)) {
      const base = `.${scope}.${partClass(component, partName)}`
      for (const r of rules) {
        const decls = declarations(r.style as Record<string, unknown>, theme.fonts)
        if (`borderWidth` in r.style) decls.push([`border-style`, `solid`])
        if (decls.length === 0) continue
        target.push(rule(ruleSelectors(base, r, macro).join(`,`), decls))
      }
    }
  }
  chunks.push(`@layer xui-recipe{${recipe.join(``)}}`)
  chunks.push(`@layer xui-part{${part.join(``)}}`)
  return { scope, css: chunks.join(`\n`), theme }
}

const cache = new WeakMap<ResolvedTheme, CompiledTheme>()

/** `compileTheme`, memoized per resolved theme object (the common case:
 *  a built-in or a theme the host loaded once). */
export function compiledTheme(theme: ResolvedTheme, options?: { extensionMacros?: ReadonlySet<string> }): CompiledTheme {
  if (options?.extensionMacros && options.extensionMacros.size > 0) return compileTheme(theme, options)
  let hit = cache.get(theme)
  if (!hit) {
    hit = compileTheme(theme)
    cache.set(theme, hit)
  }
  return hit
}
