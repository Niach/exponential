// VAPP-92: the theme loader and resolver — the reference every renderer
// mirrors (the Rust core for desktop/iOS/Android, the React renderer on web).
// `validateTheme` turns a theme file into a list of readable issues (never a
// throw), `loadTheme` flattens its `extends` chain into a `ResolvedTheme`
// (throwing ONE `ThemeError` that lists every issue) and `resolveRecipe`
// answers a painter's question for one mode with concrete values.
//
// Precedence a painter applies (`resolveNodeStyle`): the native's own
// recipe (Text/root for its variant) < the node's style (a macro template's
// structure plus the author's style) < the macro part's recipe (Badge/label).
// A theme therefore restyles what a template drew, and an author's style on
// a plain native still wins over the theme.
//
// Round 1: `applyDensity` / `applyContrast` derive the theme a surface runs
// with from its settings ONCE; `resolveMode` turns `system` into a mode;
// `resolveStyleValues` also resolves `$breakpoint.*` inside media keys.

import { TOKEN_GROUPS, parseTokenRef } from "./catalog"
import { isThemeHex } from "./color"
import { RECIPE_KEYS, RECIPE_STATES, nativeRecipeProps, recipeParts } from "./recipes"
import type { PartSpec } from "./recipes"
import { STYLE_MEDIA, STYLE_TRANSFORM, styleKeyEnum } from "./style"
import type { ExtensionDef, UiNode } from "./types"
import type {
  Density,
  Easing,
  ModeName,
  ModeSetting,
  RecipeQuery,
  RecipeRule,
  RecipeStyle,
  ResolvedStyle,
  ResolvedTheme,
  ResolvedValue,
  Shadow,
  ThemeIssue,
  ThemeMode,
  ThemeRecipes,
  ThemeSource,
  ThemeTokens,
} from "./theme-types"

export const MODES: readonly ModeName[] = [`light`, `dark`]
export const DENSITIES: readonly Density[] = [`compact`, `default`, `comfortable`]
export const THEME_SCHEMA_ID = `https://ui.exponential.at/schemas/theme/v1.json`

const ID = /^[a-z][a-z0-9-]*$/
/** Token groups whose values are per-mode (under `modes`), not under `tokens`. */
const MODE_GROUPS = new Set([`color`, `shadow`])
const STRING_GROUPS = new Set([`type.family`])
/** Token groups whose values are cubic-bezier tuples. */
const TUPLE_GROUPS = new Set([`ease`])
/** The mode-less NUMBER groups under `tokens`, in file order. */
const NUMBER_GROUPS = [`spacing`, `radius`, `control`, `opacity`, `border`, `motion`, `breakpoint`, `density`] as const
const TYPE_SUBGROUPS = [`size`, `lineHeight`, `weight`, `family`] as const
/** Which token groups each recipe key accepts. */
const KEY_GROUPS: Record<string, readonly string[]> = {
  backgroundColor: [`color`],
  color: [`color`],
  borderColor: [`color`],
  borderWidth: [`border`, `control`],
  borderTopWidth: [`border`, `control`],
  borderRightWidth: [`border`, `control`],
  borderBottomWidth: [`border`, `control`],
  borderLeftWidth: [`border`, `control`],
  borderRadius: [`radius`, `spacing`],
  borderStyle: [],
  padding: [`spacing`],
  paddingHorizontal: [`spacing`],
  paddingVertical: [`spacing`],
  gap: [`spacing`],
  width: [`control`, `spacing`, `border`],
  height: [`control`, `spacing`, `border`],
  minWidth: [`control`, `spacing`],
  minHeight: [`control`, `spacing`],
  fontSize: [`type.size`],
  fontWeight: [`type.weight`],
  lineHeight: [`type.lineHeight`],
  fontFamily: [`type.family`],
  letterSpacing: [],
  textDecoration: [],
  textTransform: [],
  fontStyle: [],
  boxShadow: [`shadow`],
  opacity: [`opacity`],
  transition: [`motion`],
  transitionEasing: [`ease`],
  transform: [],
}
const COLOR_KEYS = new Set([`backgroundColor`, `color`, `borderColor`])
const TOKEN_ONLY_KEYS = new Set([`fontFamily`, `boxShadow`, `transition`, `transitionEasing`])
const ENUM_KEYS = new Set([`borderStyle`, `textDecoration`, `textTransform`, `fontStyle`])

