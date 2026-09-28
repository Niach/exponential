// VAPP-4 spike (throwaway): a StyleX-subset style object → real CSS, written
// so the BROWSER lays the tree out with the same semantics taffy uses in the
// core (`apps/desktop/crates/vapp-spike/src/style.rs`). Anything outside the
// whitelist throws: this is the reference renderer, silent drops would hide
// exactly the disagreements the spike is looking for.
import type { CSSProperties } from "react"
import tokens from "../../../../../packages/design-tokens/tokens.json"
import type { VappNode, VappStyle, VappStyleValue } from "./fixture"
import { CONTAINER_KINDS, walk } from "./fixture"

/** Copy of `vapp_spike::WHITELIST` (lib.rs). */
export const WHITELIST = [
  `display`,
  `flexDirection`,
  `flexWrap`,
  `justifyContent`,
  `alignItems`,
  `alignContent`,
  `alignSelf`,
  `justifySelf`,
  `flexGrow`,
  `flexShrink`,
  `flexBasis`,
  `gap`,
  `rowGap`,
  `columnGap`,
  `direction`,
  `overflow`,
  `width`,
  `height`,
  `minWidth`,
  `minHeight`,
  `maxWidth`,
  `maxHeight`,
  `aspectRatio`,
  `position`,
  `top`,
  `right`,
  `bottom`,
  `left`,
  `inset`,
  `padding`,
  `paddingTop`,
  `paddingRight`,
  `paddingBottom`,
  `paddingLeft`,
  `paddingHorizontal`,
  `paddingVertical`,
  `margin`,
  `marginTop`,
  `marginRight`,
  `marginBottom`,
  `marginLeft`,
  `marginHorizontal`,
  `marginVertical`,
  `gridTemplateColumns`,
  `gridTemplateRows`,
  `gridTemplateAreas`,
  `gridArea`,
  `gridColumn`,
  `gridRow`,
  `backgroundColor`,
  `color`,
  `borderWidth`,
  `borderColor`,
  `borderRadius`,
  `opacity`,
  `boxShadow`,
  `fontSize`,
  `fontWeight`,
  `lineHeight`,
  `textAlign`,
] as const

const WHITELIST_SET: ReadonlySet<string> = new Set(WHITELIST)

/** Numbers that stay unitless; every other number is px. */
const UNITLESS = new Set([`flexGrow`, `flexShrink`, `opacity`, `fontWeight`, `aspectRatio`])

const COLOR_KEYS = new Set([`backgroundColor`, `color`, `borderColor`])

const kebab = (s: string) => s.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)

/** `$palette.card` → the app's CSS variable, `$semantic.green` → tokens.json
 *  hex (styles.css has no semantic variables), `#hex` as is. */
export function resolveColor(value: string): string {
  if (!value.startsWith(`$`)) return value
  const [group, token] = value.slice(1).split(`.`)
  if (group === `palette` && token && token in tokens.palette) {
    return `var(--color-${kebab(token)})`
  }
  if (group === `semantic` && token && token in tokens.semantic && token !== `$comment`) {
    return (tokens.semantic as Record<string, string>)[token]
  }
  throw new Error(`vapp: unknown colour token ${value}`)
}

const isCondition = (key: string) => key.startsWith(`@`) || key.startsWith(`:`)

function mediaMinWidth(key: string): number | null {
  const m = /^@media\s*\(\s*min-width\s*:\s*([\d.]+)(px)?\s*\)$/.exec(key)
  return m ? Number(m[1]) : null
}

function scalar(key: string, v: VappStyleValue): string {
  if (typeof v === `number`) {
    if (key === `aspectRatio`) return String(v)
    return UNITLESS.has(key) ? String(v) : `${v}px`
  }
  if (typeof v === `string`) return COLOR_KEYS.has(key) ? resolveColor(v) : v
  throw new Error(`vapp: ${key} expects a scalar, got ${JSON.stringify(v)}`)
}

