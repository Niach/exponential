// #EXP-1629 (fixtures/interactions-round1.json `menuPreview`): a Menu item's
// `preview`, the IDE composer menu's side hover card. The pure half every
// painter shares (the Rust core `eui_core::surface` replays the same rule
// through its `HoverTimer`): WHICH row's preview shows, given where the
// pointer is, where the keyboard highlight is and how much time passed.
//
// THE RULE: the wanted row = the keyboard-highlighted row (a row with a
// preview, else none) ?? the shown row while the pointer is on its preview
// ?? the row under the pointer (with a preview). A highlight shows its row
// at once; a hovered row shows `menuPreviewOpenMs` after the pointer
// entered it (moving to another row restarts the wait); nothing wanted
// hides the shown one `menuPreviewCloseMs` later (coming back in time keeps
// it). The preview is a nested surface (Embed's sandbox): its inner `event`
// actions fire the Menu's `action` {name, context}.

import { LAYOUT_CONSTANTS } from "./layout"

/** How long the pointer rests on a row before its preview shows (ms). */
export const MENU_PREVIEW_OPEN_MS: number = LAYOUT_CONSTANTS.menuPreviewOpenMs!
/** How long a preview stays after the pointer and the highlight left (ms). */
export const MENU_PREVIEW_CLOSE_MS: number = LAYOUT_CONSTANTS.menuPreviewCloseMs!
/** The gap between a row and its preview (px). */
export const MENU_PREVIEW_OFFSET: number = LAYOUT_CONSTANTS.menuPreviewOffset!

/** One menu's preview state. Keys are item PATHS (`"2"`, a submenu's row
 *  `"1.0"`); only rows with a `preview` are ever keys (a row without one
 *  is `null`, like no row). */
export interface MenuPreviewState {
  /** The row under the pointer. */
  hover: string | null
  /** The pointer is on the shown preview. */
  over: boolean
  /** The keyboard-highlighted row. */
  highlight: string | null
  /** The row whose preview shows. */
  shown: string | null
  /** A change waiting for its delay: show `key` (null = hide) at `at`. */
  pending: { key: string | null; at: number } | null
}

export const MENU_PREVIEW_IDLE: MenuPreviewState = { hover: null, over: false, highlight: null, shown: null, pending: null }

export type MenuPreviewInput = { hover: string | null } | { over: boolean } | { highlight: string | null }

/** The row the state wants shown. */
export function menuPreviewWanted(s: MenuPreviewState): string | null {
  if (s.highlight !== null) return s.highlight
  if (s.over && s.shown !== null) return s.shown
  return s.hover
}

function settle(s: MenuPreviewState, now: number): MenuPreviewState {
  const want = menuPreviewWanted(s)
  if (want === s.shown) return s.pending === null ? s : { ...s, pending: null }
  if (want !== null && want === s.highlight) return { ...s, shown: want, pending: null }
  if (s.pending && s.pending.key === want) return s
  return { ...s, pending: { key: want, at: now + (want === null ? MENU_PREVIEW_CLOSE_MS : MENU_PREVIEW_OPEN_MS) } }
}

/** One input at `now` (ms, any monotonic clock). */
export function menuPreviewInput(s: MenuPreviewState, input: MenuPreviewInput, now: number): MenuPreviewState {
  return settle({ ...s, ...input }, now)
}

/** Time passed: a due change applies. */
export function menuPreviewTick(s: MenuPreviewState, now: number): MenuPreviewState {
  if (!s.pending || now < s.pending.at) return s
  const shown = s.pending.key
  return settle({ ...s, shown, over: shown === null ? false : s.over, pending: null }, now)
}

/** The preview's id inside its Menu (`<menu>.preview.<path>`), the
 *  painted Embed id its nested surface keys on (`embedSurfaceId`). */
export function menuPreviewId(menuId: string, key: string): string {
  return `${menuId}.preview.${key}`
}