export class ThemeError extends Error {
  readonly issues: ThemeIssue[]
  constructor(id: string, issues: ThemeIssue[]) {
    super(`Theme "${id}" is invalid:\n${issues.map((i) => `  ${i.path}: ${i.message}`).join(`\n`)}`)
    this.name = `ThemeError`
    this.issues = issues
  }
}

export interface ThemeOptions {
  /** Themes an `extends` may name (sources or already resolved). */
  themes?: readonly (ThemeSource | ResolvedTheme)[]
  extensions?: readonly ExtensionDef[]
}

type Obj = Record<string, unknown>
const isObj = (v: unknown): v is Obj => typeof v === `object` && v !== null && !Array.isArray(v)
const known = (names: readonly string[]) => names.join(`|`)

// ---------------------------------------------------------------------------
// Validation (one file, no chain knowledge beyond "does the parent exist")
// ---------------------------------------------------------------------------

function checkNumberMap(value: unknown, path: string, names: readonly string[], issues: ThemeIssue[], range?: [number, number]): void {
  if (!isObj(value)) return void issues.push({ path, message: `expected an object of token values` })
  for (const [name, v] of Object.entries(value)) {
    if (!names.includes(name)) issues.push({ path: `${path}.${name}`, message: `unknown token; known: ${known(names)}` })
    else if (typeof v !== `number` || !Number.isFinite(v)) issues.push({ path: `${path}.${name}`, message: `expected a number` })
    else if (range && (v < range[0] || v > range[1])) issues.push({ path: `${path}.${name}`, message: `expected ${range[0]}–${range[1]}` })
  }
}

function checkEasingMap(value: unknown, path: string, names: readonly string[], issues: ThemeIssue[]): void {
  if (!isObj(value)) return void issues.push({ path, message: `expected an object of easing curves` })
  for (const [name, v] of Object.entries(value)) {
    if (!names.includes(name)) issues.push({ path: `${path}.${name}`, message: `unknown token; known: ${known(names)}` })
    else if (!Array.isArray(v) || v.length !== 4 || !v.every((n) => typeof n === `number` && Number.isFinite(n))) issues.push({ path: `${path}.${name}`, message: `expected [x1, y1, x2, y2]` })
  }
}

function checkShadow(value: unknown, path: string, issues: ThemeIssue[]): void {
  if (!Array.isArray(value)) return void issues.push({ path, message: `expected a list of shadow layers` })
  value.forEach((layer, i) => {
    const at = `${path}[${i}]`
    if (!isObj(layer)) return void issues.push({ path: at, message: `expected {x, y, blur, spread, color}` })
    for (const k of [`x`, `y`, `blur`, `spread`]) if (typeof layer[k] !== `number`) issues.push({ path: `${at}.${k}`, message: `expected a number` })
    if (!isThemeHex(layer.color)) issues.push({ path: `${at}.color`, message: `expected #rrggbb or #rrggbbaa` })
  })
}

function checkMode(value: unknown, path: string, issues: ThemeIssue[]): void {
  if (!isObj(value)) return void issues.push({ path, message: `expected {color, shadow}` })
  for (const [group, entries] of Object.entries(value)) {
    if (!MODE_GROUPS.has(group)) {
      issues.push({ path: `${path}.${group}`, message: `unknown mode group; known: color|shadow` })
      continue
    }
    if (!isObj(entries)) {
      issues.push({ path: `${path}.${group}`, message: `expected an object of token values` })
      continue
    }
    const names = TOKEN_GROUPS[group]
    for (const [name, v] of Object.entries(entries)) {
      const at = `${path}.${group}.${name}`
      if (!names.includes(name)) issues.push({ path: at, message: `unknown token; known: ${known(names)}` })
      else if (group === `color` && !isThemeHex(v)) issues.push({ path: at, message: `expected #rrggbb or #rrggbbaa (lowercase); the builder converts oklch/hsl on import` })
      else if (group === `shadow`) checkShadow(v, at, issues)
    }
  }
}

