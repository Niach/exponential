// FEED-131: Radix `Menu`'s content machinery: `MenuContent` (Radix
// `MenuContentImpl`: a dismissable layer + a focus scope + the popper, roving
// focus over the items, typeahead, first/last keys, the pointer grace toward
// a submenu; the ROOT content is modal), `createMenuRow` (`MenuItemImpl` +
// `RovingFocusGroupItem`) and `createSelectable` (`MenuItem`), the keyboard
// tracking (`isUsingKeyboardRef`). The rows' own DOM (parts, labels, icons)
// is the painter's.

import { createContext, createSignal, onCleanup, onMount, untrack, useContext, type Accessor, type JSX } from "solid-js"
import type { VirtualElement } from "@floating-ui/dom"
import { OVERLAY_PADDING } from "@exponential-at/ui"
import { createLayer, LayerContext, type OutsideEvent } from "./dismissable-layer"
import { createFocusScope, focusFirst } from "./focus-scope"
import { useFocusGuards } from "./focus-guards"
import { useModalEffects } from "./modal"
import { inPolygon, type Point } from "./grace-area"
import { Popper, popperContentStyle, type Align, type Side } from "./popper"
import { bindStyle } from "./style-writer"
import { getNextMatch } from "./typeahead"

/** Whether the person drives the menus with the keyboard (Radix's
 *  `isUsingKeyboardRef`: a keydown says yes, a pointer says no). */
let usingKeyboard = false
let keyboardListeners = 0
export function useKeyboardTracking() {
  const key = () => (usingKeyboard = true)
  const pointer = () => (usingKeyboard = false)
  if (keyboardListeners++ === 0) {
    document.addEventListener(`keydown`, key, { capture: true })
    document.addEventListener(`pointerdown`, pointer, { capture: true })
    document.addEventListener(`pointermove`, pointer, { capture: true })
  }
  onCleanup(() => {
    if (--keyboardListeners === 0) {
      document.removeEventListener(`keydown`, key, { capture: true })
      document.removeEventListener(`pointerdown`, pointer, { capture: true })
      document.removeEventListener(`pointermove`, pointer, { capture: true })
    }
  })
}

/** One menu level's content (Radix `MenuContentImpl`): roving focus over its
 *  items, typeahead, first/last keys, the pointer grace toward a submenu. */
export interface MenuLevel {
  content: () => HTMLElement | undefined
  highlighted: Accessor<HTMLElement | undefined>
  setHighlighted: (el: HTMLElement | undefined) => void
  tabStop: Accessor<HTMLElement | undefined>
  setTabStop: (el: HTMLElement | undefined) => void
  search: () => string
  onItemEnter: (e: PointerEvent) => boolean
  onItemLeave: (e: PointerEvent) => void
  onTriggerLeave: (e: PointerEvent) => boolean
  setGrace: (grace: { area: Point[]; side: Side } | null) => void
}
export const MenuLevelContext = createContext<MenuLevel>()

export const FIRST_KEYS = [`ArrowDown`, `PageUp`, `Home`]
export const LAST_KEYS = [`ArrowUp`, `PageDown`, `End`]
export const itemsOf = (content: HTMLElement | undefined) => (content ? Array.from(content.querySelectorAll<HTMLElement>(`:scope [data-radix-collection-item]`)).filter((el) => el.closest(`[data-radix-menu-content]`) === content) : [])

export type MenuContentProps = {
  anchor: Element | VirtualElement | null | undefined
  side: Side
  align: Align
  sideOffset: number
  alignOffset?: number
  /** The popper's collision padding (default `OVERLAY_PADDING`, the natives';
   *  the primitive set passes Radix's 0). */
  collisionPadding?: number
  /** A layer registered in this same mount pass is not visible to this
   *  content's FIRST style (Radix's default-body portals: the outer layer
   *  registers one commit after the nested content first renders), so its
   *  `pointer-events` starts unset. The primitive set passes it. */
  lagParentLayer?: boolean
  /** Root content: modal (trap, outside pointer events off, scroll lock,
   *  aria-hidden), focus to the content on mount. */
  root: boolean
  id?: string
  labelledBy?: string
  attrs: () => Record<string, unknown>
  state: () => string
  onDismiss: () => void
  onEscapeKeyDown?: (e: KeyboardEvent) => void
  onFocusOutside?: (e: OutsideEvent) => void
  onInteractOutside?: (e: OutsideEvent) => void
  onCloseAutoFocus: (e: Event) => void
  /** The reading direction (the popper wrapper's and the content's `dir`). */
  dir: `ltr` | `rtl`
  /** The `--radix-<kind>-content-*` prefix: `dropdown-menu` or `context-menu`. */
  kind: string
  onKeyDown?: (e: KeyboardEvent) => void
  setContent?: (el: HTMLElement | undefined) => void
  children: JSX.Element
}

