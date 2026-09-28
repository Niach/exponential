// The alternative authoring package (VAPP-4 spike prototype): StyleX's
// `create`/`props` SHAPE, zero dependencies, no compiler. `create` is identity
// plus whitelist validation; `props` merges in order and keeps the nested
// conditions intact, because per D7 the CLIENT resolves `@media (min-width)`
// against its surface width and `:pressed` against its own press state.

/** Copy of `vapp_spike::WHITELIST` (apps/desktop/crates/vapp-spike/src/lib.rs). */
export const WHITELIST = [
  `display`, `flexDirection`, `flexWrap`, `justifyContent`, `alignItems`, `alignContent`,
  `alignSelf`, `justifySelf`, `flexGrow`, `flexShrink`, `flexBasis`, `gap`, `rowGap`,
  `columnGap`, `direction`, `overflow`, `width`, `height`, `minWidth`, `minHeight`,
  `maxWidth`, `maxHeight`, `aspectRatio`, `position`, `top`, `right`, `bottom`, `left`,
  `inset`, `padding`, `paddingTop`, `paddingRight`, `paddingBottom`, `paddingLeft`,
  `paddingHorizontal`, `paddingVertical`, `margin`, `marginTop`, `marginRight`,
  `marginBottom`, `marginLeft`, `marginHorizontal`, `marginVertical`,
  `gridTemplateColumns`, `gridTemplateRows`, `gridTemplateAreas`, `gridArea`, `gridColumn`,
  `gridRow`, `backgroundColor`, `color`, `borderWidth`, `borderColor`, `borderRadius`,
  `opacity`, `boxShadow`, `fontSize`, `fontWeight`, `lineHeight`, `textAlign`,
] as const

type Len = number | `${number}%` | `${number}px` | `auto`
type Color = `#${string}` | `$palette.${string}` | `$semantic.${string}`
type Align = `flex-start` | `flex-end` | `start` | `end` | `center` | `stretch` | `baseline`
type Distribute = Align | `space-between` | `space-around` | `space-evenly`

/** Typed keys = editor autocomplete + a compile error for anything else. */
export interface VappProps {
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
  padding?: Len
  paddingTop?: Len
  paddingRight?: Len
  paddingBottom?: Len
  paddingLeft?: Len
  paddingHorizontal?: Len
  paddingVertical?: Len
  margin?: Len
  marginTop?: Len
  marginRight?: Len
  marginBottom?: Len
  marginLeft?: Len
  marginHorizontal?: Len
  marginVertical?: Len
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
  borderWidth?: number
  borderColor?: Color
  borderRadius?: number
  opacity?: number
  boxShadow?: string
  fontSize?: number
  fontWeight?: 400 | 500 | 600 | 700
  lineHeight?: number
  textAlign?: `left` | `right` | `center`
}

export type MediaKey = `@media (min-width: ${number}px)`
/** A style = base props + surface-width breakpoints + the pressed state. */
export type VappStyle = VappProps & {
  [K in MediaKey]?: VappProps
} & { ":pressed"?: VappProps }

const KEYS: ReadonlySet<string> = new Set(WHITELIST)
const MEDIA = /^@media \(min-width: \d+(\.\d+)?px\)$/

function validate(path: string, style: Record<string, unknown>, nested: boolean) {
  for (const [k, v] of Object.entries(style)) {
    if (MEDIA.test(k) || k === `:pressed`) {
      if (nested) throw new Error(`${path}: conditions do not nest (${k})`)
      if (typeof v !== `object` || v === null || Array.isArray(v)) throw new Error(`${path}.${k}: must be an object`)
      validate(`${path}.${k}`, v as Record<string, unknown>, true)
    } else if (!KEYS.has(k)) {
      throw new Error(`${path}: "${k}" is not in the vApp whitelist`)
    } else if (v !== undefined && typeof v !== `number` && typeof v !== `string` && !(k === `gridTemplateAreas` && Array.isArray(v))) {
      throw new Error(`${path}.${k}: unsupported value ${JSON.stringify(v)}`)
    }
  }
}

/** Identity + validation. Frozen so a render cannot mutate a shared style.
 *  `Record<K, VappStyle>` (not a generic `T`) keeps TypeScript's excess
 *  property check on the literal, so an unknown key is a compile error. */
export function create<K extends string>(styles: Record<K, VappStyle>): Readonly<Record<K, VappStyle>> {
  for (const [name, style] of Object.entries(styles)) validate(name, style as Record<string, unknown>, false)
  return Object.freeze(styles)
}

type Falsy = false | null | undefined | 0 | ``

/**
 * Merge in order (last wins, StyleX semantics). Condition blocks merge KEY BY
 * KEY with the same rule, and stay nested: the output is exactly what the
 * wire carries (plain JSON) and what the client resolver reads.
 */
export function props(...styles: ReadonlyArray<VappStyle | Falsy | ReadonlyArray<VappStyle | Falsy>>): VappStyle {
  const out: Record<string, unknown> = {}
  const add = (s: VappStyle | Falsy | ReadonlyArray<VappStyle | Falsy>) => {
    if (!s) return
    if (Array.isArray(s)) return s.forEach(add)
    for (const [k, v] of Object.entries(s as Record<string, unknown>)) {
      if (v === undefined) continue
      if (MEDIA.test(k) || k === `:pressed`) {
        out[k] = { ...(out[k] as object | undefined), ...(v as object) }
      } else {
        out[k] = v
      }
    }
  }
  styles.forEach(add)
  return out as VappStyle
}