function checkModes(value: unknown, path: string, issues: ThemeIssue[]): void {
  if (!isObj(value)) return void issues.push({ path, message: `expected {light, dark}` })
  for (const [mode, v] of Object.entries(value)) {
    if (!MODES.includes(mode as ModeName)) issues.push({ path: `${path}.${mode}`, message: `unknown mode; known: light|dark` })
    else checkMode(v, `${path}.${mode}`, issues)
  }
}

function checkTokens(value: unknown, path: string, issues: ThemeIssue[]): void {
  if (!isObj(value)) return void issues.push({ path, message: `expected the token groups` })
  for (const [group, entries] of Object.entries(value)) {
    const at = `${path}.${group}`
    if (group === `type`) {
      if (!isObj(entries)) {
        issues.push({ path: at, message: `expected {size, lineHeight, weight, family}` })
        continue
      }
      for (const [sub, map] of Object.entries(entries)) {
        const g = `type.${sub}`
        if (!TOKEN_GROUPS[g]) issues.push({ path: `${at}.${sub}`, message: `unknown type group; known: size|lineHeight|weight|family` })
        else if (STRING_GROUPS.has(g)) {
          if (!isObj(map)) issues.push({ path: `${at}.${sub}`, message: `expected an object of family names` })
          else
            for (const [name, v] of Object.entries(map)) {
              if (!TOKEN_GROUPS[g].includes(name)) issues.push({ path: `${at}.${sub}.${name}`, message: `unknown token; known: ${known(TOKEN_GROUPS[g])}` })
              else if (typeof v !== `string` || v.length === 0) issues.push({ path: `${at}.${sub}.${name}`, message: `expected a font family name` })
            }
        } else checkNumberMap(map, `${at}.${sub}`, TOKEN_GROUPS[g], issues)
      }
      continue
    }
    if (MODE_GROUPS.has(group)) {
      issues.push({ path: at, message: `${group} values live under modes.light / modes.dark` })
      continue
    }
    if (!TOKEN_GROUPS[group]) {
      issues.push({ path: at, message: `unknown token group; known: ${known(Object.keys(TOKEN_GROUPS).filter((g) => !g.startsWith(`type.`) && !MODE_GROUPS.has(g)))}|type` })
      continue
    }
    if (TUPLE_GROUPS.has(group)) checkEasingMap(entries, at, TOKEN_GROUPS[group], issues)
    else checkNumberMap(entries, at, TOKEN_GROUPS[group], issues, group === `opacity` ? [0, 1] : group === `density` ? [0.5, 2] : undefined)
  }
}

function checkFonts(value: unknown, path: string, issues: ThemeIssue[]): void {
  if (!isObj(value)) return void issues.push({ path, message: `expected an object keyed by family name` })
  for (const [family, spec] of Object.entries(value)) {
    const at = `${path}.${family}`
    if (!isObj(spec)) {
      issues.push({ path: at, message: `expected {fallback?, weights?, source?}` })
      continue
    }
    for (const key of Object.keys(spec)) if (![`fallback`, `weights`, `source`].includes(key)) issues.push({ path: `${at}.${key}`, message: `unknown font key; known: fallback|weights|source` })
    if (spec.fallback !== undefined && typeof spec.fallback !== `string`) issues.push({ path: `${at}.fallback`, message: `expected a string` })
    if (spec.weights !== undefined && (!Array.isArray(spec.weights) || !spec.weights.every((w) => typeof w === `number`))) issues.push({ path: `${at}.weights`, message: `expected a list of numbers` })
    if (spec.source !== undefined && spec.source !== `system` && spec.source !== `host`) issues.push({ path: `${at}.source`, message: `expected system|host` })
  }
}