export function MenuContent(props: MenuContentProps): JSX.Element {
  const layer = createLayer({
    disableOutsidePointerEvents: () => props.root,
    onEscapeKeyDown: (e) => props.onEscapeKeyDown?.(e),
    onFocusOutside: (e) => {
      if (props.root) e.preventDefault()
      props.onFocusOutside?.(e)
    },
    onInteractOutside: (e) => props.onInteractOutside?.(e),
    onDismiss: () => props.onDismiss(),
  })
  const scope = createFocusScope({
    trapped: () => props.root,
    loop: false,
    onMountAutoFocus: (e) => {
      e.preventDefault()
      if (props.root || usingKeyboard) contentEl?.focus({ preventScroll: true })
    },
    onUnmountAutoFocus: (e) => props.onCloseAutoFocus(e),
  })
  const [registered, setRegistered] = createSignal(false)
  const [highlighted, setHighlighted] = createSignal<HTMLElement>()
  const [tabStop, setTabStop] = createSignal<HTMLElement>()
  let contentEl: HTMLElement | undefined
  let search = ``
  let searchTimer: ReturnType<typeof setTimeout> | undefined
  let grace: { area: Point[]; side: Side } | null = null
  let pointerDir: Side = `right`
  let lastPointerX = 0
  const movingToSubmenu = (e: PointerEvent) => Boolean(grace && pointerDir === grace.side && inPolygon({ x: e.clientX, y: e.clientY }, grace.area))
  const level: MenuLevel = {
    content: () => contentEl,
    highlighted,
    setHighlighted,
    tabStop,
    setTabStop,
    search: () => search,
    onItemEnter: (e) => movingToSubmenu(e),
    onItemLeave: (e) => {
      if (movingToSubmenu(e)) return
      contentEl?.focus({ preventScroll: true })
      setTabStop(undefined)
    },
    onTriggerLeave: (e) => movingToSubmenu(e),
    setGrace: (g) => (grace = g),
  }
  onCleanup(() => clearTimeout(searchTimer))
  const typeahead = (key: string) => {
    const next = search + key
    const items = itemsOf(contentEl).filter((el) => !el.hasAttribute(`data-disabled`))
    const current = document.activeElement
    const currentMatch = items.find((el) => el === current)?.textContent ?? undefined
    const values = items.map((el) => el.textContent ?? ``)
    const match = getNextMatch(values, next, currentMatch)
    const target = items.find((el) => (el.textContent ?? ``) === match)
    const update = (value: string) => {
      search = value
      clearTimeout(searchTimer)
      if (value !== ``) searchTimer = setTimeout(() => update(``), 1000)
    }
    update(next)
    if (target) setTimeout(() => target.focus())
  }
  return (
    <Popper anchor={props.anchor} side={props.side} align={props.align} sideOffset={props.sideOffset} alignOffset={props.alignOffset} collisionPadding={props.collisionPadding ?? OVERLAY_PADDING} dir={props.dir}>
      {(popper) => {
        const el = (
          <div
            data-side={popper.placedSide()}
            data-align={popper.placedAlign()}
            role="menu"
            aria-orientation="vertical"
            data-state={props.state()}
            data-radix-menu-content=""
            dir={props.dir}
            id={props.id}
            aria-labelledby={props.labelledBy}
            {...props.attrs()}
            tabindex="-1"
            data-orientation="vertical"
            onKeyDown={(e: KeyboardEvent) => {
              scope.onKeyDown(e)
              const target = e.target as HTMLElement
              const inside = target.closest(`[data-radix-menu-content]`) === el
              const modifier = e.ctrlKey || e.altKey || e.metaKey
              if (inside) {
                if (e.key === `Tab`) e.preventDefault()
                if (!modifier && e.key.length === 1) typeahead(e.key)
              }
              props.onKeyDown?.(e)
              if (e.target !== el) return
              if (!FIRST_KEYS.includes(e.key) && !LAST_KEYS.includes(e.key)) return
              e.preventDefault()
              const nodes = itemsOf(el).filter((n) => !n.hasAttribute(`data-disabled`))
              if (LAST_KEYS.includes(e.key)) nodes.reverse()
              focusFirst(nodes)
            }}
            onFocus={(e: FocusEvent) => {
              // Radix RovingFocusGroup entry focus: the keyboard lands on the first item.
              if (e.target !== el || !usingKeyboard) return
              const nodes = itemsOf(el).filter((n) => !n.hasAttribute(`data-disabled`))
              const current = untrack(tabStop)
              focusFirst([current, ...nodes].filter(Boolean))
            }}
            onPointerMove={(e: PointerEvent) => {
              if (e.pointerType !== `mouse`) return
              if (el.contains(e.target as Node) && lastPointerX !== e.clientX) {
                pointerDir = e.clientX > lastPointerX ? `right` : `left`
                lastPointerX = e.clientX
              }
            }}
          >
            <LayerContext.Provider value={layer.layer}>
              <MenuLevelContext.Provider value={level}>{props.children}</MenuLevelContext.Provider>
            </LayerContext.Provider>
          </div>
        ) as HTMLDivElement
        contentEl = el
        props.setContent?.(el)
        onCleanup(() => props.setContent?.(undefined))
        popper.setContent(el)
        bindStyle(el, () => popperContentStyle(props.kind, `trigger`, props.lagParentLayer && !registered() ? undefined : layer.pointerEvents(registered()), popper.isPositioned(), { outline: `none` }))
        onMount(() => {
          layer.attach(el)
          setRegistered(true)
          scope.attach(el)
          useFocusGuards()
          if (props.root) useModalEffects(el, true)
        })
        return el
      }}
    </Popper>
  )
}

