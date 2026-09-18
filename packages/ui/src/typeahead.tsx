import * as React from "react"

import { cn } from "./cn"
import { MENU_SURFACE_CLASS } from "./menu-surface"

// EXP-941 — the free-text typeahead: the floating menu that follows what
// someone is TYPING, as opposed to `Combobox`, which owns its own field.
//
// Three of them existed (the `@`/`#`/`:` menu in `mention-textarea`, the `/`
// menu in `steer-command-menu`, the TipTap one in `markdown-editor`), each
// re-implementing the same active index, the same wrap, the same above/below
// flip and the same row button. Only one of the three returned a "handled"
// signal; the other two signalled it by not calling the host's handler, which
// is how a menu ends up swallowing a send shortcut.
//
// The rules, identical on all four clients:
//
//  * ArrowDown / ArrowUp move and WRAP.
//  * A plain Enter or Tab accepts the active item.
//  * Enter with Cmd or Ctrl is NOT ours — it is the composer's send, and the
//    handler leaves it completely alone (no `preventDefault`, returns false).
//  * Escape dismisses.
//  * With no items nothing is handled at all.
//
// `handleKeyDown` returns TRUE when the menu consumed the key: the host must
// then do nothing else. Its event is typed structurally, not as a React
// synthetic one, so ProseMirror's native `KeyboardEvent` fits it too.

export interface TypeaheadKeyEvent {
  key: string
  metaKey?: boolean
  ctrlKey?: boolean
  altKey?: boolean
  shiftKey?: boolean
  preventDefault?: () => void
}

export interface UseTypeaheadOptions<TItem> {
  /** The candidates, already ranked by the host. */
  items: readonly TItem[]
  onAccept: (item: TItem, index: number) => void
  /** Escape. The host decides what "dismissed" means (usually: remember the
   *  draft this was pressed on, so typing re-offers the menu). */
  onDismiss?: () => void
  /** Any value whose change means "a new query": the active row goes back to
   *  the top. Usually the query string itself. */
  resetKey?: unknown
}

export interface Typeahead {
  /** The active row, always a valid index (clamped as the list shrinks). */
  active: number
  setActive: (index: number) => void
  /** TRUE when the menu consumed the key. */
  handleKeyDown: (event: TypeaheadKeyEvent) => boolean
  acceptActive: () => void
}

export function useTypeahead<TItem>({
  items,
  onAccept,
  onDismiss,
  resetKey,
}: UseTypeaheadOptions<TItem>): Typeahead {
  const [active, setActive] = React.useState(0)

  // Reset during render, not in an effect: an effect would let one frame
  // render with the previous query's row highlighted.
  const lastResetKey = React.useRef(resetKey)
  if (lastResetKey.current !== resetKey) {
    lastResetKey.current = resetKey
    if (active !== 0) {
      setActive(0)
    }
  }

  const count = items.length
  const activeIndex = Math.min(active, Math.max(0, count - 1))

  const acceptActive = () => {
    const item = items[activeIndex]
    if (item === undefined) {
      return
    }
    onAccept(item, activeIndex)
  }

  const handleKeyDown = (event: TypeaheadKeyEvent) => {
    if (count === 0) {
      return false
    }
    if (event.key === `ArrowDown`) {
      event.preventDefault?.()
      setActive((current) => (Math.min(current, count - 1) + 1) % count)
      return true
    }
    if (event.key === `ArrowUp`) {
      event.preventDefault?.()
      setActive((current) => (Math.min(current, count - 1) - 1 + count) % count)
      return true
    }
    if (
      (event.key === `Enter` || event.key === `Tab`) &&
      !event.metaKey &&
      !event.ctrlKey
    ) {
      event.preventDefault?.()
      acceptActive()
      return true
    }
    if (event.key === `Escape`) {
      event.preventDefault?.()
      onDismiss?.()
      return true
    }
    return false
  }

  return { active: activeIndex, setActive, handleKeyDown, acceptActive }
}

/** The row recipe every autocomplete list has drawn since EXP-551. */
export const TYPEAHEAD_ROW_CLASS = `flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm`

const PLACEMENT_CLASS = {
  above: `bottom-full mb-1`,
  below: `top-full mt-1`,
} as const

export type TypeaheadPlacement = keyof typeof PLACEMENT_CLASS

export function TypeaheadMenu({
  placement = `below`,
  maxHeight,
  className,
  style,
  ...props
}: React.ComponentProps<`div`> & {
  placement?: TypeaheadPlacement
  /** The measured room on the chosen side; the host does the measuring. */
  maxHeight?: number
}) {
  return (
    <div
      data-slot="typeahead-menu"
      data-placement={placement}
      role="listbox"
      className={cn(
        MENU_SURFACE_CLASS,
        `absolute left-0 z-20 w-72 overflow-x-hidden overflow-y-auto`,
        PLACEMENT_CLASS[placement],
        className
      )}
      style={maxHeight === undefined ? style : { maxHeight, ...style }}
      {...props}
    />
  )
}

export function TypeaheadRow({
  active = false,
  onSelect,
  className,
  children,
  onMouseDown,
  ...props
}: Omit<React.ComponentProps<`button`>, `onSelect`> & {
  active?: boolean
  onSelect?: () => void
}) {
  return (
    <button
      type="button"
      data-slot="typeahead-row"
      role="option"
      {...props}
      aria-selected={active}
      className={cn(TYPEAHEAD_ROW_CLASS, active && `bg-glass-active`, className)}
      // The host's textarea/editor must keep the caret through the click, so
      // the row never takes focus in the first place.
      onMouseDown={(event) => {
        event.preventDefault()
        onMouseDown?.(event)
      }}
      onClick={(event) => {
        props.onClick?.(event)
        onSelect?.()
      }}
    >
      {children}
    </button>
  )
}