function checkRecipeValue(key: string, value: unknown, path: string, issues: ThemeIssue[]): void {
  if (key === `native`) {
    if (typeof value !== `boolean`) issues.push({ path, message: `expected true|false` })
    return
  }
  if (ENUM_KEYS.has(key)) {
    const allowed = styleKeyEnum(key) ?? []
    if (!allowed.includes(value as string)) issues.push({ path, message: `expected ${known(allowed.map(String))}` })
    return
  }
  if (key === `transform`) {
    if (typeof value !== `string` || !STYLE_TRANSFORM.test(value)) issues.push({ path, message: `expected translate(Xpx, Ypx), scale(N) and/or rotate(Ndeg)` })
    return
  }
  const groups = KEY_GROUPS[key]
  const ref = parseTokenRef(value)
  if (ref) {
    if (!groups.includes(ref.group)) issues.push({ path, message: groups.length ? `expected a $${groups.join(`|$`)} token, got $${ref.group}` : `expected a number, not a token` })
    else if (!TOKEN_GROUPS[ref.group]?.includes(ref.name)) issues.push({ path, message: `unknown token ${value as string}; known: ${known(TOKEN_GROUPS[ref.group] ?? [])}` })
    return
  }
  if (COLOR_KEYS.has(key)) {
    if (!isThemeHex(value)) issues.push({ path, message: `expected #rrggbb, #rrggbbaa or $color.<name>` })
    return
  }
  if (TOKEN_ONLY_KEYS.has(key)) {
    issues.push({ path, message: `expected a $${groups[0]}.<name> token` })
    return
  }
  const weights = styleKeyEnum(`fontWeight`) ?? []
  if (typeof value !== `number` || !Number.isFinite(value)) issues.push({ path, message: groups.length ? `expected a number or a $${groups.join(`|$`)} token` : `expected a number` })
  else if (key === `opacity` && (value < 0 || value > 1)) issues.push({ path, message: `expected 0–1` })
  else if (key === `fontWeight` && !weights.includes(value)) issues.push({ path, message: `expected ${known(weights.map(String))} or $type.weight.<name>` })
}

function checkRule(rule: unknown, path: string, spec: PartSpec, issues: ThemeIssue[]): void {
  if (!isObj(rule)) return void issues.push({ path, message: `expected {when?, style}` })
  for (const key of Object.keys(rule)) if (key !== `when` && key !== `style`) issues.push({ path: `${path}.${key}`, message: `unknown rule key; known: when|style` })
  if (rule.when !== undefined) {
    if (!isObj(rule.when)) issues.push({ path: `${path}.when`, message: `expected an object of recipe props` })
    else
      for (const [k, v] of Object.entries(rule.when)) {
        const at = `${path}.when.${k}`
        if (k === `state`) {
          const states = Array.isArray(v) ? v : [v]
          for (const s of states) if (!RECIPE_STATES.includes(s as string)) issues.push({ path: at, message: `unknown state; known: ${known(RECIPE_STATES)}` })
        } else if (!spec.props.includes(k)) issues.push({ path: at, message: `not a recipe prop of this part; known: ${known([`state`, ...spec.props])}` })
        else {
          const ok = (x: unknown) => [`string`, `number`, `boolean`].includes(typeof x)
          if (!(ok(v) || (Array.isArray(v) && v.every(ok)))) issues.push({ path: at, message: `expected a value or a list of values` })
        }
      }
  }
  if (!isObj(rule.style)) return void issues.push({ path: `${path}.style`, message: `expected a style object` })
  for (const [key, value] of Object.entries(rule.style)) {
    const at = `${path}.style.${key}`
    if (!RECIPE_KEYS.includes(key)) issues.push({ path: at, message: `not a recipe key; known: ${known(RECIPE_KEYS)}` })
    else checkRecipeValue(key, value, at, issues)
  }
}

function checkRecipes(value: unknown, path: string, parts: Record<string, PartSpec>, issues: ThemeIssue[]): void {
  if (!isObj(value)) return void issues.push({ path, message: `expected an object keyed by component` })
  for (const [component, byPart] of Object.entries(value)) {
    const at = `${path}.${component}`
    const spec = parts[component]
    if (!spec) {
      issues.push({ path: at, message: `unknown component` })
      continue
    }
    if (!isObj(byPart)) {
      issues.push({ path: at, message: `expected an object keyed by part; known: ${known(spec.parts)}` })
      continue
    }
    for (const [part, rules] of Object.entries(byPart)) {
      const atPart = `${at}.${part}`
      if (!spec.parts.includes(part)) {
        issues.push({ path: atPart, message: `unknown part; known: ${known(spec.parts)}` })
        continue
      }
      if (!Array.isArray(rules)) {
        issues.push({ path: atPart, message: `expected a list of rules [{when?, style}]` })
        continue
      }
      rules.forEach((rule, i) => checkRule(rule, `${atPart}[${i}]`, spec, issues))
    }
  }
}

