// VAPP-85: the Box style whitelist as code — the VAPP-4 spike's `vapp-css`
// (`create`/`props` in StyleX's shape, zero dependencies) over the key list
// in catalog/style.json. `create` is identity + validation; `props` merges
// in order and keeps the conditions NESTED, because the CLIENT resolves
// `@media (…)` against its surface and the state keys against its own
// interaction state (D7). Values may be token references (`$spacing.md`).
//
// Round 1 (docs/round-1-contract.md §2): `resolveConditions` is the
// REFERENCE flattening every renderer mirrors — base, then every matching
// `@media` block in SOURCE order, then `:hover`, `:focus-visible`,
// `:pressed`; fixtures/style-conditions.json locks it.

import styleJson from "../catalog/style.json" with { type: "json" }
import { TOKEN_GROUPS, isKnownToken, parseTokenRef } from "./catalog"

interface KeySpec {
  type?: string
  enum?: readonly (string | number)[]
  group?: string
  rootOnly?: boolean
  layoutEffect?: boolean
}

const LAYOUT = styleJson.layout as Record<string, KeySpec>
const VISUAL = styleJson.visual as Record<string, KeySpec>
export const STYLE_KEYS: readonly string[] = [
  ...Object.keys(LAYOUT),
  ...Object.keys(VISUAL),
]
export const STYLE_LAYOUT_KEYS: readonly string[] = Object.keys(LAYOUT)
export const STYLE_VISUAL_KEYS: readonly string[] = Object.keys(VISUAL)
/** The visual keys that move layout (`borderWidth` and the four sides). */
export const STYLE_LAYOUT_EFFECT_KEYS: readonly string[] = Object.entries(VISUAL).filter(([, s]) => s.layoutEffect).map(([k]) => k)
const SPEC: Record<string, KeySpec> = { ...LAYOUT, ...VISUAL }
const KEYS: ReadonlySet<string> = new Set(STYLE_KEYS)
export const STYLE_MEDIA = new RegExp(styleJson.conditions.media)
export const STYLE_TRANSFORM = new RegExp(styleJson.conditions.transform)
/** The state keys in the order they apply (the last wins). */
export const STYLE_STATES: readonly string[] = styleJson.conditions.states
export const STYLE_MEDIA_PATTERN: string = styleJson.conditions.media
export const STYLE_TRANSFORM_PATTERN: string = styleJson.conditions.transform

type Len = number | `${number}%` | `${number}px` | `auto` | `$${string}`
type Color = `#${string}` | `$color.${string}`
type Align =
  | `flex-start`
  | `flex-end`
  | `start`
  | `end`
  | `center`
  | `stretch`
  | `baseline`
type Distribute = Align | `space-between` | `space-around` | `space-evenly`
type Overflow = `visible` | `hidden` | `clip` | `scroll` | `auto`

/** A gradient painted over `backgroundColor`: CSS angle (0 = up, 90 = right)
 *  and colour stops at 0..1 offsets. */
export interface Gradient {
  angle: number
  stops: { color: Color; offset: number }[]
}

