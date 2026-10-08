// VAPP-87 + round 1: a surface's NODE stylesheet (layer `xui-node`). Per
// node, in this order (contract §2, `resolveConditions` is the reference):
//   1. its whitelisted `style` (tokens as `var(--xui-…)`);
//   2. every `@media` block in SOURCE order (later wins): width conditions
//      as CONTAINER queries on the surface's `xui` container (`min-width` =
//      `width >= N`, `max-width` = `width < N`, `$breakpoint.*` resolved from
//      the theme), height and orientation as attribute-gated rules the
//      surface sets from its measured box (`data-xq`, a container query
//      cannot ask an inline-size container for its height), `hover` and
//      `prefers-reduced-motion` as real media queries;
//   3. the state blocks `:hover`, `:focus-visible`, `:pressed`.
// A style value the macro expander left DYNAMIC (a binding or a call, e.g.
// Progress `width: percent{…}`) becomes `var(--xd-<n>)`; `NodeView` sets
// the resolved value on the element, so the layer order (native recipe <
// node style < macro part) still holds. Template subtrees are included
// (their items render under the template node's own class), so a templated
// row wears its author and macro styles (audit bug B).

import { parseMediaCondition, parseTokenRef, type FontSpec, type ResolvedTheme, type UiNode } from "@exponential-at/ui"
import { hasDynamic } from "./data"
import { declarations, rule } from "./theme-css"

export const nodeClass = (id: string): string => `xui-n-${cssEscape(id)}`
export const surfaceClass = (surfaceId: string): string => `xui-s-${cssEscape(surfaceId)}`

/** Ids are author strings; keep the class usable in a selector. */
export function cssEscape(text: string): string {
  return text.replace(/[^a-zA-Z0-9_-]/g, (c) => `_${c.charCodeAt(0).toString(16)}_`)
}

export function* walkNodes(root: UiNode): Generator<UiNode> {
  yield root
  for (const slot of Object.values(root.slots ?? {})) yield* walkNodes(slot)
  for (const child of root.children) yield* walkNodes(child)
}

/** One dynamic style value of a node: where it sits and its variable. */
export interface DynamicStyleEntry {
  /** `--xd-<n>`, numbered in walk order (base keys, then each block). */
  name: string
  /** The condition key (`@media …`, `:hover`) or `` for the base style. */
  block: string
  key: string
  value: unknown
}

/** The node's dynamic style values in the order the sheet numbers them. */
export function dynamicStyleEntries(style: Record<string, unknown> | undefined): DynamicStyleEntry[] {
  const out: DynamicStyleEntry[] = []
  if (!style) return out
  const visit = (block: string, s: Record<string, unknown>) => {
    for (const [key, value] of Object.entries(s)) {
      if (key.startsWith(`@`) || key.startsWith(`:`)) continue
      if (hasDynamic(value)) out.push({ name: `--xd-${out.length}`, block, key, value })
    }
  }
  visit(``, style)
  for (const [key, value] of Object.entries(style)) {
    if ((key.startsWith(`@`) || key.startsWith(`:`)) && typeof value === `object` && value !== null) visit(key, value as Record<string, unknown>)
  }
  return out
}

/** A JS-evaluated condition (height / orientation): its `data-xq` token. */
export function queryToken(key: string): string {
  return cssEscape(key.replace(/^@media \(|\)$/g, ``).replace(/: /, `-`))
}

export interface NodeSheet {
  css: string
  /** The resolved condition keys the surface must evaluate itself and list
   *  in `data-xq` when they hold (height, orientation). */
  queries: string[]
}

function breakpointPx(value: string, theme: ResolvedTheme | undefined): number | null {
  const ref = parseTokenRef(value)
  if (ref) {
    if (ref.group !== `breakpoint`) return null
    const px = theme?.tokens.breakpoint?.[ref.name]
    return typeof px === `number` ? px : null
  }
  const n = parseFloat(value)
  return Number.isFinite(n) ? n : null
}