const THEME_KEYS = [`$schema`, `$comment`, `id`, `name`, `extends`, `modes`, `contrast`, `tokens`, `fonts`, `recipes`]

/** Every problem in ONE theme file, each with a path and a readable
 *  message. Never throws: a non-object is one issue. */
export function validateTheme(source: unknown, options: ThemeOptions = {}): ThemeIssue[] {
  const issues: ThemeIssue[] = []
  if (!isObj(source)) return [{ path: `theme`, message: `expected a theme object {id, name, modes, tokens, recipes}` }]
  if (typeof source.id !== `string` || !ID.test(source.id)) issues.push({ path: `id`, message: `expected a kebab-case id` })
  if (typeof source.name !== `string` || source.name.length === 0) issues.push({ path: `name`, message: `expected a name` })
  for (const key of Object.keys(source))
    if (!THEME_KEYS.includes(key)) issues.push({ path: key, message: `unknown theme key; known: id|name|extends|modes|contrast|tokens|fonts|recipes` })
  if (source.extends !== undefined) {
    const parents = (options.themes ?? []).map((t) => t.id)
    if (typeof source.extends !== `string`) issues.push({ path: `extends`, message: `expected a theme id` })
    else if (!parents.includes(source.extends)) issues.push({ path: `extends`, message: `unknown theme "${source.extends}"; known: ${known(parents)}` })
    else if (source.extends === source.id) issues.push({ path: `extends`, message: `a theme cannot extend itself` })
  }
  if (source.modes !== undefined) checkModes(source.modes, `modes`, issues)
  if (source.contrast !== undefined) checkModes(source.contrast, `contrast`, issues)
  if (source.tokens !== undefined) checkTokens(source.tokens, `tokens`, issues)
  if (source.fonts !== undefined) checkFonts(source.fonts, `fonts`, issues)
  if (source.recipes !== undefined) checkRecipes(source.recipes, `recipes`, recipeParts(options.extensions), issues)
  return issues
}

// ---------------------------------------------------------------------------
// Resolution: flatten the chain, check completeness
// ---------------------------------------------------------------------------

const emptyMode = (): ThemeMode => ({ color: {}, shadow: {} })

function emptyTheme(source: ThemeSource): ResolvedTheme {
  return {
    id: source.id,
    name: source.name,
    chain: [],
    modes: { light: emptyMode(), dark: emptyMode() },
    contrast: { light: emptyMode(), dark: emptyMode() },
    tokens: { spacing: {}, radius: {}, type: { size: {}, lineHeight: {}, weight: {}, family: {} }, control: {}, opacity: {}, border: {}, motion: {}, breakpoint: {}, ease: {}, density: {} },
    fonts: {},
    recipes: {},
  }
}

function isResolved(t: ThemeSource | ResolvedTheme): t is ResolvedTheme {
  return Array.isArray((t as ResolvedTheme).chain)
}

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T
}

function overlayMode(target: ThemeMode, source: Partial<ThemeMode> | undefined): void {
  if (!source) return
  Object.assign(target.color, source.color ?? {})
  Object.assign(target.shadow, clone(source.shadow ?? {}))
}

function overlay(target: ResolvedTheme, source: ThemeSource): void {
  for (const mode of MODES) {
    overlayMode(target.modes[mode], source.modes?.[mode])
    overlayMode(target.contrast[mode], source.contrast?.[mode])
  }
  const t = source.tokens
  if (t) {
    for (const group of NUMBER_GROUPS) Object.assign(target.tokens[group], t[group] ?? {})
    Object.assign(target.tokens.ease, clone(t.ease ?? {}))
    for (const sub of TYPE_SUBGROUPS) Object.assign(target.tokens.type[sub], t.type?.[sub] ?? {})
  }
  Object.assign(target.fonts, clone(source.fonts ?? {}))
  for (const [component, byPart] of Object.entries(source.recipes ?? {})) {
    const parts = (target.recipes[component] ??= {})
    for (const [part, rules] of Object.entries(byPart)) (parts[part] ??= []).push(...clone(rules))
  }
}

