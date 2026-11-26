// #EXP-2109: THE header actions, the pure reference of
// `fixtures/header-actions.json` (its `$comment` is the spec). Every client
// replays it: one order, one round button, + never inside the `…`.

import fixture from "../fixtures/header-actions.json" with { type: "json" }

export type HeaderAction = `status` | `primary` | `call` | `page` | `search` | `inbox` | `add` | `more`
export type HeaderPage = keyof typeof fixture.pages

export const HEADER_ACTIONS_ORDER = fixture.order as readonly HeaderAction[]
export const HEADER_ACTION_ICONS = fixture.icons
export const HEADER_CIRCLE = {
  phone: { circle: fixture.circle, glyph: fixture.glyph },
  wide: { circle: fixture.wideCircle, glyph: fixture.wideGlyph },
} as const
export const HEADER_GAP = fixture.gap

/** A page's actions in order; null = the page has no header on that form. */
export function headerActions(page: HeaderPage, form: `phone` | `wide`): HeaderAction[] | null {
  const actions = fixture.pages[page][form] as HeaderAction[] | null
  return actions && orderHeaderActions(actions)
}

export function orderHeaderActions(actions: readonly HeaderAction[]): HeaderAction[] {
  return HEADER_ACTIONS_ORDER.filter((action) => actions.includes(action))
}

export const HEADER_MENU_DROPS = fixture.menuDrops as readonly string[]

/** The `…` of a header: the page's menu without what the + does (and Open on
 *  the page itself), separators tidied; empty = no button. */
export function headerMenu<T extends string>(entries: readonly T[]): T[] {
  return entries
    .filter((entry) => !HEADER_MENU_DROPS.includes(entry))
    .filter((entry, index, all) => entry !== `separator` || (index > 0 && index < all.length - 1 && all[index - 1] !== `separator`))
}

/** #EXP-2183: the trailing chips a narrow bar may fold to their glyph. */
export type HeaderCollapsible = `primary` | `status`
export type HeaderCollapseChip = { action: HeaderCollapsible; full: number; compact: number }

export const HEADER_TITLE_MIN_SHARE = fixture.collapse.titleMinShare
export const HEADER_COLLAPSE_ORDER = fixture.collapse.order as readonly HeaderCollapsible[]

/** COLLAPSE: fold the chips in `collapse.order` until the title gets at
 *  least `titleMinShare` of the bar (or its whole intrinsic width when
 *  shorter); returns the folded actions in fold order. `fixed` = what never
 *  folds (back, disc, call, +, …, gaps, padding). */
export function headerCollapse({
  bar,
  title,
  fixed,
  chips,
}: {
  bar: number
  title: number
  fixed: number
  chips: readonly HeaderCollapseChip[]
}): HeaderCollapsible[] {
  const target = Math.min(title, HEADER_TITLE_MIN_SHARE * bar)
  let used = fixed + chips.reduce((sum, chip) => sum + chip.full, 0)
  const folded: HeaderCollapsible[] = []
  for (const action of HEADER_COLLAPSE_ORDER) {
    if (bar - used >= target) break
    const chip = chips.find((entry) => entry.action === action)
    if (!chip) continue
    used -= chip.full - chip.compact
    folded.push(action)
  }
  return folded
}
