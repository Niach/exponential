// N-12: THE DockCard and THE Dock pile of the primitive sets: the class
// recipes and the pile's geometry both renderers bind (React
// @exponential_app/ui-react/primitives `DockCard`/`DockPile`, Solid
// @exponential_app/ui-solid/primitives, the same markup). A DockCard is a
// one-row floating card: the kind glyph and the tinted label, a muted
// caption, trailing meta, ONE title line and the open chevron (or a trailing
// action); several pile like the toasts (`stackGeometry` with the Dock's
// numbers, anchored at the bottom: the front card last, the older ones
// peeking out above it, `visible` of them drawn; expanded = fanned `gap`
// apart). The Exponential app's Dock (packages/ui dock.tsx) paints through
// these; its sheet is the app's.
//
// VAPP-106: the constants are WRITTEN here (a renderer entry ships no
// fixture); packages/ui's dock test holds them equal to the app's
// fixtures/dock.json `stack`.

import { GLASS_CARD_CLASS } from "./glass-card"
import { stackGeometry, type StackConstants, type ToastStackItem } from "./toast"

export const DOCK_PILE_CONSTANTS: StackConstants = { peek: 14, scaleStep: 0.05, visible: 3, gap: 6 }

export type DockCardTint = `plan` | `question` | `muted` | `warning`

export const DOCK_CARD_CLASSES = {
  /** THE card: the glass card (ONE gray: the opaque card fill under the
   *  card hairline), padded. */
  root: `${GLASS_CARD_CLASS} block w-full p-3 text-left outline-none`,
  /** A card that opens (a press, Enter or Space). */
  pressable: `cursor-pointer transition-colors duration-fast hover:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50`,
  /** The glyph + text row; a card behind the front one hides its words. */
  body: `flex items-start gap-2 data-[hidden=true]:opacity-0`,
  icon: `mt-0.5 size-3.5 shrink-0`,
  text: `min-w-0 flex-1`,
  header: `mb-1 flex items-center gap-2`,
  label: `truncate text-xs font-medium`,
  caption: `min-w-0 truncate text-xs text-muted-foreground`,
  meta: `ml-auto shrink-0 text-xs text-muted-foreground`,
  row: `flex min-w-0 items-center gap-2`,
  title: `min-w-0 flex-1 truncate text-sm text-foreground/90`,
  chevron: `size-3.5 shrink-0 text-muted-foreground`,
  trailing: `shrink-0`,
  /** The tint sits on the glyph and the label alone. */
  tint: {
    plan: `text-primary`,
    question: `text-yellow-400`,
    muted: `text-muted-foreground`,
    warning: `text-amber-400`,
  } satisfies Record<DockCardTint, string>,
} as const

export const DOCK_PILE_CLASSES = {
  /** The pile box: its height = the geometry's, animated over `slow`. */
  root: `relative transition-[height] duration-slow ease-standard motion-reduce:transition-none`,
  /** One card, placed by `--offset`/`--scale`; it shrinks about the TOP
   *  edge (the one that peeks out above the front card). */
  item: `absolute inset-x-0 top-(--offset) origin-top scale-(--scale) transition-[top,scale,opacity] duration-slow ease-standard motion-reduce:transition-none data-[visible=false]:pointer-events-none data-[visible=false]:opacity-0`,
} as const

/** The pile's geometry: `stackGeometry` with the Dock's numbers, anchored
 *  at the bottom (the front card is the LAST height). */
export function dockPileGeometry(heights: readonly number[], expanded: boolean): { height: number; items: ToastStackItem[] } {
  return stackGeometry(heights, expanded, true, DOCK_PILE_CONSTANTS)
}