function overlayResolved(target: ResolvedTheme, parent: ResolvedTheme): void {
  for (const mode of MODES) {
    overlayMode(target.modes[mode], parent.modes[mode])
    overlayMode(target.contrast[mode], parent.contrast?.[mode])
  }
  target.tokens = clone(parent.tokens)
  target.fonts = clone(parent.fonts)
  target.recipes = clone(parent.recipes)
}

/** The token names the merged theme still lacks, as issues. */
function completeness(theme: ResolvedTheme): ThemeIssue[] {
  const issues: ThemeIssue[] = []
  for (const [group, names] of Object.entries(TOKEN_GROUPS)) {
    for (const name of names) {
      if (MODE_GROUPS.has(group)) {
        for (const mode of MODES) {
          const table = theme.modes[mode][group as `color` | `shadow`] as Record<string, unknown>
          if (table[name] === undefined) issues.push({ path: `modes.${mode}.${group}.${name}`, message: `missing value for $${group}.${name}` })
        }
        continue
      }
      const table = group.startsWith(`type.`)
        ? (theme.tokens.type as unknown as Record<string, Record<string, unknown>>)[group.slice(5)]
        : (theme.tokens as unknown as Record<string, Record<string, unknown>>)[group]
      if (table?.[name] === undefined) issues.push({ path: `tokens.${group}.${name}`, message: `missing value for $${group}.${name}` })
    }
  }
  for (const family of Object.values(theme.tokens.type.family)) {
    if (!theme.fonts[family]) issues.push({ path: `fonts.${family}`, message: `family named by tokens.type.family has no fonts entry` })
  }
  return issues
}

/** The source(s) a chain needs, root first; null when a link is missing. */
function chainOf(source: ThemeSource, options: ThemeOptions): (ThemeSource | ResolvedTheme)[] | ThemeIssue {
  const registry = options.themes ?? []
  const chain: (ThemeSource | ResolvedTheme)[] = [source]
  const seen = new Set([source.id])
  let current: ThemeSource | ResolvedTheme = source
  while (!isResolved(current) && current.extends) {
    const parentId: string = current.extends
    const parent = registry.find((t) => t.id === parentId)
    if (!parent) return { path: `extends`, message: `unknown theme "${parentId}"` }
    if (seen.has(parent.id)) return { path: `extends`, message: `cycle: ${[...seen, parent.id].join(` → `)}` }
    seen.add(parent.id)
    chain.unshift(parent)
    current = parent
  }
  return chain
}

/** Validate + flatten, returning issues instead of throwing. */
export function tryLoadTheme(source: unknown, options: ThemeOptions = {}): { theme: ResolvedTheme | null; issues: ThemeIssue[] } {
  const issues = validateTheme(source, options)
  if (issues.length > 0) return { theme: null, issues }
  const src = source as ThemeSource
  const chain = chainOf(src, options)
  if (!Array.isArray(chain)) return { theme: null, issues: [chain] }
  const theme = emptyTheme(src)
  for (const link of chain) {
    if (isResolved(link)) {
      overlayResolved(theme, link)
      theme.chain.push(...link.chain)
      continue
    }
    if (link !== src) {
      const parentIssues = validateTheme(link, options)
      if (parentIssues.length > 0) return { theme: null, issues: parentIssues.map((i) => ({ path: `extends(${link.id}).${i.path}`, message: i.message })) }
    }
    overlay(theme, link)
    theme.chain.push(link.id)
  }
  const missing = completeness(theme)
  if (missing.length > 0) return { theme: null, issues: missing }
  return { theme, issues: [] }
}

/** A theme file → the flat theme a painter uses. Throws `ThemeError` with
 *  every issue listed, never anything else. */
export function loadTheme(source: unknown, options: ThemeOptions = {}): ResolvedTheme {
  const { theme, issues } = tryLoadTheme(source, options)
  if (!theme) throw new ThemeError(isObj(source) && typeof source.id === `string` ? source.id : `?`, issues)
  return theme
}