/** The node sheet for one tree (plus the template subtrees it renders). */
export function compileNodeSheet(roots: readonly UiNode[], surfaceId: string, theme?: ResolvedTheme, fonts: Record<string, FontSpec> = theme?.fonts ?? {}): NodeSheet {
  const scope = `.${surfaceClass(surfaceId)}`
  const rules: string[] = []
  const queries = new Set<string>()
  const seen = new Set<string>()
  for (const root of roots) {
    for (const node of walkNodes(root)) {
      const style = node.style
      if (!style || seen.has(node.id)) continue
      seen.add(node.id)
      const dyn = dynamicStyleEntries(style)
      const withVars = (block: string, s: Record<string, unknown>) => {
        const mine = dyn.filter((d) => d.block === block)
        if (mine.length === 0) return s
        const copy = { ...s }
        for (const d of mine) copy[d.key] = `var(${d.name})`
        return copy
      }
      const cls = nodeClass(node.id)
      const sel = `${scope} .${cls}`
      const base = declarations(withVars(``, style), fonts)
      if (base.length) rules.push(rule(sel, base))
      for (const [key, value] of Object.entries(style)) {
        if (!key.startsWith(`@`) || typeof value !== `object` || value === null) continue
        const block = declarations(withVars(key, value as Record<string, unknown>), fonts)
        if (block.length === 0) continue
        const condition = parseMediaCondition(key)
        if (!condition) continue
        switch (condition.feature) {
          case `min-width`:
          case `max-width`: {
            const px = breakpointPx(condition.value, theme)
            if (px === null) break // an unknown token never matches
            const op = condition.feature === `min-width` ? `>=` : `<`
            rules.push(`@container xui (width ${op} ${px}px){${rule(sel, block)}}`)
            break
          }
          case `min-height`:
          case `max-height`: {
            const px = breakpointPx(condition.value, theme)
            if (px === null) break
            const resolved = `@media (${condition.feature}: ${px}px)`
            queries.add(resolved)
            rules.push(rule(`${scope}:where([data-xq~="${queryToken(resolved)}"]) .${cls}`, block))
            break
          }
          case `orientation`:
            queries.add(key)
            rules.push(rule(`${scope}:where([data-xq~="${queryToken(key)}"]) .${cls}`, block))
            break
          case `hover`:
          case `prefers-reduced-motion`:
            rules.push(`@media (${condition.feature}: ${condition.value}){${rule(sel, block)}}`)
            break
        }
      }
      const states: [string, string][] = [
        [`:hover`, `${sel}:hover,${sel}[data-xs~="hover"]`],
        [`:focus-visible`, `${sel}:focus-visible,${sel}[data-xs~="focus-visible"]`],
        [`:pressed`, `${sel}:active,${sel}[data-xs~="pressed"]`],
      ]
      for (const [key, selector] of states) {
        const value = style[key]
        if (typeof value !== `object` || value === null) continue
        const block = declarations(withVars(key, value as Record<string, unknown>), fonts)
        if (block.length) rules.push(rule(selector, block))
      }
    }
  }
  return { css: `@layer xui-node{${rules.join(``)}}`, queries: [...queries] }
}

/** The node sheet's CSS for one tree (the VAPP-87 entry point). */
export function nodeSheet(root: UiNode, surfaceId: string, fontsOrTheme: Record<string, FontSpec> | ResolvedTheme = {}, extra: readonly UiNode[] = []): string {
  const theme = isTheme(fontsOrTheme) ? fontsOrTheme : undefined
  const fonts = theme ? theme.fonts : (fontsOrTheme as Record<string, FontSpec>)
  return compileNodeSheet([root, ...extra], surfaceId, theme, fonts).css
}

function isTheme(v: unknown): v is ResolvedTheme {
  return typeof v === `object` && v !== null && `tokens` in v && `modes` in v
}