/** Typed keys = editor autocomplete + a compile error for anything else. */
export interface StyleProps {
  display?: `flex` | `grid` | `block` | `none`
  flexDirection?: `row` | `column` | `row-reverse` | `column-reverse`
  flexWrap?: `nowrap` | `wrap` | `wrap-reverse`
  justifyContent?: Distribute
  alignItems?: Align
  alignContent?: Distribute
  alignSelf?: Align
  justifySelf?: Align
  flexGrow?: number
  flexShrink?: number
  flexBasis?: Len
  gap?: Len
  rowGap?: Len
  columnGap?: Len
  direction?: `ltr` | `rtl`
  overflow?: Overflow
  overflowX?: Overflow
  overflowY?: Overflow
  width?: Len
  height?: Len
  minWidth?: Len
  minHeight?: Len
  maxWidth?: Len
  maxHeight?: Len
  aspectRatio?: number | `${number}/${number}`
  position?: `relative` | `absolute`
  top?: Len
  right?: Len
  bottom?: Len
  left?: Len
  inset?: Len
  insetInlineStart?: Len
  insetInlineEnd?: Len
  insetBlockStart?: Len
  insetBlockEnd?: Len
  padding?: Len
  paddingTop?: Len
  paddingRight?: Len
  paddingBottom?: Len
  paddingLeft?: Len
  paddingHorizontal?: Len
  paddingVertical?: Len
  paddingInlineStart?: Len
  paddingInlineEnd?: Len
  margin?: Len
  marginTop?: Len
  marginRight?: Len
  marginBottom?: Len
  marginLeft?: Len
  marginHorizontal?: Len
  marginVertical?: Len
  marginInlineStart?: Len
  marginInlineEnd?: Len
  /** Track list: px, %, fr, auto, min-content, max-content, minmax(a, b), repeat(n, …). */
  gridTemplateColumns?: string
  gridTemplateRows?: string
  /** One string per row (`["nav main", "nav footer"]`). */
  gridTemplateAreas?: ReadonlyArray<string>
  gridArea?: string
  gridColumn?: string
  gridRow?: string
  backgroundColor?: Color
  backgroundGradient?: Gradient
  color?: Color
  borderWidth?: number | `$border.${string}`
  borderTopWidth?: number | `$border.${string}`
  borderRightWidth?: number | `$border.${string}`
  borderBottomWidth?: number | `$border.${string}`
  borderLeftWidth?: number | `$border.${string}`
  borderColor?: Color
  borderStyle?: `solid` | `dashed` | `dotted`
  borderRadius?: Len
  borderTopLeftRadius?: Len
  borderTopRightRadius?: Len
  borderBottomRightRadius?: Len
  borderBottomLeftRadius?: Len
  opacity?: number | `$opacity.${string}`
  boxShadow?: `$shadow.${string}`
  fontSize?: Len
  fontWeight?: 300 | 400 | 500 | 600 | 700 | 800
  lineHeight?: Len
  letterSpacing?: Len
  textAlign?: `left` | `right` | `center` | `start` | `end` | `justify`
  textDecoration?: `none` | `underline` | `line-through`
  textTransform?: `none` | `uppercase` | `lowercase` | `capitalize`
  fontStyle?: `normal` | `italic`
  fontFamily?: `$type.family.${string}`
  transition?: `$motion.${string}`
  transitionEasing?: `$ease.${string}`
  /** `translate(Xpx, Ypx) scale(N) rotate(Ndeg)`, paint-only. */
  transform?: string
  visibility?: `visible` | `hidden`
  pointerEvents?: `auto` | `none`
  userSelect?: `auto` | `none` | `text`
  cursor?: `auto` | `default` | `pointer` | `text` | `not-allowed` | `grab` | `grabbing` | `move` | `col-resize` | `row-resize`
}

export type MediaKey = `@media (${string})`
export type StateKey = `:hover` | `:focus-visible` | `:pressed`
/** A style = base props + media condition blocks + state blocks, one level. */
export type Style = StyleProps & {
  [K in MediaKey]?: StyleProps
} & {
  [K in StateKey]?: StyleProps
}

const LENGTH = /^-?\d+(\.\d+)?(px|%)$/
const HEX = /^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8}|[0-9a-fA-F]{3,4})$/
const RATIO = /^\d+(\.\d+)?\/\d+(\.\d+)?$/
const NUMERIC_TOKEN_GROUPS = new Set([
  `spacing`,
  `radius`,
  `control`,
  `type.size`,
  `type.lineHeight`,
  `opacity`,
  `border`,
])

function tokenOk(value: unknown, groups: ReadonlySet<string>): boolean {
  const ref = parseTokenRef(value)
  return ref !== null && groups.has(ref.group) && isKnownToken(value)
}

function colorOk(value: unknown): boolean {
  return (typeof value === `string` && HEX.test(value)) || tokenOk(value, new Set([`color`]))
}

function gradientError(key: string, value: unknown): string | null {
  const bad = `${key}: expected {angle, stops: [{color, offset 0..1}, …]} with two or more stops`
  if (typeof value !== `object` || value === null || Array.isArray(value)) return bad
  const g = value as Record<string, unknown>
  if (typeof g.angle !== `number` || !Number.isFinite(g.angle)) return bad
  if (!Array.isArray(g.stops) || g.stops.length < 2) return bad
  for (const stop of g.stops) {
    if (typeof stop !== `object` || stop === null) return bad
    const s = stop as Record<string, unknown>
    if (!colorOk(s.color)) return bad
    if (typeof s.offset !== `number` || s.offset < 0 || s.offset > 1) return bad
  }
  for (const k of Object.keys(g)) if (k !== `angle` && k !== `stops`) return bad
  return null
}

