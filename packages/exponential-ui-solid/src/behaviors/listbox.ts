// FEED-131: the listbox keyboard model (React's natives/listbox.ts
// `useListNavigation`) and Radix Select's trigger keys. `createListNavigation`
// = the Select's and the ChipInput's (the clamp runs as an effect);
// `createListNavigationSync` = the TimePicker's (the clamp runs as a
// computation). The two differ in timing only, kept apart so neither moves.

import { createComputed, createEffect, createSignal, untrack, type Accessor } from "solid-js"
import { nextEnabledIndex, typeaheadStep } from "@exponential_app/ui/web"

export interface ListNavOptions {
  count: Accessor<number>
  isDisabled?: (index: number) => boolean
  labelOf?: (index: number) => string
  onPick: (index: number) => void
  /** Type-ahead (off when a search field owns the keys). */
  typeahead?: Accessor<boolean>
  initial?: number
}

/** React's `useListNavigation`: an active option (aria-activedescendant,
 *  focus stays where it is), ArrowUp/ArrowDown move it (wrapping, skipping
 *  disabled), Home/End jump, Enter picks, printable keys type-ahead by label
 *  when the list owns focus. */
export function createListNavigation(o: ListNavOptions) {
  const isDisabled = o.isDisabled ?? (() => false)
  const labelOf = o.labelOf ?? (() => ``)
  const typeahead = o.typeahead ?? (() => false)
  const [active, setActive] = createSignal(o.initial ?? -1)
  let buffer = { text: ``, at: 0 }
  createEffect(() => {
    const count = o.count()
    if (active() >= count) setActive(count - 1)
  })
  const move = (from: number, delta: number) => nextEnabledIndex(o.count(), from, delta, isDisabled)
  const onKeyDown = (e: KeyboardEvent) => {
    const count = o.count()
    const a = active()
    if (e.key === `ArrowDown`) {
      e.preventDefault()
      setActive(move(a < 0 ? -1 : a, 1))
    } else if (e.key === `ArrowUp`) {
      e.preventDefault()
      setActive(move(a < 0 ? 0 : a, -1))
    } else if (e.key === `Home`) {
      e.preventDefault()
      setActive(move(-1, 1))
    } else if (e.key === `End`) {
      e.preventDefault()
      setActive(move(0, -1))
    } else if (e.key === `Enter` || (e.key === ` ` && typeahead() && buffer.text === ``)) {
      if (a >= 0 && a < count && !isDisabled(a)) {
        e.preventDefault()
        o.onPick(a)
      }
    } else if (typeahead() && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const step = typeaheadStep(buffer, e.key, Date.now(), a, count, isDisabled, labelOf)
      buffer = step.buffer
      if (step.index >= 0) setActive(step.index)
    }
  }
  return { active, setActive, onKeyDown }
}

export interface ListNavSyncOptions {
  count: Accessor<number>
  isDisabled?: (index: number) => boolean
  labelOf?: (index: number) => string
  onPick: (index: number) => void
  /** Type-ahead (off when a search field owns the keys). */
  typeahead?: boolean
  initial?: number
}

/** The TimePicker's variant of `createListNavigation`: `typeahead` is a
 *  constant, the active index is clamped to the count SYNCHRONOUSLY
 *  (`createComputed`, before the list renders) and `setActive` is a plain
 *  setter (no updater function). React's `useListNavigation`: an active
 *  option (aria-activedescendant, focus stays where it is), ArrowUp/ArrowDown
 *  move it (wrapping, skipping disabled), Home/End jump, Enter picks,
 *  printable keys type-ahead. */
export function createListNavigationSync(o: ListNavSyncOptions): { active: Accessor<number>; setActive: (i: number) => void; onKeyDown: (e: KeyboardEvent) => void } {
  const isDisabled = o.isDisabled ?? (() => false)
  const labelOf = o.labelOf ?? (() => ``)
  const [active, setActive] = createSignal(o.initial ?? -1)
  let buffer = { text: ``, at: 0 }
  createComputed(() => {
    const c = o.count()
    if (untrack(active) >= c) setActive(c - 1)
  })
  const move = (from: number, delta: number) => nextEnabledIndex(o.count(), from, delta, isDisabled)
  const onKeyDown = (e: KeyboardEvent) => {
    const count = o.count()
    const a = active()
    if (e.key === `ArrowDown`) {
      e.preventDefault()
      setActive(move(a < 0 ? -1 : a, 1))
    } else if (e.key === `ArrowUp`) {
      e.preventDefault()
      setActive(move(a < 0 ? 0 : a, -1))
    } else if (e.key === `Home`) {
      e.preventDefault()
      setActive(move(-1, 1))
    } else if (e.key === `End`) {
      e.preventDefault()
      setActive(move(0, -1))
    } else if (e.key === `Enter` || (e.key === ` ` && o.typeahead && buffer.text === ``)) {
      if (a >= 0 && a < count && !isDisabled(a)) {
        e.preventDefault()
        o.onPick(a)
      }
    } else if (o.typeahead && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const step = typeaheadStep(buffer, e.key, Date.now(), a, count, isDisabled, labelOf)
      buffer = step.buffer
      if (step.index >= 0) setActive(step.index)
    }
  }
  return { active, setActive: (i) => setActive(i), onKeyDown }
}

/** Radix Select: the keys that open the content from the trigger, and the
 *  keys that pick the focused item. */
export const OPEN_KEYS = [` `, `Enter`, `ArrowUp`, `ArrowDown`]
export const SELECTION_KEYS = [` `, `Enter`]