/** One flat (condition-free) style map → CSS declarations, in author order. */
export function declarations(style: VappStyle): Array<[string, string]> {
  const out: Array<[string, string]> = []
  for (const [key, v] of Object.entries(style)) {
    if (isCondition(key)) continue
    if (!WHITELIST_SET.has(key)) throw new Error(`vapp: style key ${key} is not whitelisted`)
    switch (key) {
      case `gridTemplateAreas`: {
        const rows = Array.isArray(v) ? v : [String(v)]
        out.push([`grid-template-areas`, rows.map((r) => `"${r}"`).join(` `)])
        break
      }
      case `paddingHorizontal`:
      case `marginHorizontal`: {
        const base = key.startsWith(`padding`) ? `padding` : `margin`
        out.push([`${base}-left`, scalar(key, v)], [`${base}-right`, scalar(key, v)])
        break
      }
      case `paddingVertical`:
      case `marginVertical`: {
        const base = key.startsWith(`padding`) ? `padding` : `margin`
        out.push([`${base}-top`, scalar(key, v)], [`${base}-bottom`, scalar(key, v)])
        break
      }
      case `borderWidth`:
        out.push([`border-style`, `solid`], [`border-width`, scalar(key, v)])
        break
      case `overflow`:
        // taffy maps hidden|clip → Overflow::Clip, which is NOT a scroll
        // container, so its automatic minimum size stays content-based. CSS
        // `overflow: hidden` would zero it; `clip` matches taffy.
        out.push([`overflow`, v === `hidden` ? `clip` : scalar(key, v)])
        break
      default:
        out.push([kebab(key), scalar(key, v)])
    }
  }
  return out
}

/** The same thing as React inline styles (for callers that want an object). */
export function toCSSProperties(style: VappStyle): CSSProperties {
  const out: Record<string, string> = {}
  for (const [prop, value] of declarations(style)) {
    out[prop.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase())] = value
  }
  return out as CSSProperties
}

/** Validate the conditions on a style (keys inside them too). */
function conditions(style: VappStyle): {
  media: Array<[number, VappStyle]>
  pressed: VappStyle | null
} {
  const media: Array<[number, VappStyle]> = []
  let pressed: VappStyle | null = null
  for (const [key, v] of Object.entries(style)) {
    if (!isCondition(key)) continue
    if (typeof v !== `object` || Array.isArray(v)) throw new Error(`vapp: ${key} must be an object`)
    const min = mediaMinWidth(key)
    if (min !== null) media.push([min, v])
    else if (key === `:pressed`) pressed = v
    else throw new Error(`vapp: unsupported condition ${key}`)
  }
  media.sort((a, b) => a[0] - b[0])
  return { media, pressed }
}

/** taffy's defaults, restated for the browser: every node is a border-box
 *  flex row, relative, stretch; min-width stays `auto` (both agree). */
const TAFFY_BASE: Array<[string, string]> = [
  [`display`, `flex`],
  [`flex-direction`, `row`],
  [`box-sizing`, `border-box`],
  [`position`, `relative`],
]

export const nodeClass = (id: string) => `vapp-n-${id}`

const rule = (selector: string, decls: Array<[string, string]>) =>
  `${selector}{${decls.map(([p, v]) => `${p}:${v}`).join(`;`)}}`

/**
 * ONE stylesheet for the whole surface: a base rule per node, then the
 * `@media (min-width)` overrides as CONTAINER queries on the `vapp` container
 * (the surface wrapper, so they resolve against the SURFACE width like the
 * core), ascending, then `:pressed` as `:active`. Leaves get the taffy base
 * too, but `display` is overridden by the renderer's own leaf rule
 * (`leafDisplay`).
 */
export function surfaceStylesheet(
  root: VappNode,
  opts: { rtl: boolean; leafDisplay: string }
): string {
  const base: Array<string> = []
  const media: Array<[number, string]> = []
  const pressed: Array<string> = []
  walk(root, (node) => {
    const style = node.style ?? {}
    const sel = `.${nodeClass(node.id)}`
    const decls = [...TAFFY_BASE]
    if (!CONTAINER_KINDS.has(node.kind)) {
      // A leaf box = the layout box; the control inside it fills it.
      decls[0] = [`display`, opts.leafDisplay]
      decls[1] = [`flex-direction`, `column`]
    }
    decls.push(...declarations(style))
    if (node === root && opts.rtl) decls.push([`direction`, `rtl`])
    base.push(rule(sel, decls))
    const c = conditions(style)
    for (const [min, overrides] of c.media) {
      media.push([min, `@container vapp (min-width: ${min}px){${rule(sel, declarations(overrides))}}`])
    }
    if (c.pressed) pressed.push(rule(`${sel}:active`, declarations(c.pressed)))
  })
  media.sort((a, b) => a[0] - b[0])
  return [...base, ...media.map(([, css]) => css), ...pressed].join(`\n`)
}
