import * as React from "react"
import { createPortal } from "react-dom"

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
//  * A key that lands mid IME composition (`isComposing`) is the IME's, not
//    ours: Enter there commits the candidate, never a row.
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
  /** `KeyboardEvent.isComposing` (a React host passes `nativeEvent`'s). */
  isComposing?: boolean
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
    if (event.isComposing) {
      return false
    }
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

/** The row recipe every autocomplete list has drawn since EXP-551 — on the
 *  menu row geometry since EXP-1074, like every other menu-like row. */
export const TYPEAHEAD_ROW_CLASS = `flex min-h-(--menu-item-height) w-full items-center gap-(--menu-item-gap) px-(--menu-item-padding-x) py-1 text-left text-sm`

const PLACEMENT_CLASS = {
  above: `bottom-full mb-1`,
  below: `top-full mt-1`,
} as const

export type TypeaheadPlacement = keyof typeof PLACEMENT_CLASS

// ── The anchored arm (EXP-959) ──
//
// A textarea hangs the menu under (or over) itself with `position: absolute`,
// but a rich-text editor cannot: its caret sits anywhere in a document that
// may itself live inside a dialog's `overflow-y-auto` scroll region, which
// would clip the popup and inflate `scrollHeight` (EXP-54). So the editor's
// menu takes the CARET rect in viewport coordinates, portals to
// `document.body` at `position: fixed`, and flips above the caret when the
// room below runs out. The measuring is pure (`placeTypeaheadMenu`) so a
// test can drive it with a viewport of its own.

/** Where the trigger character sits, in viewport (fixed) coordinates: what
 *  ProseMirror's `coordsAtPos` or a `getBoundingClientRect` returns. */
export interface TypeaheadAnchor {
  top: number
  bottom: number
  left: number
}

/** The part of the window the menu may use. Read from `visualViewport` when
 *  there is one: with the mobile keyboard open it is shorter than
 *  `innerHeight`, and a menu sized against the layout viewport would open
 *  underneath the keyboard (EXP-198). */
export interface TypeaheadViewport {
  /** `visualViewport.offsetTop`. */
  top: number
  /** `visualViewport.height`. */
  height: number
  width: number
  /** `window.innerHeight` — what a `bottom:` offset is measured against. */
  innerHeight: number
}

export function readTypeaheadViewport(): TypeaheadViewport {
  const vv = window.visualViewport
  return {
    top: vv?.offsetTop ?? 0,
    height: vv?.height ?? window.innerHeight,
    width: window.innerWidth,
    innerHeight: window.innerHeight,
  }
}

/** `w-72`. */
export const TYPEAHEAD_MENU_WIDTH = 288
const VIEWPORT_PAD = 8
const ANCHOR_GAP = 4
/** Below this much room under the caret the menu flips above (when above has
 *  more). */
const FLIP_BELOW = 200
const MIN_HEIGHT = 48
const MAX_HEIGHT = 320

export interface TypeaheadAnchoredStyle {
  left: number
  top?: number
  bottom?: number
  maxHeight: number
}

export interface TypeaheadAnchoredPlacement {
  placement: TypeaheadPlacement
  style: TypeaheadAnchoredStyle
}

/** Fixed placement for a menu hanging off `anchor`: clamped to the viewport
 *  horizontally, below the caret unless the room there is short and the room
 *  above is larger, and capped to the room on the chosen side. */
export function placeTypeaheadMenu(
  anchor: TypeaheadAnchor,
  viewport: TypeaheadViewport = readTypeaheadViewport()
): TypeaheadAnchoredPlacement {
  const visibleBottom = viewport.top + viewport.height
  const left = Math.max(
    VIEWPORT_PAD,
    Math.min(anchor.left, viewport.width - TYPEAHEAD_MENU_WIDTH - VIEWPORT_PAD)
  )
  const spaceBelow = visibleBottom - anchor.bottom - VIEWPORT_PAD
  const spaceAbove = anchor.top - viewport.top - VIEWPORT_PAD
  const cap = (space: number) =>
    Math.max(MIN_HEIGHT, Math.min(space - ANCHOR_GAP, MAX_HEIGHT))
  if (spaceBelow < FLIP_BELOW && spaceAbove > spaceBelow) {
    return {
      placement: `above`,
      style: {
        left,
        bottom: viewport.innerHeight - anchor.top + ANCHOR_GAP,
        maxHeight: cap(spaceAbove),
      },
    }
  }
  return {
    placement: `below`,
    style: { left, top: anchor.bottom + ANCHOR_GAP, maxHeight: cap(spaceBelow) },
  }
}

