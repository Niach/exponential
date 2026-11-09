// Round 1 (a11y.json Select / TimePicker / ChipInput suggestions; FEED-128:
// hoisted from the React renderer's natives/listbox.ts): the LISTBOX
// keyboard model's arithmetic. ArrowUp/ArrowDown move the active option
// (wrapping, skipping disabled), Home/End jump, printable keys type-ahead by
// label (a 700 ms buffer; one repeated letter cycles).

import { LAYOUT_CONSTANTS } from "../layout"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "../overlay"

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
 *  `popupWidth` (px) wins, else a row popup is the layout's
 *  `selectRowPopupWidth` and a field's at least the trigger's width
 *  (`minWidth` = that trigger-width variable). A `row` popup hangs off its
 *  VALUE (`anchorValue`) by `selectRowPopupOffset`, fitted into the window
 *  `selectRowPopupMargin` from its edges; a field's off the trigger by the
 *  overlay defaults. */
export function selectPopupGeometry(props: Record<string, unknown>, triggerWidthVar: string): { align: `start` | `center` | `end`; minWidth: string | undefined; width: string | undefined; anchorValue: boolean; sideOffset: number; collisionPadding: number } {
  const row = props.triggerVariant === `row`
  const align = props.align === `start` || props.align === `center` || props.align === `end` ? props.align : `start`
  const set = typeof props.popupWidth === `number` && Number.isFinite(props.popupWidth) && props.popupWidth > 0 ? props.popupWidth : undefined
  const width = set ?? (row ? LAYOUT_CONSTANTS.selectRowPopupWidth : undefined)
  return {
    align,
    minWidth: width === undefined ? `var(${triggerWidthVar})` : undefined,
    width: width === undefined ? undefined : `${width}px`,
    anchorValue: row,
    sideOffset: row ? LAYOUT_CONSTANTS.selectRowPopupOffset! : OVERLAY_OFFSET,
    collisionPadding: row ? LAYOUT_CONSTANTS.selectRowPopupMargin! : OVERLAY_PADDING,
  }
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