function valueError(key: string, value: unknown): string | null {
  const spec = SPEC[key]
  if (spec.enum) {
    return spec.enum.includes(value as string | number)
      ? null
      : `${key}: expected one of ${spec.enum.join(`|`)}`
  }
  switch (spec.type) {
    case `number`:
      if (typeof value === `number` && Number.isFinite(value)) return null
      if (tokenOk(value, NUMERIC_TOKEN_GROUPS)) return null
      return `${key}: expected a number or a numeric token`
    case `length`:
      if (typeof value === `number` && Number.isFinite(value)) return null
      if (value === `auto`) return null
      if (typeof value === `string` && LENGTH.test(value)) return null
      if (tokenOk(value, NUMERIC_TOKEN_GROUPS)) return null
      return `${key}: expected px, "N%", "auto" or a numeric token`
    case `color`:
      if (colorOk(value)) return null
      return `${key}: expected #hex or $color.<name>`
    case `gradient`:
      return gradientError(key, value)
    case `token`:
      if (tokenOk(value, new Set([spec.group ?? ``]))) return null
      return `${key}: expected $${spec.group}.<name>`
    case `transform`:
      if (typeof value === `string` && STYLE_TRANSFORM.test(value)) return null
      return `${key}: expected translate(Xpx, Ypx), scale(N) and/or rotate(Ndeg)`
    case `ratio`:
      if (typeof value === `number` && value > 0) return null
      if (typeof value === `string` && RATIO.test(value)) return null
      return `${key}: expected a number or "w/h"`
    case `tracks`:
    case `string`:
      return typeof value === `string` ? null : `${key}: expected a string`
    case `areas`:
      return Array.isArray(value) && value.every((row) => typeof row === `string`)
        ? null
        : `${key}: expected an array of row strings`
    default:
      return `${key}: unknown spec`
  }
}

export interface StyleIssue {
  path: string
  message: string
}

/** The enum a style key takes (`borderStyle` → solid|dashed|dotted), if any. */
export function styleKeyEnum(key: string): readonly (string | number)[] | undefined {
  return SPEC[key]?.enum
}

/** The value type a style key takes (`length`, `color`, `token`, …). */
export function styleKeyType(key: string): string | undefined {
  return SPEC[key]?.enum ? `enum` : SPEC[key]?.type
}

/** A `@media` or state condition key. */
export function isConditionKey(key: string): boolean {
  return STYLE_MEDIA.test(key) || STYLE_STATES.includes(key)
}

/** Every rule the client resolver relies on: whitelisted keys, well-typed
 *  values, conditions one level deep, `direction` on the root only. */
export function validateStyle(
  style: unknown,
  options: { path?: string; root?: boolean } = {}
): StyleIssue[] {
  const issues: StyleIssue[] = []
  const path = options.path ?? `style`
  const walk = (obj: Record<string, unknown>, at: string, nested: boolean) => {
    for (const [key, value] of Object.entries(obj)) {
      if (value === undefined) continue
      if (isConditionKey(key)) {
        const bp = /\$breakpoint\.([a-zA-Z0-9]+)/.exec(key)
        if (bp && !TOKEN_GROUPS.breakpoint.includes(bp[1])) issues.push({ path: `${at}.${key}`, message: `unknown breakpoint $breakpoint.${bp[1]}; known: ${TOKEN_GROUPS.breakpoint.join(`|`)}` })
        if (nested) issues.push({ path: `${at}.${key}`, message: `conditions do not nest` })
        else if (typeof value !== `object` || value === null || Array.isArray(value))
          issues.push({ path: `${at}.${key}`, message: `must be an object` })
        else walk(value as Record<string, unknown>, `${at}.${key}`, true)
        continue
      }
      if (!KEYS.has(key)) {
        issues.push({ path: `${at}.${key}`, message: key.startsWith(`@media`) || key.startsWith(`:`) ? `not a supported condition` : `not in the Box style whitelist` })
        continue
      }
      if (SPEC[key].rootOnly && options.root === false)
        issues.push({ path: `${at}.${key}`, message: `allowed on the root only` })
      const error = valueError(key, value)
      if (error) issues.push({ path: `${at}.${key}`, message: error })
    }
  }
  if (typeof style !== `object` || style === null || Array.isArray(style))
    return [{ path, message: `expected an object` }]
  walk(style as Record<string, unknown>, path, false)
  return issues
}

/** Identity + validation, frozen so a render cannot mutate a shared style.
 *  `Record<K, Style>` (not a generic) keeps the excess-property check on the
 *  literal, so an unknown key is a compile error too. */
export function create<K extends string>(
  styles: Record<K, Style>
): Readonly<Record<K, Style>> {
  for (const [name, style] of Object.entries(styles)) {
    const issues = validateStyle(style, { path: name })
    if (issues.length > 0)
      throw new Error(issues.map((i) => `${i.path}: ${i.message}`).join(`\n`))
  }
  return Object.freeze(styles)
}

type Falsy = false | null | undefined | 0 | ``

/** Merge in order (last wins, StyleX semantics). Condition blocks merge KEY
 *  BY KEY with the same rule, and stay nested: the output is exactly what the
 *  wire carries (plain JSON) and what the client resolver reads. */
export function props(
  ...styles: ReadonlyArray<Style | Falsy | ReadonlyArray<Style | Falsy>>
): Style {
  const out: Record<string, unknown> = {}
  const add = (s: Style | Falsy | ReadonlyArray<Style | Falsy>) => {
    if (!s) return
    if (Array.isArray(s)) {
      ;(s as ReadonlyArray<Style | Falsy>).forEach(add)
      return
    }
    for (const [k, v] of Object.entries(s as Record<string, unknown>)) {
      if (v === undefined) continue
      if (isConditionKey(k)) {
        out[k] = { ...(out[k] as object | undefined), ...(v as object) }
      } else {
        out[k] = v
      }
    }
  }
  styles.forEach(add)
  return out as Style
}

