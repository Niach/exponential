// VAPP-85: the Box style whitelist as code — the VAPP-4 spike's `vapp-css`
// (`create`/`props` in StyleX's shape, zero dependencies) over the key list
// in catalog/style.json. `create` is identity + validation; `props` merges
// in order and keeps the conditions NESTED, because the CLIENT resolves
// `@media (min-width)` against its surface width and `:pressed` against its
// own press state (D7). Values may be token references (`$spacing.md`).

import styleJson from "../catalog/style.json" with { type: "json" }
import { isKnownToken, parseTokenRef } from "./catalog"

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
const SPEC: Record<string, KeySpec> = { ...LAYOUT, ...VISUAL }
const KEYS: ReadonlySet<string> = new Set(STYLE_KEYS)
export const STYLE_MEDIA = new RegExp(styleJson.conditions.media)
export const STYLE_STATES: readonly string[] = styleJson.conditions.states

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
  overflow?: `visible` | `hidden` | `clip` | `scroll` | `auto`
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
  color?: Color
  borderWidth?: number | `$border.${string}`
  borderColor?: Color
  borderRadius?: Len
  opacity?: number | `$opacity.${string}`
  boxShadow?: `$shadow.${string}`
  fontSize?: Len
  fontWeight?: 400 | 500 | 600 | 700
  lineHeight?: Len
  textAlign?: `left` | `right` | `center`
  fontFamily?: `$type.family.${string}`
}

export type MediaKey = `@media (min-width: ${number}px)`
/** A style = base props + surface-width breakpoints + the pressed state. */
export type Style = StyleProps & {
  [K in MediaKey]?: StyleProps
} & { ":pressed"?: StyleProps }

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
      if (typeof value === `string` && HEX.test(value)) return null
      if (tokenOk(value, new Set([`color`]))) return null
      return `${key}: expected #hex or $color.<name>`
    case `token`:
      if (tokenOk(value, new Set([spec.group ?? ``]))) return null
      return `${key}: expected $${spec.group}.<name>`
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
      if (STYLE_MEDIA.test(key) || STYLE_STATES.includes(key)) {
        if (nested) issues.push({ path: `${at}.${key}`, message: `conditions do not nest` })
        else if (typeof value !== `object` || value === null || Array.isArray(value))
          issues.push({ path: `${at}.${key}`, message: `must be an object` })
        else walk(value as Record<string, unknown>, `${at}.${key}`, true)
        continue
      }
      if (!KEYS.has(key)) {
        issues.push({ path: `${at}.${key}`, message: `not in the Box style whitelist` })
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
      if (STYLE_MEDIA.test(k) || STYLE_STATES.includes(k)) {
        out[k] = { ...(out[k] as object | undefined), ...(v as object) }
      } else {
        out[k] = v
      }
    }
  }
  styles.forEach(add)
  return out as Style
}