/** The attribute the anchored arm stamps on its portal. It lives outside the
 *  host's subtree, so a Radix modal would take a click on it as an OUTSIDE
 *  interaction and close: dialog hosts whitelist this selector in their
 *  `onInteractOutside`, and treat a mounted one as "a menu is open" when
 *  routing Escape. */
export const TYPEAHEAD_PORTAL_SELECTOR = `[data-editor-autocomplete]`

/** Whether a Radix outside-interaction (its `target` is the pointed-at
 *  node) landed inside the anchored arm's portal. `DialogContent` and
 *  `SheetContent` ignore that one by default. */
export function isTypeaheadPortalInteraction(event: {
  target: EventTarget | null
}): boolean {
  return (
    event.target instanceof Element &&
    event.target.closest(TYPEAHEAD_PORTAL_SELECTOR) !== null
  )
}

export function TypeaheadMenu({
  placement = `below`,
  maxHeight,
  anchor,
  onAnchorLost,
  className,
  style,
  ref,
  ...props
}: React.ComponentProps<`div`> & {
  placement?: TypeaheadPlacement
  /** The measured room on the chosen side; the host does the measuring. */
  maxHeight?: number
  /** The anchored arm: the caret rect in viewport coordinates. The menu then
   *  portals to `document.body` at fixed coordinates, choosing `placement`
   *  and `maxHeight` itself (`placeTypeaheadMenu`); the props of the same
   *  name are ignored, except an explicit `maxHeight`, which wins. */
  anchor?: TypeaheadAnchor
  /** A fixed menu detaches from its caret the moment any ancestor scroll
   *  region moves (dialog body, page, sheet) or the window resizes. Rather
   *  than chase the caret, the arm reports it and the host closes the menu.
   *  Scrolling inside the menu itself is not a loss. */
  onAnchorLost?: () => void
}) {
  const ownRef = React.useRef<HTMLDivElement | null>(null)
  const setRef = (node: HTMLDivElement | null) => {
    ownRef.current = node
    if (typeof ref === `function`) {
      ref(node)
    } else if (ref) {
      ref.current = node
    }
  }

  const anchored = anchor !== undefined
  React.useEffect(() => {
    if (!anchored || !onAnchorLost) return
    const lost = (event: Event) => {
      if (
        event.target instanceof Node &&
        ownRef.current?.contains(event.target)
      ) {
        return
      }
      onAnchorLost()
    }
    window.addEventListener(`scroll`, lost, true)
    window.addEventListener(`resize`, lost)
    return () => {
      window.removeEventListener(`scroll`, lost, true)
      window.removeEventListener(`resize`, lost)
    }
  }, [anchored, onAnchorLost])

  if (anchor !== undefined) {
    const placed = placeTypeaheadMenu(anchor)
    return createPortal(
      <div
        ref={setRef}
        data-slot="typeahead-menu"
        data-placement={placed.placement}
        data-editor-autocomplete=""
        role="listbox"
        className={cn(
          MENU_SURFACE_CLASS,
          // `pointer-events-auto`: a Radix modal dialog sets
          // `pointer-events: none` on <body> while open, and this portal is
          // outside the DialogContent subtree, so it re-enables them itself
          // or every click falls through to the dialog beneath (EXP-54).
          // `z-[60]`: above that dialog (z-50), outranking the surface
          // recipe's own z-50.
          `pointer-events-auto fixed z-[60] w-72 overflow-x-hidden overflow-y-auto`,
          className
        )}
        style={{
          ...placed.style,
          ...(maxHeight === undefined ? null : { maxHeight }),
          ...style,
        }}
        {...props}
      />,
      document.body
    )
  }

  return (
    <div
      ref={setRef}
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