// ---------------------------------------------------------------------------
// Surface settings: mode, density, contrast
// ---------------------------------------------------------------------------

/** `system` → what the platform prefers; a mode stays itself. */
export function resolveMode(setting: ModeSetting, systemPrefersDark: boolean): ModeName {
  if (setting === `system`) return systemPrefersDark ? `dark` : `light`
  return setting
}

function scaled(table: Record<string, number>, factor: number): Record<string, number> {
  const out: Record<string, number> = {}
  for (const [k, v] of Object.entries(table)) out[k] = Math.round(v * factor)
  return out
}

/** The theme a surface of this density runs with: `control` and `spacing`
 *  scaled by the theme's `$density.<name>` multiplier and rounded to whole
 *  px; `default` returns the theme itself. Done once per surface, so every
 *  resolver and fixture stays density-agnostic. */
export function applyDensity(theme: ResolvedTheme, density: Density): ResolvedTheme {
  if (density === `default`) return theme
  const factor = theme.tokens.density[density]
  if (typeof factor !== `number`) return theme
  return { ...theme, tokens: { ...theme.tokens, control: scaled(theme.tokens.control, factor), spacing: scaled(theme.tokens.spacing, factor) } }
}

/** The theme with its high-contrast overlays merged into the modes (the
 *  platform asked for more contrast). A theme without overlays is unchanged. */
export function applyContrast(theme: ResolvedTheme): ResolvedTheme {
  const modes = {} as Record<ModeName, ThemeMode>
  for (const mode of MODES) {
    modes[mode] = { color: { ...theme.modes[mode].color, ...theme.contrast[mode].color }, shadow: { ...theme.modes[mode].shadow, ...theme.contrast[mode].shadow } }
  }
  return { ...theme, modes }
}

// ---------------------------------------------------------------------------
// Resolving values for one mode
// ---------------------------------------------------------------------------

/** `$color.primary` in `dark` → `#e5e5e5`; `$spacing.md` → 12; `$shadow.sm`
 *  → the layers; `$type.family.sans` → `Inter`; `$ease.standard` → the four
 *  control points. Undefined when not a token. */
export function resolveToken(theme: ResolvedTheme, ref: unknown, mode: ModeName): ResolvedValue | undefined {
  const parsed = parseTokenRef(ref)
  if (!parsed) return undefined
  const { group, name } = parsed
  if (group === `color`) return theme.modes[mode].color[name]
  if (group === `shadow`) return theme.modes[mode].shadow[name]
  if (group.startsWith(`type.`)) return (theme.tokens.type as unknown as Record<string, Record<string, ResolvedValue>>)[group.slice(5)]?.[name]
  return (theme.tokens as unknown as Record<string, Record<string, ResolvedValue>>)[group]?.[name]
}

const BREAKPOINT_REF = /\$breakpoint\.([a-zA-Z0-9]+)/

/** A media key with its `$breakpoint.<name>` replaced by the theme's px. */
export function resolveConditionKey(theme: ResolvedTheme, key: string): string {
  if (!STYLE_MEDIA.test(key)) return key
  return key.replace(BREAKPOINT_REF, (whole, name: string) => {
    const px = theme.tokens.breakpoint[name]
    return typeof px === `number` ? `${px}px` : whole
  })
}

/** Every token reference in a style object replaced by its value for the
 *  mode (nested condition blocks, gradients and arrays included, and the
 *  `$breakpoint` tokens inside media keys); other values pass through. */
export function resolveStyleValues<T extends Record<string, unknown>>(theme: ResolvedTheme, style: T, mode: ModeName): T {
  const resolve = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(resolve)
    if (isObj(value)) return resolveStyleValues(theme, value, mode)
    if (typeof value === `string` && value.startsWith(`$`)) {
      const resolved = resolveToken(theme, value, mode)
      return resolved === undefined ? value : resolved
    }
    return value
  }
  const out: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(style)) out[resolveConditionKey(theme, key)] = resolve(value)
  return out as T
}

