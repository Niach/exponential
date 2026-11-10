// Round 1 (a11y.json Select / TimePicker / ChipInput suggestions; FEED-128:
// hoisted from the React renderer's natives/listbox.ts): the LISTBOX
// keyboard model's arithmetic. ArrowUp/ArrowDown move the active option
// (wrapping, skipping disabled), Home/End jump, printable keys type-ahead by
// label (a 700 ms buffer; one repeated letter cycles).

import { LAYOUT_CONSTANTS } from "../layout"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "../overlay"
import type { UiNode } from "../types"

/** The TYPE-AHEAD buffer resets after this pause (ms). */
export const TYPEAHEAD_RESET_MS = 700

/** The next enabled index `delta` steps from `from` (wrapping), or -1. */
export function nextEnabledIndex(count: number, from: number, delta: number, isDisabled: (index: number) => boolean = () => false): number {
  if (count === 0) return -1
  let i = from
  for (let n = 0; n < count; n++) {
    i = (((i + delta) % count) + count) % count
    if (!isDisabled(i)) return i
  }
  return -1
}

export interface TypeaheadBuffer {
  text: string
  at: number
}

/** One printable key typed: the new buffer and the index it lands on (-1 =
 *  no label starts with the buffer; the active option stays). */
export function typeaheadStep(buffer: TypeaheadBuffer, key: string, now: number, active: number, count: number, isDisabled: (index: number) => boolean, labelOf: (index: number) => string): { buffer: TypeaheadBuffer; index: number } {
  const next = { text: now - buffer.at > TYPEAHEAD_RESET_MS ? key.toLowerCase() : buffer.text + key.toLowerCase(), at: now }
  for (let n = 1; n <= count; n++) {
    const i = (Math.max(0, active) + (next.text.length > 1 ? n - 1 : n)) % count
    if (!isDisabled(i) && labelOf(i).toLowerCase().startsWith(next.text)) return { buffer: next, index: i }
  }
  return { buffer: next, index: -1 }
}

/** #EXP-1629: a Select popup's placement. `align` unset = `start`;
 *  `popupWidth` (px) wins, else a row (#EXP-1765: or pill) popup is the
 *  layout's `selectRowPopupWidth` and a field's at least the trigger's width
 *  (`minWidth` = that trigger-width variable). A `row` popup hangs off its
 *  VALUE (`anchorValue`) by `selectRowPopupOffset`, fitted into the window
 *  `selectRowPopupMargin` from its edges; #EXP-1765: a `pill`'s border box
 *  hangs `selectPillPopupOffset` under the pill, starting
 *  `selectPillPopupInset` OUTSIDE its aligned edge (`alignOffset`, the
 *  popper's: negative = outward for `start` and `end`, none centred), the
 *  same margin; a field's off the trigger by the overlay defaults. */
export function selectPopupGeometry(props: Record<string, unknown>, triggerWidthVar: string): { align: `start` | `center` | `end`; minWidth: string | undefined; width: string | undefined; anchorValue: boolean; sideOffset: number; alignOffset: number; collisionPadding: number } {
  const row = props.triggerVariant === `row`
  const pill = props.triggerVariant === `pill`
  const align = props.align === `start` || props.align === `center` || props.align === `end` ? props.align : `start`
  const set = typeof props.popupWidth === `number` && Number.isFinite(props.popupWidth) && props.popupWidth > 0 ? props.popupWidth : undefined
  const width = set ?? (row || pill ? LAYOUT_CONSTANTS.selectRowPopupWidth : undefined)
  return {
    align,
    minWidth: width === undefined ? `var(${triggerWidthVar})` : undefined,
    width: width === undefined ? undefined : `${width}px`,
    anchorValue: row,
    sideOffset: row ? LAYOUT_CONSTANTS.selectRowPopupOffset! : pill ? LAYOUT_CONSTANTS.selectPillPopupOffset! : OVERLAY_OFFSET,
    alignOffset: pill && align !== `center` ? -LAYOUT_CONSTANTS.selectPillPopupInset! : 0,
    collisionPadding: row || pill ? LAYOUT_CONSTANTS.selectRowPopupMargin! : OVERLAY_PADDING,
  }
}

/** #EXP-1765: the Skeleton a `loading` Select's list holds in place of its
 *  options (`<select id>.skeleton`, `selectLoadingRows` rows: the Skeleton
 *  component's own rows and recipes). */