// ---------------------------------------------------------------------------
// Condition resolution (the reference the renderers mirror)
// ---------------------------------------------------------------------------

/** What a client knows when it flattens a style: the SURFACE box, the
 *  platform's pointer and motion preference, the node's interaction states
 *  and the theme's breakpoint values (px). */
export interface ConditionContext {
  width: number
  height?: number
  /** A hover-capable pointer (mouse, trackpad); false on touch. */
  hover?: boolean
  /** The platform asks for reduced motion. */
  reducedMotion?: boolean
  /** The node's active states, as the state keys without the colon
   *  (`hover`, `focus-visible`, `pressed`). */
  states?: readonly string[]
  /** `$breakpoint.<name>` → px; a block naming an unknown token never matches. */
  breakpoints?: Record<string, number>
}

export interface MediaCondition {
  feature: `min-width` | `max-width` | `min-height` | `max-height` | `orientation` | `hover` | `prefers-reduced-motion`
  /** The px value or the `$breakpoint.<name>` token for the size features;
   *  the keyword for the others. */
  value: string
}

const MEDIA_PARSE = /^@media \(([a-z-]+): ([^)]+)\)$/

/** `"@media (min-width: $breakpoint.md)"` → `{feature, value}`; null when the
 *  key is not a media condition of the grammar. */
export function parseMediaCondition(key: string): MediaCondition | null {
  if (!STYLE_MEDIA.test(key)) return null
  const m = MEDIA_PARSE.exec(key)
  if (!m) return null
  return { feature: m[1] as MediaCondition[`feature`], value: m[2] }
}

function sizeValue(value: string, ctx: ConditionContext): number | null {
  const ref = parseTokenRef(value)
  if (ref) {
    if (ref.group !== `breakpoint`) return null
    const px = ctx.breakpoints?.[ref.name]
    return typeof px === `number` ? px : null
  }
  const n = parseFloat(value)
  return Number.isFinite(n) ? n : null
}

/** Does a media condition hold for the context? `min-*` = size >= N,
 *  `max-*` = size < N, orientation portrait = height >= width (landscape
 *  when the height is unknown). */
export function mediaMatches(condition: MediaCondition, ctx: ConditionContext): boolean {
  switch (condition.feature) {
    case `min-width`:
    case `max-width`:
    case `min-height`:
    case `max-height`: {
      const n = sizeValue(condition.value, ctx)
      if (n === null) return false
      const size = condition.feature.endsWith(`width`) ? ctx.width : ctx.height
      if (size === undefined) return false
      return condition.feature.startsWith(`min`) ? size >= n : size < n
    }
    case `orientation`: {
      const portrait = ctx.height !== undefined && ctx.height >= ctx.width
      return condition.value === (portrait ? `portrait` : `landscape`)
    }
    case `hover`:
      return condition.value === (ctx.hover ? `hover` : `none`)
    case `prefers-reduced-motion`:
      return condition.value === (ctx.reducedMotion ? `reduce` : `no-preference`)
  }
}

/** Flatten a style for one pass: the base keys, then every matching `@media`
 *  block in source order, then the matching state blocks in STYLE_STATES
 *  order. Token values pass through untouched (the theme resolves them). */
export function resolveConditions(style: Record<string, unknown>, ctx: ConditionContext): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(style)) if (!isConditionKey(k) && v !== undefined) out[k] = v
  for (const [k, v] of Object.entries(style)) {
    const condition = parseMediaCondition(k)
    if (!condition || typeof v !== `object` || v === null) continue
    if (mediaMatches(condition, ctx)) Object.assign(out, v as Record<string, unknown>)
  }
  const states = ctx.states ?? []
  for (const key of STYLE_STATES) {
    const block = style[key]
    if (states.includes(key.slice(1)) && typeof block === `object` && block !== null) Object.assign(out, block as Record<string, unknown>)
  }
  return out
}

/** The breakpoint a surface of this width is in: the LAST `$breakpoint`
 *  name (tokens.json order, ascending) whose px is <= width; null below the
 *  first (= `base`). A native's responsive prop takes
 *  `responsiveAt(value, activeBreakpoint(width, theme.tokens.breakpoint))`. */
export function activeBreakpoint(width: number, breakpoints: Record<string, number>): string | null {
  let out: string | null = null
  for (const name of TOKEN_GROUPS.breakpoint) {
    const px = breakpoints[name]
    if (typeof px === `number` && width >= px) out = name
  }
  return out
}
