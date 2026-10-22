// FEED-131: Radix `RovingFocusGroup`: the group's own tab stop (`tabindex=0`
// until an item holds the stop, entry focus to the active item), the items'
// roving `tabindex`, the arrow/Home/End intents by orientation and `dir`,
// `focusFirst` in a timeout. Tabs, Segmented (Radix ToggleGroup) and the
// Radio group's keys use it.

import { createSignal } from "solid-js"
import { wrapArray } from "./typeahead"

export type Intent = `first` | `last` | `prev` | `next`
export const KEY_INTENT: Record<string, Intent> = { ArrowLeft: `prev`, ArrowUp: `prev`, ArrowRight: `next`, ArrowDown: `next`, PageUp: `first`, Home: `first`, PageDown: `last`, End: `last` }

export function focusIntent(e: KeyboardEvent, orientation: `horizontal` | `vertical` | undefined, dir: `ltr` | `rtl`): Intent | undefined {
  let key = e.key
  if (dir === `rtl`) key = key === `ArrowLeft` ? `ArrowRight` : key === `ArrowRight` ? `ArrowLeft` : key
  if (orientation === `vertical` && (key === `ArrowLeft` || key === `ArrowRight`)) return undefined
  if (orientation === `horizontal` && (key === `ArrowUp` || key === `ArrowDown`)) return undefined
  return KEY_INTENT[key]
}

/** Radix RovingFocusGroup's `focusFirst`: the first candidate that takes
 *  focus (a candidate already focused stops the walk). */
export function rovingFocusFirst(candidates: (HTMLElement | null | undefined)[]): void {
  const previous = document.activeElement
  for (const candidate of candidates) {
    if (!candidate) continue
    if (candidate === previous) return
    candidate.focus()
    if (document.activeElement !== previous) return
  }
}

/** Radix's RovingFocusGroup: the group's own attributes + handlers, and
 *  each item's. `items()` = the item elements in order, `focusable(el)`,
 *  `active(el)` = the selected one (the group's entry focus). */
export function createRovingGroup(opts: { orientation?: `horizontal` | `vertical`; dir: () => `ltr` | `rtl`; loop: boolean; items: () => HTMLElement[]; focusable: (el: HTMLElement) => boolean; active: (el: HTMLElement) => boolean }) {
  const [stop, setStop] = createSignal<HTMLElement | null>(null)
  const [tabbingBackOut, setTabbingBackOut] = createSignal(false)
  let clickFocus = false
  const group = (): Record<string, unknown> => ({ tabindex: tabbingBackOut() ? -1 : 0, "data-orientation": opts.orientation })
  // React's onFocus/onBlur BUBBLE (focusin/focusout): a click on an item
  // resets the click flag through the group.
  const groupHandlers = {
    onMouseDown: () => void (clickFocus = true),
    onFocusIn: (e: FocusEvent) => {
      const keyboard = !clickFocus
      if (e.target === e.currentTarget && keyboard && !tabbingBackOut()) {
        const items = opts.items().filter(opts.focusable)
        const activeItem = items.find(opts.active)
        const current = items.find((el) => el === stop())
        rovingFocusFirst([activeItem, current, ...items].filter((el): el is HTMLElement => Boolean(el)))
      }
      clickFocus = false
    },
    onFocusOut: () => setTabbingBackOut(false),
  }
  const item = (el: () => HTMLElement | undefined, focusable: () => boolean) => ({
    tabindex: () => (stop() !== null && stop() === el() ? 0 : -1),
    onMouseDown: (e: MouseEvent) => {
      if (!focusable()) e.preventDefault()
      else setStop(el() ?? null)
    },
    onFocus: () => setStop(el() ?? null),
    onKeyDown: (e: KeyboardEvent) => {
      if (e.key === `Tab` && e.shiftKey) {
        setTabbingBackOut(true)
        return
      }
      if (e.target !== e.currentTarget) return
      const intent = focusIntent(e, opts.orientation, opts.dir())
      if (intent === undefined) return
      if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
      e.preventDefault()
      let candidates = opts.items().filter(opts.focusable)
      if (intent === `last`) candidates.reverse()
      else if (intent === `prev` || intent === `next`) {
        if (intent === `prev`) candidates.reverse()
        const at = candidates.indexOf(e.currentTarget as HTMLElement)
        candidates = opts.loop ? wrapArray(candidates, at + 1) : candidates.slice(at + 1)
      }
      setTimeout(() => rovingFocusFirst(candidates))
    },
  })
  return { group, groupHandlers, item }
}