/** The attributes + behaviour every menu row shares (Radix `MenuItemImpl` +
 *  `RovingFocusGroupItem`): roving tabindex, `data-highlighted` while
 *  focused, pointer highlight, arrow keys. */
export function createMenuRow(disabled: Accessor<boolean>) {
  const level = useContext(MenuLevelContext)!
  let el: HTMLElement | undefined
  const attrs = (): Record<string, unknown> => ({
    tabindex: level.tabStop() === el && el !== undefined ? `0` : `-1`,
    "data-orientation": `vertical`,
    "data-radix-collection-item": ``,
    "data-highlighted": level.highlighted() === el && el !== undefined ? `` : undefined,
    "aria-disabled": disabled() ? `true` : undefined,
    "data-disabled": disabled() ? `` : undefined,
  })
  const handlers = {
    onFocus: () => {
      level.setHighlighted(el)
      if (!untrack(disabled)) level.setTabStop(el)
    },
    onBlur: () => {
      if (untrack(level.highlighted) === el) level.setHighlighted(undefined)
    },
    onPointerMove: (e: PointerEvent) => {
      if (e.pointerType !== `mouse`) return
      if (untrack(disabled)) level.onItemLeave(e)
      else if (!level.onItemEnter(e)) el?.focus({ preventScroll: true })
    },
    onPointerLeave: (e: PointerEvent) => {
      if (e.pointerType !== `mouse`) return
      level.onItemLeave(e)
    },
    onKeyDown: (e: KeyboardEvent) => {
      if (e.target !== e.currentTarget) return
      const intent = e.key === `ArrowUp` ? `prev` : e.key === `ArrowDown` ? `next` : e.key === `Home` || e.key === `PageUp` ? `first` : e.key === `End` || e.key === `PageDown` ? `last` : undefined
      if (!intent || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
      e.preventDefault()
      let candidates = itemsOf(level.content()).filter((n) => !n.hasAttribute(`data-disabled`))
      if (intent === `last`) candidates.reverse()
      else if (intent === `prev` || intent === `next`) {
        if (intent === `prev`) candidates.reverse()
        const at = candidates.indexOf(e.currentTarget as HTMLElement)
        candidates = candidates.slice(at + 1)
      }
      focusFirst(candidates)
    },
  }
  return { attrs, handlers, ref: (node: HTMLElement) => (el = node), level }
}

/** A selectable row (Radix `MenuItem`): click / Enter / Space select it, a
 *  press that started elsewhere selects on release; `onSelect` may prevent
 *  the `close` (the root menu's). */
export function createSelectable(disabled: Accessor<boolean>, onSelect: (e: Event) => void, close: () => void) {
  const row = createMenuRow(disabled)
  let pointerDownHere = false
  const select = () => {
    if (untrack(disabled)) return
    const e = new CustomEvent(`menu.itemSelect`, { bubbles: true, cancelable: true })
    onSelect(e)
    if (e.defaultPrevented) pointerDownHere = false
    else close()
  }
  const handlers = {
    ...row.handlers,
    onClick: select,
    onPointerDown: () => (pointerDownHere = true),
    onPointerUp: (e: PointerEvent) => {
      if (!pointerDownHere) (e.currentTarget as HTMLElement)?.click()
    },
    onKeyDown: (e: KeyboardEvent) => {
      row.handlers.onKeyDown(e)
      if (e.defaultPrevented) return
      const typingAhead = row.level.search() !== ``
      if (untrack(disabled) || (typingAhead && e.key === ` `)) return
      if (e.key === `Enter` || e.key === ` `) {
        ;(e.currentTarget as HTMLElement).click()
        e.preventDefault()
      }
    },
  }
  return { ...row, handlers }
}