export function selectLoadingNode(selectId: string): UiNode {
  return { id: `${selectId}.skeleton`, component: `Skeleton`, props: { rows: LAYOUT_CONSTANTS.selectLoadingRows ?? 3 }, children: [] }
}

/** #EXP-1765: a pill search's CLEAR button: of the `searchIcon` part's
 *  `width` (16 unset), the glyph 0.75 × (14 → 10.5) centred in a square box
 *  1.25 × (14 → 17.5). */
export function selectClearSize(searchIconWidth: unknown): { glyph: number; box: number } {
  const icon = typeof searchIconWidth === `number` && Number.isFinite(searchIconWidth) && searchIconWidth > 0 ? searchIconWidth : 16
  return { glyph: icon * 0.75, box: icon * 1.25 }
}

/** #EXP-1765: does a pill Select opened this way open WITHOUT a cursor (a
 *  pointer opening; the keyboard keeps the first enabled / chosen one)? */
export function selectOpensWithoutCursor(props: Record<string, unknown>, byPointer: boolean): boolean {
  return byPointer && props.triggerVariant === `pill`
}

/** #EXP-1629: a popper anchored on the TRIGGER placed as if anchored on
 *  `value` inside it (Radix's Select anchors its trigger): the side offset
 *  and the align offset that move it under the value. */
export function valueAnchorOffsets(trigger: Element, value: Element, align: `start` | `center` | `end`, sideOffset: number, rtl: boolean): { sideOffset: number; alignOffset: number } {
  const t = trigger.getBoundingClientRect()
  const v = value.getBoundingClientRect()
  const start = rtl ? t.right - v.right : v.left - t.left
  const end = rtl ? v.left - t.left : t.right - v.right
  return { sideOffset: sideOffset - (t.bottom - v.bottom), alignOffset: align === `start` ? start : align === `end` ? end : (v.left + v.width / 2 - (t.left + t.width / 2)) * (rtl ? -1 : 1) }
}

/** #EXP-1629: the first enabled option index, or -1 (where every Select
 *  opening puts its cursor: a fresh pick). */
export function firstEnabledIndex(count: number, isDisabled: (index: number) => boolean = () => false): number {
  for (let i = 0; i < count; i++) if (!isDisabled(i)) return i
  return -1
}

/** #EXP-1629: where a Select popup opens its cursor: the first enabled
 *  option, or with `isChosen` (a `multiple` Select, the IDE's multi
 *  picker) the first enabled CHOSEN one, -1 when none is (no cursor until
 *  the arrows reach for the list). */
export function openingCursorIndex(count: number, isDisabled: (index: number) => boolean = () => false, isChosen?: (index: number) => boolean): number {
  if (!isChosen) return firstEnabledIndex(count, isDisabled)
  for (let i = 0; i < count; i++) if (!isDisabled(i) && isChosen(i)) return i
  return -1
}

/** #EXP-1629: a `multiple` Select's run of adjacent picked SHOWN rows is
 *  ONE block (web's Picker, the IDE's `multi_row_edges`): the item recipe
 *  props of shown row `index`, `joinTop` under a picked row and
 *  `joinBottom` over one (only the true ones; nothing on a single Select). */
export function selectItemJoins(multiple: boolean, count: number, isPicked: (index: number) => boolean, index: number): { joinTop?: true; joinBottom?: true } {
  if (!multiple || !isPicked(index)) return {}
  return { ...(index > 0 && isPicked(index - 1) ? { joinTop: true as const } : {}), ...(index + 1 < count && isPicked(index + 1) ? { joinBottom: true as const } : {}) }
}

/** #EXP-1629: does `list` have rows below its visible end (the bottom
 *  `fade` shows; SearchList's rule)? */
export function listMoreBelow(list: HTMLElement | null | undefined): boolean {
  return Boolean(list && list.scrollTop + list.clientHeight < list.scrollHeight - 1)
}

/** #EXP-1629: scroll `list` (only it, never the page) so `option` is in
 *  view, the `block: nearest` rule. */
export function revealOption(list: HTMLElement | null | undefined, option: Element | null | undefined): void {
  if (!list || !option) return
  const box = list.getBoundingClientRect()
  const at = option.getBoundingClientRect()
  if (at.top < box.top) list.scrollTop -= box.top - at.top
  else if (at.bottom > box.bottom) list.scrollTop += Math.min(at.bottom - box.bottom, at.top - box.top)
}
