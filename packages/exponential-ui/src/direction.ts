// Round 2 (docs/round-2-contract.md §2, direction): `direction` may sit on
// ANY node and is inherited; everything direction-dependent reads the
// NODE's resolved direction, not the surface's: flex row order, logical
// insets and paddings, mirrored glyphs and `textAlign` start/end (the
// implicit alignment of text is `start`). fixtures/text-direction.json locks
// it. Pure.

import { resolveConditions, type ConditionContext } from "./style"
import type { UiNode } from "./types"

export type Direction = `ltr` | `rtl`
export type PhysicalAlign = `left` | `right` | `center` | `justify`

function ownDirection(node: UiNode, ctx: ConditionContext | undefined): Direction | undefined {
  if (!node.style) return undefined
  const style = ctx ? resolveConditions(node.style, ctx) : node.style
  const d = style.direction
  return d === `ltr` || d === `rtl` ? d : undefined
}

/** Every node's direction: its own style `direction` (resolved against
 *  `ctx` when given), else its parent's; the root's parent is the surface
 *  (catalog/locale.json `textDirection(locale)`). Slots inherit from their
 *  owner. A template's items take the direction of the node that renders
 *  them (call again with that direction for the template node). */
export function nodeDirections(root: UiNode, surface: Direction, ctx?: ConditionContext): Record<string, Direction> {
  const out: Record<string, Direction> = {}
  const walk = (n: UiNode, inherited: Direction) => {
    const dir = ownDirection(n, ctx) ?? inherited
    out[n.id] = dir
    for (const slot of Object.values(n.slots ?? {})) walk(slot, dir)
    for (const child of n.children) walk(child, dir)
  }
  walk(root, surface)
  return out
}

/** `start`/`end` (and an absent value = `start`) as the physical side for
 *  a direction; `left`, `right`, `center` and `justify` stay. */
export function physicalTextAlign(align: unknown, direction: Direction): PhysicalAlign {
  switch (align) {
    case `left`:
    case `right`:
    case `center`:
    case `justify`:
      return align
    case `end`:
      return direction === `rtl` ? `left` : `right`
    default:
      return direction === `rtl` ? `right` : `left`
  }
}

/** A node's text alignment: its style `textAlign`, else a Text's `align`
 *  prop, else `start` — resolved against the node's direction. */
export function nodeTextAlign(node: UiNode, direction: Direction, ctx?: ConditionContext): PhysicalAlign {
  const style = node.style ? (ctx ? resolveConditions(node.style, ctx) : node.style) : {}
  return physicalTextAlign(style.textAlign ?? node.props.align, direction)
}
