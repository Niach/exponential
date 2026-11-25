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
