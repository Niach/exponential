// VAPP-87: a surface's NODE stylesheet (layer `xui-node`). One rule per node
// from its whitelisted `style` (tokens as `var(--xui-…)`), then every
// `@media (min-width: Npx)` as a CONTAINER query on the surface's `xui`
// container (so a breakpoint resolves against the SURFACE width, like the
// core) in ascending order, then `:pressed` as `:active`. Scoped by the
// surface id, so two surfaces with the same node ids never collide.

import type { FontSpec, UiNode } from "@exponential-at/ui"
import { declarations, rule } from "./theme-css"

const MEDIA = /^@media \(min-width: (\d+(?:\.\d+)?)px\)$/

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

/** The node sheet for one tree. */
export function nodeSheet(roots: UiNode | readonly UiNode[], surfaceId: string, fonts: Record<string, FontSpec> = {}): string {
  const scope = `.${surfaceClass(surfaceId)}`
  const base: string[] = []
  const media: [number, string][] = []
  const pressed: string[] = []
  // VAPP-91: template components render per item but are reduced apart from
  // the tree; the surface passes them as extra roots so their styles land.
  for (const node of (Array.isArray(roots) ? roots : [roots]).flatMap((r) => [...walkNodes(r)])) {
    const style = node.style
    if (!style) continue
    const sel = `${scope} .${nodeClass(node.id)}`
    const decls = declarations(style, fonts)
    if (`borderWidth` in style) decls.push([`border-style`, `solid`])
    if (decls.length) base.push(rule(sel, decls))
    for (const [key, value] of Object.entries(style)) {
      if (typeof value !== `object` || value === null || Array.isArray(value)) continue
      const m = MEDIA.exec(key)
      const block = declarations(value as Record<string, unknown>, fonts)
      if (`borderWidth` in (value as object)) block.push([`border-style`, `solid`])
      if (block.length === 0) continue
      if (m) media.push([parseFloat(m[1]), `@container xui (min-width: ${m[1]}px){${rule(sel, block)}}`])
      else if (key === `:pressed`) pressed.push(rule(`${sel}:active,${sel}[data-xs~="pressed"]`, block))
    }
  }
  media.sort((a, b) => a[0] - b[0])
  return `@layer xui-node{${[...base, ...media.map(([, css]) => css), ...pressed].join(``)}}`
}