function matches(rule: RecipeRule, props: Record<string, unknown>, states: readonly string[]): boolean {
  if (!rule.when) return true
  for (const [key, want] of Object.entries(rule.when)) {
    if (key === `state`) {
      const needed = Array.isArray(want) ? want : [want]
      if (!needed.every((s) => states.includes(s as string))) return false
      continue
    }
    const actual = props[key]
    if (Array.isArray(want) ? !want.includes(actual as string) : actual !== want) return false
  }
  return true
}

/** How specific a rule is: one point per `when` condition (a `state` list
 *  counts each state). VAPP-90: rules merge in SPECIFICITY order, like the
 *  CSS the web renderer compiles them to — a base rule (no `when`) never
 *  shadows a `checked`/`focus`/`variant` rule that sits before it, which is
 *  what a child theme's appended base override used to do under plain
 *  source order. Ties keep source order (later wins). */
export function ruleSpecificity(rule: RecipeRule): number {
  if (!rule.when) return 0
  let n = 0
  for (const [key, want] of Object.entries(rule.when)) n += key === `state` && Array.isArray(want) ? want.length : 1
  return n
}

/** The matching rules' styles merged by specificity (token refs kept). */
export function recipeStyle(theme: ResolvedTheme, query: RecipeQuery): RecipeStyle {
  const rules = theme.recipes[query.component]?.[query.part] ?? []
  const props = query.props ?? {}
  const states = query.states ?? []
  const ordered = rules.map((rule, index) => ({ rule, index, specificity: ruleSpecificity(rule) })).sort((a, b) => a.specificity - b.specificity || a.index - b.index)
  const out: RecipeStyle = {}
  for (const { rule } of ordered) if (matches(rule, props, states)) Object.assign(out, rule.style)
  return out
}

/** A part's concrete visuals for one mode: the fixture
 *  `theme-recipes.json` locks this per theme × component. */
export function resolveRecipe(theme: ResolvedTheme, query: RecipeQuery, mode: ModeName): ResolvedStyle {
  return resolveStyleValues(theme, recipeStyle(theme, query) as Record<string, unknown>, mode) as ResolvedStyle
}

/** The recipe query a NORMALIZED node answers to: its macro part, or the
 *  native's root with its own recipe props. */
export function nodeRecipeQuery(node: UiNode, extensions: readonly ExtensionDef[] = []): RecipeQuery {
  if (node.recipe) return { component: node.recipe.macro, part: node.recipe.part, props: node.recipe.props }
  return { component: node.component, part: `root`, props: nativeRecipeProps(node.component, node.props, extensions) }
}

/** What a painter paints a node with: native recipe < node style < macro
 *  part recipe, token references resolved, condition blocks kept nested
 *  (the client resolves `@media` and the state keys itself). */
export function resolveNodeStyle(
  theme: ResolvedTheme,
  node: UiNode,
  mode: ModeName,
  states: readonly string[] = [],
  extensions: readonly ExtensionDef[] = []
): ResolvedStyle {
  const own = resolveRecipe(theme, { component: node.component, part: `root`, props: nativeRecipeProps(node.component, node.props, extensions), states }, mode)
  const style = node.style ? resolveStyleValues(theme, node.style, mode) : {}
  const part = node.recipe ? resolveRecipe(theme, { ...nodeRecipeQuery(node, extensions), states }, mode) : {}
  return { ...own, ...(style as ResolvedStyle), ...part }
}

/** The shadow layers as one CSS `box-shadow` string (for web painters). */
export function shadowCss(layers: Shadow[]): string {
  return layers.length === 0 ? `none` : layers.map((l) => `${l.x}px ${l.y}px ${l.blur}px ${l.spread}px ${l.color}`).join(`, `)
}

/** An easing as CSS `cubic-bezier(…)`. */
export function easingCss(curve: Easing): string {
  return `cubic-bezier(${curve.join(`, `)})`
}

/** The parts of the theme a mode-less consumer may list (the builder). */
export function themeTokens(theme: ResolvedTheme): ThemeTokens {
  return theme.tokens
}
export function themeMode(theme: ResolvedTheme, mode: ModeName): ThemeMode {
  return theme.modes[mode]
}
export type { ThemeRecipes }
