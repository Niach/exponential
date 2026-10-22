// FEED-128 (React's natives/overlays.tsx): the OVERLAYS — Dialog, Drawer,
// Popover, Tooltip, Menu (press | contextmenu) and the Toast. Every portal
// mounts into the surface's OVERLAY LAYER inside the `xui` container (so a
// breakpoint still matches inside a Dialog, audit bug C); toasts mount into
// the TOAST layer above it.
//
// React paints these on Radix; the Solid painter paints Radix's DOM itself
// (Kobalte's parts cannot produce it: their wrappers, attribute names,
// portal containers and positioning differ) on plain elements, with the
// SAME machinery underneath: `@floating-ui/dom` with Radix Popper's exact
// middleware (offset → shift(limitShift) → flip → size → transformOrigin,
// `strategy: fixed`, `OVERLAY_OFFSET`/`OVERLAY_PADDING`, so `placeOverlay`
// and fixtures/overlay-geometry.json hold), `aria-hidden`'s `hideOthers`,
// the dismissable-layer stack (Escape, pointer-down and focus outside, the
// body's pointer-events while a modal layer is up), the focus-scope stack
// (auto focus, trap, loop, return), the focus guards and the scroll lock,
// each writing what Radix writes in the order Radix writes it (the
// web-parity check diffs the `style` attribute strings).

import { For, Show, children as resolveChildren, createContext, createEffect, createMemo, createRenderEffect, createSignal, createUniqueId, on, onCleanup, onMount, untrack, useContext, type Accessor, type Component, type JSX } from "solid-js"
import { Dynamic, insert } from "solid-js/web"
import { autoUpdate, computePosition, flip, limitShift, offset, shift, size, type Middleware, type Placement, type VirtualElement } from "@floating-ui/dom"
import { hideOthers } from "aria-hidden"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential-at/ui"
import { absolutePath, isBinding } from "@exponential-at/ui/web"
import { useSurfaceContext } from "../context"
import { CloseOnSubmitContext } from "../form"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, num, objects, parts, str, type Attrs, type PartFn } from "./shared"
import { SearchBody, searchMatches, type SearchItem } from "./search"

type Side = `top` | `right` | `bottom` | `left`
type Align = `start` | `center` | `end`
type StyleObject = Record<string, string | undefined>

// ---------------------------------------------------------------------------
// The machinery (Radix's primitives, on plain elements)
// ---------------------------------------------------------------------------

/** React's `setValueForStyles(node, next, prev)` (kebab-case keys): a key
 *  gone from `next` is cleared, a changed one is written; an existing
 *  declaration keeps its place in the `style` attribute, a new one appends. */
function styleWriter(el: HTMLElement): (next: StyleObject) => void {
  let prev: StyleObject | null = null
  const write = (k: string, v: string | undefined) => el.style.setProperty(k, v === undefined ? `` : v)
  return (next) => {
    if (prev) {
      for (const k of Object.keys(prev)) if (!(k in next)) write(k, ``)
      for (const k of Object.keys(next)) if (prev[k] !== next[k]) write(k, next[k])
    } else for (const k of Object.keys(next)) write(k, next[k])
    prev = { ...next }
  }
}

/** A portal WITHOUT a wrapper (Solid's `<Portal>` adds a `<div>`, Radix's
 *  appends the content itself): the children are inserted before an empty
 *  text marker appended to `mount`, in mount order. */
export function LayerPortal(props: { mount: HTMLElement | null; children: JSX.Element }): JSX.Element {
  return (
    <Show when={props.mount}>
      {(mount) => {
        const el = mount()
        // The children first: a portal mounting INSIDE this content in the
        // same pass (a Popover open in an opening Dialog) lands BEFORE it,
        // as React places a portal's deeper host nodes first; one opened
        // later appends after it.
        const nodes = resolveChildren(() => props.children)
        const marker = document.createTextNode(``)
        el.appendChild(marker)
        insert(el, nodes, marker)
        onCleanup(() => {
          for (const n of [nodes()].flat(Infinity as 1) as unknown[]) if (n instanceof Node && n.parentNode === el) el.removeChild(n)
          marker.remove()
        })
        return null
      }}
    </Show>
  )
}

/** Radix `Presence`: mounted while `present`, and on close until the exit
 *  animation the closed state starts has ended (none = at once). */
function createPresence(present: Accessor<boolean>): { mounted: Accessor<boolean>; ref: (el: HTMLElement) => void } {
  const [mounted, setMounted] = createSignal(untrack(present))
  let node: HTMLElement | undefined
  let openName = `none`
  let stop: (() => void) | undefined
  const nameOf = (el: HTMLElement) => getComputedStyle(el).animationName || `none`
  createEffect(
    on(present, (p) => {
      stop?.()
      stop = undefined
      if (p) {
        setMounted(true)
        return
      }
      const el = node
      if (!el || !el.isConnected) return setMounted(false)
      const styles = getComputedStyle(el)
      const current = styles.animationName || `none`
      if (current === `none` || styles.display === `none` || current === openName) return setMounted(false)
      const done = (e: AnimationEvent) => {
        if (e.target !== el || !nameOf(el).split(`,`).map((s) => s.trim()).includes(e.animationName)) return
        stop?.()
        setMounted(false)
      }
      el.addEventListener(`animationend`, done)
      el.addEventListener(`animationcancel`, done)
      stop = () => {
        el.removeEventListener(`animationend`, done)
        el.removeEventListener(`animationcancel`, done)
      }
    }, { defer: true })
  )
  onCleanup(() => stop?.())
  return {
    mounted,
    ref: (el) => {
      node = el
      el.addEventListener(`animationstart`, (e) => {
        if (e.target === el && untrack(present)) openName = nameOf(el)
      })
    },
  }
}

// -- the dismissable-layer stack -------------------------------------------

interface Layer {
  el?: HTMLElement
  parent?: Layer
  kids: Set<Layer>
}
const LAYERS: Layer[] = []
const MODAL_LAYERS: Layer[] = []
const [layersVersion, setLayersVersion] = createSignal(0)
let originalBodyPointerEvents = ``
const LayerContext = createContext<Layer | undefined>()

/** Inside this layer or one it opened (Radix checks the React tree, where a
 *  portalled child layer still sits under its parent). */
function layerContains(layer: Layer, target: Node | null): boolean {
  if (!target) return false
  if (layer.el?.contains(target)) return true
  for (const kid of layer.kids) if (layerContains(kid, target)) return true
  return false
}

interface OutsideEvent {
  target: EventTarget | null
  detail: { originalEvent: Event }
  defaultPrevented: boolean
  preventDefault: () => void
}
const outsideEvent = (originalEvent: Event): OutsideEvent => {
  const e: OutsideEvent = { target: originalEvent.target, detail: { originalEvent }, defaultPrevented: false, preventDefault: () => (e.defaultPrevented = true) }
  return e
}

interface LayerOptions {
  disableOutsidePointerEvents?: Accessor<boolean>
  onEscapeKeyDown?: (e: KeyboardEvent) => void
  onPointerDownOutside?: (e: OutsideEvent) => void
  onFocusOutside?: (e: OutsideEvent) => void
  onInteractOutside?: (e: OutsideEvent) => void
  onDismiss: () => void
}

/** Radix `DismissableLayer` on `el` (call `attach(el)` once it is in the
 *  document): Escape (the highest layer only), a pointer down outside, a
 *  focus outside; a modal layer turns the body's pointer events off.
 *  `pointerEvents()` = the layer's own `pointer-events` (Radix's value). */
function createLayer(o: LayerOptions): { layer: Layer; attach: (el: HTMLElement) => void; pointerEvents: (registered: boolean) => string | undefined } {
  const parent = useContext(LayerContext)
  const layer: Layer = { parent, kids: new Set() }
  const modal = () => o.disableOutsidePointerEvents?.() ?? false
  const enabled = () => {
    const highest = MODAL_LAYERS[MODAL_LAYERS.length - 1]
    return LAYERS.indexOf(layer) >= (highest ? LAYERS.indexOf(highest) : -1)
  }
  const pointerEvents = (registered: boolean) => {
    layersVersion()
    if (MODAL_LAYERS.length === 0) return undefined
    const highest = MODAL_LAYERS[MODAL_LAYERS.length - 1]
    const index = registered ? LAYERS.indexOf(layer) : -1
    return index >= LAYERS.indexOf(highest) ? `auto` : `none`
  }
  const attach = (el: HTMLElement) => {
    layer.el = el
    const doc = el.ownerDocument
    const disable = untrack(modal)
    if (disable) {
      if (MODAL_LAYERS.length === 0) {
        originalBodyPointerEvents = doc.body.style.pointerEvents
        doc.body.style.pointerEvents = `none`
      }
      MODAL_LAYERS.push(layer)
    }
    LAYERS.push(layer)
    parent?.kids.add(layer)
    setLayersVersion((v) => v + 1)
    let clickHandler: ((e: Event) => void) | undefined
    const pointerDown = (event: PointerEvent) => {
      if (event.target && !layerContains(layer, event.target as Node)) {
        const run = () => {
          if (!enabled()) return
          const e = outsideEvent(event)
          o.onPointerDownOutside?.(e)
          o.onInteractOutside?.(e)
          if (!e.defaultPrevented) o.onDismiss()
        }
        if (event.pointerType === `touch`) {
          if (clickHandler) doc.removeEventListener(`click`, clickHandler)
          clickHandler = run
          doc.addEventListener(`click`, clickHandler, { once: true })
        } else run()
      } else if (clickHandler) doc.removeEventListener(`click`, clickHandler)
    }
    const focusIn = (event: FocusEvent) => {
      if (!event.target || layerContains(layer, event.target as Node)) return
      const e = outsideEvent(event)
      o.onFocusOutside?.(e)
      o.onInteractOutside?.(e)
      if (!e.defaultPrevented) o.onDismiss()
    }
    const keyDown = (event: KeyboardEvent) => {
      if (event.key !== `Escape`) return
      if (LAYERS.indexOf(layer) !== LAYERS.length - 1) return
      o.onEscapeKeyDown?.(event)
      if (!event.defaultPrevented) {
        event.preventDefault()
        o.onDismiss()
      }
    }
    const timer = setTimeout(() => doc.addEventListener(`pointerdown`, pointerDown), 0)
    doc.addEventListener(`focusin`, focusIn)
    doc.addEventListener(`keydown`, keyDown, { capture: true })
    onCleanup(() => {
      clearTimeout(timer)
      doc.removeEventListener(`pointerdown`, pointerDown)
      if (clickHandler) doc.removeEventListener(`click`, clickHandler)
      doc.removeEventListener(`focusin`, focusIn)
      doc.removeEventListener(`keydown`, keyDown, { capture: true })
      if (disable && MODAL_LAYERS.length === 1) doc.body.style.pointerEvents = originalBodyPointerEvents
      LAYERS.splice(LAYERS.indexOf(layer), 1)
      const m = MODAL_LAYERS.indexOf(layer)
      if (m >= 0) MODAL_LAYERS.splice(m, 1)
      parent?.kids.delete(layer)
      setLayersVersion((v) => v + 1)
    })
  }
  return { layer, attach, pointerEvents }
}

// -- the focus-scope stack ---------------------------------------------------

interface Scope {
  paused: boolean
}
let SCOPES: Scope[] = []
const scopesAdd = (s: Scope) => {
  const active = SCOPES[0]
  if (active && active !== s) active.paused = true
  SCOPES = [s, ...SCOPES.filter((x) => x !== s)]
}
const scopesRemove = (s: Scope) => {
  SCOPES = SCOPES.filter((x) => x !== s)
  if (SCOPES[0]) SCOPES[0].paused = false
}

function tabbables(container: HTMLElement): HTMLElement[] {
  const nodes: HTMLElement[] = []
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_ELEMENT, {
    acceptNode: (node) => {
      const el = node as HTMLElement & { disabled?: boolean; type?: string }
      const hiddenInput = el.tagName === `INPUT` && el.type === `hidden`
      if (el.disabled || el.hidden || hiddenInput) return NodeFilter.FILTER_SKIP
      return el.tabIndex >= 0 ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP
    },
  })
  while (walker.nextNode()) nodes.push(walker.currentNode as HTMLElement)
  return nodes
}
function isHidden(node: HTMLElement | null, upTo: HTMLElement): boolean {
  if (node && getComputedStyle(node).visibility === `hidden`) return true
  while (node) {
    if (node === upTo) return false
    if (getComputedStyle(node).display === `none`) return true
    node = node.parentElement
  }
  return false
}
function focusEl(el: HTMLElement | null | undefined, select = false) {
  if (!el?.focus) return
  const prev = document.activeElement
  el.focus({ preventScroll: true })
  if (el !== prev && select && el instanceof HTMLInputElement && `select` in el) el.select()
}
function focusFirst(candidates: (HTMLElement | null | undefined)[], select = false) {
  const prev = document.activeElement
  for (const c of candidates) {
    focusEl(c, select)
    if (document.activeElement !== prev) return
  }
}

const AUTOFOCUS_ON_MOUNT = `focusScope.autoFocusOnMount`
const AUTOFOCUS_ON_UNMOUNT = `focusScope.autoFocusOnUnmount`

/** Radix `FocusScope` on `el`: auto focus on mount (the first tabbable, else
 *  the container; `onMountAutoFocus` may prevent it), focus back on unmount
 *  (`onUnmountAutoFocus` may prevent it), `trapped` keeps the focus inside,
 *  `loop` wraps Tab. Returns the container's keydown handler. */
function createFocusScope(o: { trapped: Accessor<boolean>; loop: boolean; onMountAutoFocus?: (e: Event) => void; onUnmountAutoFocus?: (e: Event) => void }): { attach: (el: HTMLElement) => void; onKeyDown: (e: KeyboardEvent) => void } {
  const scope: Scope = { paused: false }
  let container: HTMLElement | undefined
  const attach = (el: HTMLElement) => {
    // Radix's FocusScope effect runs one commit AFTER the dismissable layers
    // of the same mount registered their outside listeners (its container
    // is state): every overlay mounted together listens before any of them
    // takes the focus (a Popover open next to an opening Dialog loses).
    let started = false
    let disposed = false
    let previouslyFocused: HTMLElement | null = null
    let lastFocused: HTMLElement | null = null
    const focusIn = (event: FocusEvent) => {
      if (!untrack(o.trapped) || scope.paused) return
      const target = event.target as HTMLElement | null
      if (el.contains(target)) lastFocused = target
      else focusEl(lastFocused, true)
    }
    const focusOut = (event: FocusEvent) => {
      if (!untrack(o.trapped) || scope.paused) return
      const related = event.relatedTarget as HTMLElement | null
      if (related === null) return
      if (!el.contains(related)) focusEl(lastFocused, true)
    }
    const observer = new MutationObserver((mutations) => {
      if (!untrack(o.trapped) || scope.paused) return
      if (document.activeElement !== document.body) return
      for (const m of mutations) if (m.removedNodes.length > 0) focusEl(el)
    })
    queueMicrotask(() => {
      if (disposed || !el.isConnected) return
      started = true
      container = el
      scopesAdd(scope)
      previouslyFocused = document.activeElement as HTMLElement | null
      if (!el.contains(previouslyFocused)) {
        const mountEvent = new CustomEvent(AUTOFOCUS_ON_MOUNT, { bubbles: false, cancelable: true })
        const onMount = (e: Event) => o.onMountAutoFocus?.(e)
        el.addEventListener(AUTOFOCUS_ON_MOUNT, onMount)
        el.dispatchEvent(mountEvent)
        el.removeEventListener(AUTOFOCUS_ON_MOUNT, onMount)
        if (!mountEvent.defaultPrevented) {
          focusFirst(tabbables(el).filter((n) => n.tagName !== `A`), true)
          if (document.activeElement === previouslyFocused) focusEl(el)
        }
      }
      // The trap (Radix: focusin/focusout on the document + a mutation watch).
      document.addEventListener(`focusin`, focusIn)
      document.addEventListener(`focusout`, focusOut)
      observer.observe(el, { childList: true, subtree: true })
    })
    onCleanup(() => {
      disposed = true
      if (!started) return
      document.removeEventListener(`focusin`, focusIn)
      document.removeEventListener(`focusout`, focusOut)
      observer.disconnect()
      const prev = previouslyFocused
      setTimeout(() => {
        const unmountEvent = new CustomEvent(AUTOFOCUS_ON_UNMOUNT, { bubbles: false, cancelable: true })
        const onUnmount = (e: Event) => o.onUnmountAutoFocus?.(e)
        el.addEventListener(AUTOFOCUS_ON_UNMOUNT, onUnmount)
        el.dispatchEvent(unmountEvent)
        if (!unmountEvent.defaultPrevented) focusEl(prev ?? document.body, true)
        el.removeEventListener(AUTOFOCUS_ON_UNMOUNT, onUnmount)
        scopesRemove(scope)
      }, 0)
    })
  }
  const onKeyDown = (event: KeyboardEvent) => {
    if (!o.loop && !untrack(o.trapped)) return
    if (scope.paused || !container) return
    const isTab = event.key === `Tab` && !event.altKey && !event.ctrlKey && !event.metaKey
    const focused = document.activeElement as HTMLElement | null
    if (!isTab || !focused) return
    const candidates = tabbables(container)
    const first = candidates.find((c) => !isHidden(c, container!))
    const last = [...candidates].reverse().find((c) => !isHidden(c, container!))
    if (!first || !last) {
      if (focused === container) event.preventDefault()
    } else if (!event.shiftKey && focused === last) {
      event.preventDefault()
      if (o.loop) focusEl(first, true)
    } else if (event.shiftKey && focused === first) {
      event.preventDefault()
      if (o.loop) focusEl(last, true)
    }
  }
  return { attach, onKeyDown }
}

// -- focus guards, scroll lock -----------------------------------------------

let guardCount = 0
function createGuard(): HTMLSpanElement {
  const el = document.createElement(`span`)
  el.setAttribute(`data-radix-focus-guard`, ``)
  el.tabIndex = 0
  el.style.outline = `none`
  el.style.opacity = `0`
  el.style.position = `fixed`
  el.style.pointerEvents = `none`
  return el
}
/** Radix `useFocusGuards`: two focusable spans at the body's edges while
 *  any overlay that needs them is up. */
function useFocusGuards() {
  const edges = document.querySelectorAll(`[data-radix-focus-guard]`)
  document.body.insertAdjacentElement(`afterbegin`, edges[0] ?? createGuard())
  document.body.insertAdjacentElement(`beforeend`, edges[1] ?? createGuard())
  guardCount++
  onCleanup(() => {
    if (guardCount === 1) document.querySelectorAll(`[data-radix-focus-guard]`).forEach((n) => n.remove())
    guardCount--
  })
}

const LOCK_ATTRIBUTE = `data-scroll-locked`
let lockSheets = 0
let lockSheet: HTMLStyleElement | null = null
/** react-remove-scroll-bar's body lock (the same sheet, the same counter
 *  attribute) while a modal overlay is up. */
function useScrollLock() {
  const body = document.body
  const count = () => {
    const n = parseInt(body.getAttribute(LOCK_ATTRIBUTE) || `0`, 10)
    return isFinite(n) ? n : 0
  }
  body.setAttribute(LOCK_ATTRIBUTE, String(count() + 1))
  if (lockSheets === 0) {
    const cs = getComputedStyle(body)
    const parse = (x: string) => parseInt(x || ``, 10) || 0
    const [left, top, right] = [parse(cs.marginLeft), parse(cs.marginTop), parse(cs.marginRight)]
    const gap = Math.max(0, window.innerWidth - document.documentElement.clientWidth + right - left)
    const important = `!important`
    const css = `\n  .with-scroll-bars-hidden {\n   overflow: hidden ${important};\n   padding-right: ${gap}px ${important};\n  }\n  body[${LOCK_ATTRIBUTE}] {\n    overflow: hidden ${important};\n    overscroll-behavior: contain;\n    ${[`position: relative ${important};`, `\n    padding-left: ${left}px;\n    padding-top: ${top}px;\n    padding-right: ${right}px;\n    margin-left:0;\n    margin-top:0;\n    margin-right: ${gap}px ${important};\n    `].join(``)}\n  }\n  \n  .right-scroll-bar-position {\n    right: ${gap}px ${important};\n  }\n  \n  .width-before-scroll-bar {\n    margin-right: ${gap}px ${important};\n  }\n  \n  .right-scroll-bar-position .right-scroll-bar-position {\n    right: 0 ${important};\n  }\n  \n  .width-before-scroll-bar .width-before-scroll-bar {\n    margin-right: 0 ${important};\n  }\n  \n  body[${LOCK_ATTRIBUTE}] {\n    --removed-body-scroll-bar-size: ${gap}px;\n  }\n`
    lockSheet = document.createElement(`style`)
    lockSheet.type = `text/css`
    lockSheet.appendChild(document.createTextNode(css))
    ;(document.head || document.getElementsByTagName(`head`)[0]).appendChild(lockSheet)
  }
  lockSheets++
  onCleanup(() => {
    const n = count() - 1
    if (n <= 0) body.removeAttribute(LOCK_ATTRIBUTE)
    else body.setAttribute(LOCK_ATTRIBUTE, String(n))
    lockSheets--
    if (!lockSheets && lockSheet) {
      lockSheet.remove()
      lockSheet = null
    }
  })
}

/** Radix's modal content: the rest of the page `aria-hidden` (live regions
 *  and scripts kept), the focus guards, the scroll lock. */
function useModalEffects(el: HTMLElement, lockScroll: boolean) {
  const undo = hideOthers(el)
  onCleanup(undo)
  if (lockScroll) useScrollLock()
}

// -- the popper ----------------------------------------------------------------

/** Radix Popper's `transformOrigin` middleware (no arrow: always hidden). */
const transformOrigin: Middleware = {
  name: `transformOrigin`,
  fn(data) {
    const [placedSide, placedAlign = `center`] = data.placement.split(`-`) as [Side, Align | undefined]
    const noArrowAlign = { start: `0%`, center: `50%`, end: `100%` }[placedAlign]
    let x = ``
    let y = ``
    if (placedSide === `bottom`) {
      x = noArrowAlign
      y = `${-0}px`
    } else if (placedSide === `top`) {
      x = noArrowAlign
      y = `${data.rects.floating.height + 0}px`
    } else if (placedSide === `right`) {
      x = `${-0}px`
      y = noArrowAlign
    } else if (placedSide === `left`) {
      x = `${data.rects.floating.width + 0}px`
      y = noArrowAlign
    }
    return { data: { x, y } }
  },
}

interface PopperState {
  placedSide: Accessor<Side>
  placedAlign: Accessor<Align>
  isPositioned: Accessor<boolean>
  /** The content element (its computed `z-index` goes on the wrapper). */
  setContent: (el: HTMLElement) => void
}

/** Radix `PopperContent`: the `data-radix-popper-content-wrapper` (fixed,
 *  translated, `min-width: max-content`, the content's z-index, the
 *  `--radix-popper-*` variables) around the content `children(state)`.
 *  Placement = floating-ui with Radix's middleware, auto-updated. */
function Popper(props: { anchor: Element | VirtualElement | null | undefined; side: Side; align: Align; sideOffset: number; alignOffset?: number; collisionPadding: number; dir?: string; children: (state: PopperState) => JSX.Element }): JSX.Element {
  const desired = () => (props.align === `center` ? props.side : `${props.side}-${props.align}`) as Placement
  const [placement, setPlacement] = createSignal<Placement>(untrack(desired))
  const [isPositioned, setPositioned] = createSignal(false)
  const [content, setContent] = createSignal<HTMLElement>()
  let pos = { x: 0, y: 0, origin: undefined as { x: string; y: string } | undefined }
  let floatingSet = false
  let zIndex: string | undefined
  let disposed = false
  const wrapper = (<div data-radix-popper-content-wrapper="" dir={props.dir as `ltr` | `rtl` | undefined} />) as HTMLDivElement
  const writeWrapper = styleWriter(wrapper)
  const dpr = () => wrapper.ownerDocument.defaultView?.devicePixelRatio || 1
  const roundByDPR = (v: number) => Math.round(v * dpr()) / dpr()
  const paintWrapper = () => {
    const s: StyleObject = { position: `fixed`, left: `0`, top: `0` }
    // floating-ui's floatingStyles: the transform (+ will-change at DPR ≥ 1.5) once the floating element is known.
    if (floatingSet) {
      s.transform = `translate(${roundByDPR(pos.x)}px, ${roundByDPR(pos.y)}px)`
      if (dpr() >= 1.5) s[`will-change`] = `transform`
    }
    s.transform = untrack(isPositioned) ? s.transform : `translate(0, -200%)`
    s[`min-width`] = `max-content`
    s[`z-index`] = zIndex || undefined
    s[`--radix-popper-transform-origin`] = [pos.origin?.x, pos.origin?.y].join(` `)
    writeWrapper(s)
  }
  paintWrapper()
  const state: PopperState = { placedSide: () => placement().split(`-`)[0] as Side, placedAlign: () => (placement().split(`-`)[1] ?? `center`) as Align, isPositioned, setContent }
  insert(wrapper, untrack(() => props.children(state)))
  createEffect(() => {
    const anchor = props.anchor
    const el = content()
    if (!anchor || !el) return
    untrack(() => {
      floatingSet = true
      paintWrapper()
      zIndex = getComputedStyle(el).zIndex
      paintWrapper()
    })
    const padding = props.collisionPadding
    const detect = { padding, boundary: [] as Element[], altBoundary: false }
    const middleware = [
      offset({ mainAxis: props.sideOffset, alignmentAxis: props.alignOffset ?? 0 }),
      shift({ mainAxis: true, crossAxis: false, limiter: limitShift(), ...detect }),
      flip({ ...detect }),
      size({
        ...detect,
        apply: ({ elements, rects, availableWidth, availableHeight }) => {
          const st = elements.floating.style
          st.setProperty(`--radix-popper-available-width`, `${availableWidth}px`)
          st.setProperty(`--radix-popper-available-height`, `${availableHeight}px`)
          st.setProperty(`--radix-popper-anchor-width`, `${rects.reference.width}px`)
          st.setProperty(`--radix-popper-anchor-height`, `${rects.reference.height}px`)
        },
      }),
      transformOrigin,
    ]
    const placementNow = desired()
    const update = () => {
      void computePosition(anchor, wrapper, { strategy: `fixed`, placement: placementNow, middleware }).then((d) => {
        if (disposed) return
        const origin = d.middlewareData.transformOrigin as { x: string; y: string } | undefined
        const same = untrack(isPositioned) && pos.x === d.x && pos.y === d.y && pos.origin?.x === origin?.x && pos.origin?.y === origin?.y && untrack(placement) === d.placement
        if (same) return
        pos = { x: d.x, y: d.y, origin }
        setPlacement(d.placement)
        setPositioned(true)
        paintWrapper()
      })
    }
    const stop = autoUpdate(anchor, wrapper, update, { animationFrame: false })
    onCleanup(stop)
  })
  onCleanup(() => (disposed = true))
  return wrapper
}

// ---------------------------------------------------------------------------
// Shared pieces of the overlays
// ---------------------------------------------------------------------------

/** Radix's triggers write `type="button"` on the slot wrapper span too. */
const TYPE_BUTTON: Attrs = { type: `button` }

/** An overlay's `open`: local mirror, bound path written, `change {open}`. */
function createOpenState(props: NativeProps): [Accessor<boolean>, (next: boolean) => void] {
  const [open, setOpen] = createBoundState(
    () => props.node,
    () => props.scope,
    `open`,
    () => bool(props.props.open)
  )
  const change = (next: boolean) => {
    setOpen(next)
    void props.emit(`change`, { open: next }, { open: next })
  }
  return [open, change]
}

/** Round 2 §7: an overlay is LAYOUT-TRANSPARENT — its frame is its
 *  trigger's; without a trigger it lays out nothing. */
function overlayRootProps(rootProps: Record<string, unknown>, trigger: boolean, triggerNode?: { component: string } | null): Record<string, unknown> {
  if (!trigger) return { ...rootProps, "data-xui-overlay-root": `none` }
  return { ...rootProps, "data-xui-overlay-root": triggerNode?.component === `Link` ? `stretch` : `` }
}

/** The content's own style the way Radix composes it: `prefix` (a menu's
 *  `outline`), the dismissable layer's `pointer-events`, the popper's
 *  `--radix-<kind>-*` variables, then `animation: none` until placed. */
function popperContentStyle(kind: string, trigger: string, pointerEvents: string | undefined, positioned: boolean, prefix: StyleObject = {}): StyleObject {
  return {
    ...prefix,
    "pointer-events": pointerEvents,
    [`--radix-${kind}-content-transform-origin`]: `var(--radix-popper-transform-origin)`,
    [`--radix-${kind}-content-available-width`]: `var(--radix-popper-available-width)`,
    [`--radix-${kind}-content-available-height`]: `var(--radix-popper-available-height)`,
    [`--radix-${kind}-${trigger}-width`]: `var(--radix-popper-anchor-width)`,
    [`--radix-${kind}-${trigger}-height`]: `var(--radix-popper-anchor-height)`,
    animation: positioned ? undefined : `none`,
  }
}

/** Write `style()` with React's semantics on `el`, first now, then on change. */
function bindStyle(el: HTMLElement, style: Accessor<StyleObject>) {
  const write = styleWriter(el)
  createRenderEffect(() => write(style()))
}

// ---------------------------------------------------------------------------
// Dialog + Drawer
// ---------------------------------------------------------------------------

export function DialogNative(props: NativeProps): JSX.Element {
  return <ModalNative {...props} kind="Dialog" />
}
export function DrawerNative(props: NativeProps): JSX.Element {
  return <ModalNative {...props} kind="Drawer" />
}

/** Drawer drag-to-dismiss: a drag toward the drawer's edge past 80 px (or a
 *  flick) closes it; anything less springs back. */
function createDragToDismiss(side: Accessor<string>, enabled: Accessor<boolean>, onDismiss: () => void, reducedMotion: Accessor<boolean>) {
  let panel: HTMLElement | undefined
  let drag: { id: number; x: number; y: number; at: number } | null = null
  const sign = () => (side() === `bottom` || side() === `right` ? 1 : -1)
  const axis = () => (side() === `left` || side() === `right` ? `x` : `y`)
  const distance = (e: PointerEvent) => (drag ? Math.max(0, (axis() === `x` ? e.clientX - drag.x : e.clientY - drag.y) * sign()) : 0)
  const paint = (px: number, animate: boolean) => {
    if (!panel) return
    panel.style.transition = animate && !reducedMotion() ? `transform var(--xui-motion-standard) var(--xui-ease-decelerate)` : `none`
    panel.style.transform = px ? `translate${axis() === `x` ? `X` : `Y`}(${px * sign()}px)` : ``
  }
  return {
    setPanel: (el: HTMLElement) => (panel = el),
    onPointerDown: (e: PointerEvent) => {
      if (!untrack(enabled) || e.button !== 0) return
      drag = { id: e.pointerId, x: e.clientX, y: e.clientY, at: performance.now() }
      ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
    },
    onPointerMove: (e: PointerEvent) => {
      if (untrack(enabled) && drag?.id === e.pointerId) paint(distance(e), false)
    },
    onPointerUp: (e: PointerEvent) => {
      if (!untrack(enabled) || drag?.id !== e.pointerId) return
      const px = distance(e)
      const dt = performance.now() - drag.at
      drag = null
      if (px > 80 || (px > 24 && dt > 0 && px / dt > 0.6)) {
        paint(0, false)
        onDismiss()
      } else paint(0, true)
    },
    onPointerCancel: () => {
      if (!untrack(enabled)) return
      drag = null
      paint(0, true)
    },
  }
}

function ModalNative(props: NativeProps & { kind: `Dialog` | `Drawer` }): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const [open, setOpen] = createOpenState(props)
  const dismissible = () => props.props.dismissible !== false
  const side = () => str(props.props.side, `bottom`)
  const title = () => str(props.props.title)
  const description = () => str(props.props.description)
  const contentId = createUniqueId()
  const titleId = createUniqueId()
  const descriptionId = createUniqueId()
  const onOpenChange = (next: boolean) => (next || untrack(dismissible) ? setOpen(next) : undefined)
  const close = () => setOpen(false)
  const drag = createDragToDismiss(side, () => props.kind === `Drawer` && props.props.dragToDismiss !== false && dismissible(), () => setOpen(false), () => ctx.reducedMotion)
  const hasTrigger = () => Boolean(props.node.slots?.trigger)
  let triggerEl: HTMLSpanElement | undefined
  const state = () => (open() ? `open` : `closed`)
  const overlayPresence = createPresence(open)
  const contentPresence = createPresence(open)

  const Content = (): JSX.Element => {
    const layer = createLayer({
      disableOutsidePointerEvents: () => true,
      onEscapeKeyDown: (e) => {
        if (!untrack(dismissible)) e.preventDefault()
      },
      onPointerDownOutside: (e) => {
        const original = e.detail.originalEvent as PointerEvent
        const ctrlLeftClick = original.button === 0 && original.ctrlKey === true
        if (original.button === 2 || ctrlLeftClick) e.preventDefault()
        if (!untrack(dismissible)) e.preventDefault()
      },
      onFocusOutside: (e) => e.preventDefault(),
      onInteractOutside: (e) => {
        if (!untrack(dismissible)) e.preventDefault()
      },
      onDismiss: () => onOpenChange(false),
    })
    const scope = createFocusScope({
      trapped: () => true,
      loop: true,
      onMountAutoFocus: (e) => {
        // A part marked `accessibility.autoFocus` (contract §6: the
        // AlertDialog's cancel) takes the focus, else the cancel action.
        const content = e.currentTarget as HTMLElement | null
        const marked = content?.querySelector<HTMLElement>(`[data-xui-autofocus]`) ?? content?.querySelector<HTMLElement>(`[data-xui-part="AlertDialog/cancel"]`)
        const target = marked && (marked.matches(`button,input,textarea,select,a[href],[tabindex]`) ? marked : marked.querySelector<HTMLElement>(`button,input,textarea,select,a[href],[tabindex]`))
        if (target) {
          e.preventDefault()
          target.focus()
        }
      },
      onUnmountAutoFocus: (e) => {
        e.preventDefault()
        triggerEl?.focus()
      },
    })
    const [registered, setRegistered] = createSignal(false)
    const el = (
      <div
        {...part(`content`, open() && `open`)}
        data-side={props.kind === `Drawer` ? side() : undefined}
        data-xui-overlay={props.kind}
        dir={ctx.direction}
        role={props.kind === `Dialog` && !dismissible() ? `alertdialog` : `dialog`}
        id={contentId}
        aria-describedby={description() ? descriptionId : undefined}
        aria-labelledby={titleId}
        data-state={state()}
        tabindex="-1"
        onKeyDown={scope.onKeyDown}
      >
        {props.kind === `Drawer` && (side() === `bottom` || side() === `top`) ? <div {...part(`handle`)} aria-hidden="true" onPointerDown={drag.onPointerDown} onPointerMove={drag.onPointerMove} onPointerUp={drag.onPointerUp} on:pointercancel={drag.onPointerCancel} /> : null}
        {title() || description() ? (
          props.kind === `Drawer` ? (
            <div class="xui-overlay-head" onPointerDown={drag.onPointerDown} onPointerMove={drag.onPointerMove} onPointerUp={drag.onPointerUp} on:pointercancel={drag.onPointerCancel}>
              <ModalHead part={part} title={title()} description={description()} titleId={titleId} descriptionId={descriptionId} fallback={ctx.t(`dialog`)} />
            </div>
          ) : (
            <div class="xui-overlay-head">
              <ModalHead part={part} title={title()} description={description()} titleId={titleId} descriptionId={descriptionId} fallback={ctx.t(`dialog`)} />
            </div>
          )
        ) : (
          <h2 class="xui-sr-only" id={titleId}>
            {ctx.t(`dialog`)}
          </h2>
        )}
        {/* Round 4: a valid submit closes THIS overlay when it is the
            nearest one around the form and dismissible (an AlertDialog,
            dismissible false, never auto-closes). */}
        <CloseOnSubmitContext.Provider value={() => (untrack(dismissible) ? close() : undefined)}>
          <LayerContext.Provider value={layer.layer}>
            <div class="xui-overlay-body">{props.children}</div>
            <Show when={props.node.slots?.footer}>
              <div {...part(`footer`)}>{props.slots.footer}</div>
            </Show>
          </LayerContext.Provider>
        </CloseOnSubmitContext.Provider>
        {props.kind === `Dialog` && dismissible() ? (
          <button type="button" {...part(`close`)} aria-label={ctx.t(`close`)} onClick={() => onOpenChange(false)}>
            <BuiltinIcon slot="Dialog.close" />
          </button>
        ) : null}
      </div>
    ) as HTMLDivElement
    contentPresence.ref(el)
    drag.setPanel(el)
    bindStyle(el, () => ({ "pointer-events": layer.pointerEvents(registered()) }))
    onMount(() => {
      layer.attach(el)
      setRegistered(true)
      scope.attach(el)
      useFocusGuards()
      useModalEffects(el, false)
    })
    return el
  }

  const Overlay = (): JSX.Element => {
    const el = (<div {...part(`overlay`, open() && `open`)} data-xui-overlay-backdrop={props.kind} data-state={state()} />) as HTMLDivElement
    overlayPresence.ref(el)
    bindStyle(el, () => ({ "pointer-events": `auto` }))
    // react-remove-scroll sits on the Dialog's overlay.
    onMount(() => useScrollLock())
    return el
  }

  return (
    <>
      <div {...overlayRootProps(props.rootProps, hasTrigger(), props.node.slots?.trigger)}>
        <Show when={hasTrigger()}>
          <span ref={triggerEl} class="xui-trigger" {...TYPE_BUTTON} aria-haspopup="dialog" aria-expanded={open()} aria-controls={contentId} data-state={state()} onClick={() => onOpenChange(!untrack(open))}>
            {props.slots.trigger}
          </span>
        </Show>
      </div>
      {/* Radix: one portal per part (the overlay's, then the content's). */}
      <LayerPortal mount={ctx.portal}>
        <Show when={overlayPresence.mounted()}>
          <Overlay />
        </Show>
      </LayerPortal>
      <LayerPortal mount={ctx.portal}>
        <Show when={contentPresence.mounted()}>
          <Content />
        </Show>
      </LayerPortal>
    </>
  )
}

function ModalHead(props: { part: PartFn; title: string; description: string; titleId: string; descriptionId: string; fallback: string }): JSX.Element {
  return (
    <>
      {props.title ? (
        <h2 {...props.part(`title`)} id={props.titleId}>
          {props.title}
        </h2>
      ) : (
        <h2 class="xui-sr-only" id={props.titleId}>
          {props.fallback}
        </h2>
      )}
      {props.description ? (
        <p {...props.part(`description`)} id={props.descriptionId}>
          {props.description}
        </p>
      ) : null}
    </>
  )
}

// ---------------------------------------------------------------------------
// Popover
// ---------------------------------------------------------------------------

const HOVER_OPEN_MS = 200
const HOVER_CLOSE_MS = 150

export function PopoverNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const [open, setOpen] = createOpenState(props)
  const side = () => str(props.props.side, `bottom`) as Side
  // `openOn: hover` (a hover card): opens on pointer hover AND keyboard focus
  // of the trigger, stays while the pointer is over the content; touch
  // (no hover pointer) keeps press.
  const hoverMode = () => props.props.openOn === `hover` && ctx.hover
  const contentId = createUniqueId()
  const hasTrigger = () => Boolean(props.node.slots?.trigger)
  let timer: ReturnType<typeof setTimeout> | undefined
  // THIS popover's content (focus moving from the trigger into it keeps it open).
  let contentEl: HTMLElement | undefined
  const schedule = (next: boolean) => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(
      () => {
        if (untrack(open) !== next) setOpen(next)
      },
      next ? HOVER_OPEN_MS : HOVER_CLOSE_MS
    )
  }
  onCleanup(() => timer && clearTimeout(timer))
  const [anchor, setAnchor] = createSignal<HTMLElement>()
  const state = () => (open() ? `open` : `closed`)
  const presence = createPresence(open)

  const Content = (): JSX.Element => {
    let hasInteractedOutside = false
    let hasPointerDownOutside = false
    const layer = createLayer({
      onInteractOutside: (e) => {
        if (!e.defaultPrevented) {
          hasInteractedOutside = true
          if (e.detail.originalEvent.type === `pointerdown`) hasPointerDownOutside = true
        }
        const target = e.target as Node | null
        if (target && anchor()?.contains(target) && hasTrigger()) e.preventDefault()
        if (e.detail.originalEvent.type === `focusin` && hasPointerDownOutside) e.preventDefault()
      },
      onDismiss: () => setOpen(false),
    })
    const scope = createFocusScope({
      trapped: () => false,
      loop: true,
      onMountAutoFocus: (e) => {
        if (untrack(hoverMode)) e.preventDefault()
      },
      onUnmountAutoFocus: (e) => {
        if (!hasInteractedOutside && hasTrigger()) anchor()?.focus()
        e.preventDefault()
        hasInteractedOutside = false
        hasPointerDownOutside = false
      },
    })
    const [registered, setRegistered] = createSignal(false)
    return (
      <Popper anchor={anchor()} side={side()} align="center" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} dir={ctx.direction}>
        {(popper) => {
          const el = (
            <div
              {...part(`content`, open() && `open`)}
              data-side={popper.placedSide()}
              data-align={popper.placedAlign()}
              data-state={state()}
              role="dialog"
              id={contentId}
              data-xui-overlay="Popover"
              data-open-on={hoverMode() ? `hover` : undefined}
              dir={ctx.direction}
              tabindex="-1"
              onKeyDown={scope.onKeyDown}
              onPointerEnter={() => hoverMode() && schedule(true)}
              onPointerLeave={() => hoverMode() && schedule(false)}
            >
              {/* The nearest overlay decides: a Form in a Popover closes nothing,
                  not even a Dialog around the Popover. */}
              <CloseOnSubmitContext.Provider value={null}>
                <LayerContext.Provider value={layer.layer}>{props.children}</LayerContext.Provider>
              </CloseOnSubmitContext.Provider>
            </div>
          ) as HTMLDivElement
          contentEl = el
          presence.ref(el)
          popper.setContent(el)
          bindStyle(el, () => popperContentStyle(`popover`, `trigger`, layer.pointerEvents(registered()), popper.isPositioned()))
          onMount(() => {
            layer.attach(el)
            setRegistered(true)
            scope.attach(el)
            useFocusGuards()
          })
          return el
        }}
      </Popper>
    )
  }

  return (
    <>
      <div {...overlayRootProps(props.rootProps, hasTrigger(), props.node.slots?.trigger)}>
        <Show when={hasTrigger()} fallback={<span ref={setAnchor} style={css({ display: `inline-flex` })} />}>
          <span
            ref={setAnchor}
            class="xui-trigger"
            {...TYPE_BUTTON}
            aria-haspopup="dialog"
            aria-expanded={open()}
            aria-controls={contentId}
            data-state={state()}
            onClick={() => setOpen(!untrack(open))}
            onPointerEnter={(e) => hoverMode() && e.pointerType !== `touch` && schedule(true)}
            onPointerLeave={(e) => hoverMode() && e.pointerType !== `touch` && schedule(false)}
            onFocusIn={() => hoverMode() && schedule(true)}
            onFocusOut={(e) => {
              if (!hoverMode()) return
              const to = e.relatedTarget as Node | null
              if (!to || !contentEl?.contains(to)) schedule(false)
            }}
          >
            {props.slots.trigger}
          </span>
        </Show>
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <Content />
        </Show>
      </LayerPortal>
    </>
  )
}

// ---------------------------------------------------------------------------
// Tooltip
// ---------------------------------------------------------------------------

const TOOLTIP_OPEN = `tooltip.open`
const TOOLTIP_DELAY_MS = 300
const TOOLTIP_SKIP_DELAY_MS = 300

type Point = { x: number; y: number }
/** Is the point inside the polygon (ray casting, Radix's `isPointInPolygon`)? */
function inPolygon(point: Point, polygon: Point[]): boolean {
  let inside = false
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const a = polygon[i]
    const b = polygon[j]
    if (a.y > point.y !== b.y > point.y && point.x < ((b.x - a.x) * (point.y - a.y)) / (b.y - a.y) + a.x) inside = !inside
  }
  return inside
}
/** The convex hull of points (Andrew's monotone chain, Radix's `getHull`). */
function hull(points: Point[]): Point[] {
  const sorted = [...points].sort((a, b) => (a.x < b.x ? -1 : a.x > b.x ? 1 : a.y < b.y ? -1 : a.y > b.y ? 1 : 0))
  if (sorted.length <= 1) return sorted
  const cross = (o: Point, a: Point, b: Point) => (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
  const upper: Point[] = []
  for (const p of sorted) {
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p) >= 0) upper.pop()
    upper.push(p)
  }
  upper.pop()
  const lower: Point[] = []
  for (let i = sorted.length - 1; i >= 0; i--) {
    const p = sorted[i]
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], p) >= 0) lower.pop()
    lower.push(p)
  }
  lower.pop()
  return upper.length === 1 && lower.length === 1 && upper[0].x === lower[0].x && upper[0].y === lower[0].y ? upper : upper.concat(lower)
}
/** The grace area from the exit point to a hover target's box. */
function graceArea(exit: Point, from: DOMRect, to: DOMRect): Point[] {
  const padding = 5
  const top = Math.abs(from.top - exit.y)
  const bottom = Math.abs(from.bottom - exit.y)
  const right = Math.abs(from.right - exit.x)
  const left = Math.abs(from.left - exit.x)
  const m = Math.min(top, bottom, right, left)
  const padded: Point[] =
    m === left
      ? [{ x: exit.x + padding, y: exit.y - padding }, { x: exit.x + padding, y: exit.y + padding }]
      : m === right
        ? [{ x: exit.x - padding, y: exit.y - padding }, { x: exit.x - padding, y: exit.y + padding }]
        : m === top
          ? [{ x: exit.x - padding, y: exit.y + padding }, { x: exit.x + padding, y: exit.y + padding }]
          : [{ x: exit.x - padding, y: exit.y - padding }, { x: exit.x + padding, y: exit.y - padding }]
  return hull([...padded, { x: to.left, y: to.top }, { x: to.right, y: to.top }, { x: to.right, y: to.bottom }, { x: to.left, y: to.bottom }])
}

/** Radix `VisuallyHidden` with `role="tooltip"` (its inline style, in its order). */
function VisuallyHidden(props: { id: string; children: JSX.Element }): JSX.Element {
  const el = (
    <span id={props.id} role="tooltip">
      {props.children}
    </span>
  ) as HTMLSpanElement
  styleWriter(el)({ position: `absolute`, border: `0`, width: `1px`, height: `1px`, padding: `0`, margin: `-1px`, overflow: `hidden`, clip: `rect(0, 0, 0, 0)`, "white-space": `nowrap`, "word-wrap": `normal` })
  return el
}

/** Tooltip: shows on hover AND keyboard focus (a11y.json), Escape hides. */
export function TooltipNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const side = () => str(props.props.side, `top`) as Side
  const content = () => str(props.props.content) || str(props.props.text)
  const contentId = createUniqueId()
  const [open, setOpenState] = createSignal(false)
  const [delayed, setDelayed] = createSignal(false)
  let openDelayed = true
  let openTimer: ReturnType<typeof setTimeout> | undefined
  let skipTimer: ReturnType<typeof setTimeout> | undefined
  let pointerMoveOpened = false
  let pointerDown = false
  let anchorEl: HTMLDivElement | undefined
  let contentEl: HTMLElement | undefined
  const setOpen = (next: boolean) => {
    if (next) {
      clearTimeout(skipTimer)
      openDelayed = false
      document.dispatchEvent(new CustomEvent(TOOLTIP_OPEN))
    } else {
      clearTimeout(skipTimer)
      skipTimer = setTimeout(() => (openDelayed = true), TOOLTIP_SKIP_DELAY_MS)
    }
    setOpenState(next)
  }
  const handleOpen = () => {
    clearTimeout(openTimer)
    setDelayed(false)
    setOpen(true)
  }
  const handleClose = () => {
    clearTimeout(openTimer)
    if (untrack(open)) setOpen(false)
  }
  const handleDelayedOpen = () => {
    clearTimeout(openTimer)
    openTimer = setTimeout(() => {
      setDelayed(true)
      setOpen(true)
    }, TOOLTIP_DELAY_MS)
  }
  onCleanup(() => {
    clearTimeout(openTimer)
    clearTimeout(skipTimer)
  })
  const stateAttribute = () => (open() ? (delayed() ? `delayed-open` : `instant-open`) : `closed`)
  // Hoverable content: leaving the trigger toward the content keeps it open.
  let grace: Point[] | null = null
  const trackGrace = (e: PointerEvent) => {
    if (!grace) return
    const target = e.target as Node | null
    const point = { x: e.clientX, y: e.clientY }
    const overTrigger = anchorEl?.contains(target) ?? false
    const overContent = contentEl?.contains(target) ?? false
    if (overTrigger || overContent) grace = null
    else if (!inPolygon(point, grace)) {
      grace = null
      handleClose()
    }
  }
  // Radix tracks the grace area from an effect after the leave: the
  // pointer move that caused the leave itself is never judged.
  const leave = (e: PointerEvent, from: HTMLElement | undefined, to: HTMLElement | undefined) => {
    if (!from || !to || !untrack(open)) return
    const area = graceArea({ x: e.clientX, y: e.clientY }, from.getBoundingClientRect(), to.getBoundingClientRect())
    setTimeout(() => (grace = area), 0)
  }
  document.addEventListener(`pointermove`, trackGrace)
  onCleanup(() => document.removeEventListener(`pointermove`, trackGrace))
  const presence = createPresence(open)

  const Content = (): JSX.Element => {
    const layer = createLayer({ onFocusOutside: (e) => e.preventDefault(), onDismiss: handleClose })
    const [registered, setRegistered] = createSignal(false)
    const onOtherOpen = () => handleClose()
    const onScroll = (e: Event) => {
      const target = e.target as Node | null
      if (target && anchorEl && (target as Node).contains?.(anchorEl)) handleClose()
    }
    return (
      <Popper anchor={anchorEl} side={side()} align="center" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING}>
        {(popper) => {
          const el = (
            <div {...part(`content`)} data-side={popper.placedSide()} data-align={popper.placedAlign()} data-state={stateAttribute()} data-xui-overlay="Tooltip" onPointerLeave={(e) => leave(e, contentEl, anchorEl)}>
              {content()}
              <VisuallyHidden id={contentId}>{content()}</VisuallyHidden>
            </div>
          ) as HTMLDivElement
          contentEl = el
          presence.ref(el)
          popper.setContent(el)
          bindStyle(el, () => popperContentStyle(`tooltip`, `trigger`, layer.pointerEvents(registered()), popper.isPositioned()))
          onMount(() => {
            layer.attach(el)
            setRegistered(true)
            // Another tooltip opening closes this one; so does scrolling the trigger away.
            setTimeout(() => document.addEventListener(TOOLTIP_OPEN, onOtherOpen), 0)
            window.addEventListener(`scroll`, onScroll, { capture: true })
            onCleanup(() => {
              document.removeEventListener(TOOLTIP_OPEN, onOtherOpen)
              window.removeEventListener(`scroll`, onScroll, { capture: true })
            })
          })
          return el
        }}
      </Popper>
    )
  }

  // The tooltip wraps its children in the ANCHOR (`<id>.anchor`), the frame
  // it shows against; the root is layout-transparent around it.
  return (
    <>
      <div {...props.rootProps} data-xui-overlay-root="">
        <div
          ref={anchorEl}
          {...part(`anchor`)}
          tabindex="0"
          aria-describedby={open() ? contentId : undefined}
          data-state={stateAttribute()}
          onPointerMove={(e) => {
            if (e.pointerType === `touch`) return
            if (!pointerMoveOpened && !grace) {
              if (openDelayed) handleDelayedOpen()
              else handleOpen()
              pointerMoveOpened = true
            }
          }}
          onPointerLeave={(e) => {
            clearTimeout(openTimer)
            pointerMoveOpened = false
            leave(e, anchorEl, contentEl)
          }}
          onPointerDown={() => {
            if (untrack(open)) handleClose()
            pointerDown = true
            document.addEventListener(`pointerup`, () => setTimeout(() => (pointerDown = false), 0), { once: true })
          }}
          onFocusIn={() => {
            if (!pointerDown) handleOpen()
          }}
          onFocusOut={handleClose}
          onClick={handleClose}
        >
          {props.children}
        </div>
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <Content />
        </Show>
      </LayerPortal>
    </>
  )
}

// ---------------------------------------------------------------------------
// Menu (round 3: ONE native for press and context menus)
// ---------------------------------------------------------------------------

interface MenuItem {
  label?: string
  value?: string
  kind?: `item` | `checkbox` | `separator` | `label` | `submenu`
  icon?: string
  shortcut?: string
  checked?: boolean
  destructive?: boolean
  disabled?: boolean
  separator?: boolean
  items?: MenuItem[]
  /** EXP-1259: the submenu is a search list of its entries. */
  searchable?: boolean
  placeholder?: string
}

/** Whether the person drives the menus with the keyboard (Radix's
 *  `isUsingKeyboardRef`: a keydown says yes, a pointer says no). */
let usingKeyboard = false
let keyboardListeners = 0
function useKeyboardTracking() {
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

interface MenuRoot {
  close: () => void
  dir: () => `ltr` | `rtl`
  overlay: string
  openOn: string
  part: PartFn
  emit: NativeProps[`emit`]
  scope: () => string
}
const MenuRootContext = createContext<MenuRoot>()

/** One menu level's content (Radix `MenuContentImpl`): roving focus over its
 *  items, typeahead, first/last keys, the pointer grace toward a submenu. */
interface MenuLevel {
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
const MenuLevelContext = createContext<MenuLevel>()

const FIRST_KEYS = [`ArrowDown`, `PageUp`, `Home`]
const LAST_KEYS = [`ArrowUp`, `PageDown`, `End`]
const itemsOf = (content: HTMLElement | undefined) => (content ? Array.from(content.querySelectorAll<HTMLElement>(`:scope [data-radix-collection-item]`)).filter((el) => el.closest(`[data-radix-menu-content]`) === content) : [])
const wrapArray = <T,>(array: T[], start: number) => array.map((_, i) => array[(start + i) % array.length])

function getNextMatch(values: string[], search: string, currentMatch?: string): string | undefined {
  const isRepeated = search.length > 1 && Array.from(search).every((c) => c === search[0])
  const normalized = isRepeated ? search[0] : search
  const currentIndex = currentMatch ? values.indexOf(currentMatch) : -1
  let wrapped = wrapArray(values, Math.max(currentIndex, 0))
  if (normalized.length === 1) wrapped = wrapped.filter((v) => v !== currentMatch)
  const next = wrapped.find((v) => v.toLowerCase().startsWith(normalized.toLowerCase()))
  return next !== currentMatch ? next : undefined
}

type MenuContentProps = {
  anchor: Element | VirtualElement | null | undefined
  side: Side
  align: Align
  sideOffset: number
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
  onKeyDown?: (e: KeyboardEvent) => void
  setContent?: (el: HTMLElement | undefined) => void
  children: JSX.Element
}

function MenuContent(props: MenuContentProps): JSX.Element {
  const root = useContext(MenuRootContext)!
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
    <Popper anchor={props.anchor} side={props.side} align={props.align} sideOffset={props.sideOffset} collisionPadding={OVERLAY_PADDING} dir={root.dir()}>
      {(popper) => {
        const el = (
          <div
            data-side={popper.placedSide()}
            data-align={popper.placedAlign()}
            role="menu"
            aria-orientation="vertical"
            data-state={props.state()}
            data-radix-menu-content=""
            dir={root.dir()}
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
        const kind = root.openOn === `contextmenu` ? `context-menu` : `dropdown-menu`
        bindStyle(el, () => popperContentStyle(kind, `trigger`, layer.pointerEvents(registered()), popper.isPositioned(), { outline: `none` }))
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
function createMenuRow(disabled: Accessor<boolean>) {
  const level = useContext(MenuLevelContext)!
  let el: HTMLElement | undefined
  const attrs = (): Attrs => ({
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
 *  the close. */
function createSelectable(disabled: Accessor<boolean>, onSelect: (e: Event) => void) {
  const root = useContext(MenuRootContext)!
  const row = createMenuRow(disabled)
  let pointerDownHere = false
  const select = () => {
    if (untrack(disabled)) return
    const e = new CustomEvent(`menu.itemSelect`, { bubbles: true, cancelable: true })
    onSelect(e)
    if (e.defaultPrevented) pointerDownHere = false
    else root.close()
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

function MenuEntry(props: { item: MenuItem; raw: Record<string, unknown> | undefined; path: string }): JSX.Element {
  const root = useContext(MenuRootContext)!
  const kind = () => (props.item.separator ? `separator` : (props.item.kind ?? `item`))
  return (
    <Dynamic
      component={(() => {
        const k = kind()
        if (k === `separator`) return MenuSeparator
        if (k === `label`) return MenuLabel
        if (k === `submenu`) return MenuSub
        if (k === `checkbox`) return MenuCheckbox
        return MenuPlainItem
      })()}
      item={props.item}
      raw={props.raw}
      path={props.path}
      root={root}
    />
  )
}

type EntryProps = { item: MenuItem; raw: Record<string, unknown> | undefined; path: string; root: MenuRoot }

function MenuSeparator(props: EntryProps): JSX.Element {
  return <div role="separator" aria-orientation="horizontal" {...props.root.part(`separator`)} />
}
function MenuLabel(props: EntryProps): JSX.Element {
  return <div {...props.root.part(`label`)}>{str(props.item.label)}</div>
}

function ItemIcon(props: { name?: string }): JSX.Element {
  const ctx = useSurfaceContext()
  return <Show when={props.name}>{(name) => <IconGlyph icons={ctx.host.icons} name={name()} />}</Show>
}
function Shortcut(props: { text?: string; part: PartFn }): JSX.Element {
  return <Show when={props.text}>{(t) => <span {...props.part(`shortcut`)}>{t()}</span>}</Show>
}

function MenuPlainItem(props: EntryProps): JSX.Element {
  const disabled = () => bool(props.item.disabled)
  const row = createSelectable(disabled, () => void props.root.emit(`select`, { value: props.item.value ?? props.item.label }))
  return (
    <div
      ref={row.ref}
      role="menuitem"
      {...props.root.part(`item`, disabled() && `disabled`)}
      data-destructive={props.item.destructive ? `true` : undefined}
      {...row.attrs()}
      onClick={row.handlers.onClick}
      onPointerDown={row.handlers.onPointerDown}
      onPointerUp={row.handlers.onPointerUp}
      onPointerMove={row.handlers.onPointerMove}
      onPointerLeave={row.handlers.onPointerLeave}
      onKeyDown={row.handlers.onKeyDown}
      onFocus={row.handlers.onFocus}
      onBlur={row.handlers.onBlur}
    >
      <ItemIcon name={props.item.icon} />
      <span class="xui-menu-label">{str(props.item.label)}</span>
      <Shortcut text={props.item.shortcut} part={props.root.part} />
    </div>
  )
}

function MenuCheckbox(props: EntryProps): JSX.Element {
  const ctx = useSurfaceContext()
  const disabled = () => bool(props.item.disabled)
  const checked = () => bool(props.item.checked)
  const row = createSelectable(disabled, (e) => {
    const next = !untrack(checked)
    const raw = props.raw?.checked
    if (isBinding(raw)) ctx.setData(absolutePath(raw.path, props.root.scope()), next)
    void props.root.emit(`select`, { value: props.item.value ?? props.item.label, checked: next })
    e.preventDefault()
  })
  return (
    <div
      ref={row.ref}
      role="menuitemcheckbox"
      aria-checked={checked() ? `true` : `false`}
      {...props.root.part(`item`, disabled() && `disabled`, checked() && `checked`)}
      data-state={checked() ? `checked` : `unchecked`}
      {...row.attrs()}
      onClick={row.handlers.onClick}
      onPointerDown={row.handlers.onPointerDown}
      onPointerUp={row.handlers.onPointerUp}
      onPointerMove={row.handlers.onPointerMove}
      onPointerLeave={row.handlers.onPointerLeave}
      onKeyDown={row.handlers.onKeyDown}
      onFocus={row.handlers.onFocus}
      onBlur={row.handlers.onBlur}
    >
      <span {...props.root.part(`check`)}>
        <Show when={checked()}>
          <span data-state="checked">
            <BuiltinIcon slot="Menu.check" />
          </span>
        </Show>
      </span>
      <span class="xui-menu-label">{str(props.item.label)}</span>
      <Shortcut text={props.item.shortcut} part={props.root.part} />
    </div>
  )
}

function MenuSub(props: EntryProps): JSX.Element {
  const ctx = useSurfaceContext()
  const disabled = () => bool(props.item.disabled)
  const row = createMenuRow(disabled)
  const [open, setOpen] = createSignal(false)
  const triggerId = createUniqueId()
  const contentId = createUniqueId()
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const [content, setContent] = createSignal<HTMLElement>()
  let openTimer: ReturnType<typeof setTimeout> | undefined
  let graceTimer: ReturnType<typeof setTimeout> | undefined
  onCleanup(() => {
    clearTimeout(openTimer)
    clearTimeout(graceTimer)
  })
  const rtl = () => props.root.dir() === `rtl`
  const openKeys = () => [`Enter`, ` `, rtl() ? `ArrowLeft` : `ArrowRight`]
  const closeKey = () => (rtl() ? `ArrowRight` : `ArrowLeft`)
  const presence = createPresence(open)
  return (
    <>
      <div
        ref={(el) => {
          row.ref(el)
          setTrigger(el)
        }}
        role="menuitem"
        id={triggerId}
        aria-haspopup="menu"
        aria-expanded={open() ? `true` : `false`}
        aria-controls={contentId}
        data-state={open() ? `open` : `closed`}
        {...props.root.part(`item`, disabled() && `disabled`)}
        {...row.attrs()}
        onClick={() => {
          if (!untrack(disabled) && !untrack(open)) setOpen(true)
        }}
        onPointerMove={(e: PointerEvent) => {
          row.handlers.onPointerMove(e)
          if (e.pointerType !== `mouse`) return
          if (!untrack(disabled) && !untrack(open) && !openTimer) {
            row.level.setGrace(null)
            openTimer = setTimeout(() => {
              setOpen(true)
              openTimer = undefined
            }, 100)
          }
        }}
        onPointerLeave={(e: PointerEvent) => {
          if (e.pointerType !== `mouse`) return
          clearTimeout(openTimer)
          openTimer = undefined
          const rect = content()?.getBoundingClientRect()
          if (rect) {
            const s = content()?.dataset.side as Side
            const right = s === `right`
            const bleed = right ? -5 : 5
            const near = rect[right ? `left` : `right`]
            const far = rect[right ? `right` : `left`]
            row.level.setGrace({
              area: [
                { x: e.clientX + bleed, y: e.clientY },
                { x: near, y: rect.top },
                { x: far, y: rect.top },
                { x: far, y: rect.bottom },
                { x: near, y: rect.bottom },
              ],
              side: s,
            })
            clearTimeout(graceTimer)
            graceTimer = setTimeout(() => row.level.setGrace(null), 300)
          } else {
            if (row.level.onTriggerLeave(e)) return
            row.level.setGrace(null)
            row.handlers.onPointerLeave(e)
          }
        }}
        onKeyDown={(e: KeyboardEvent) => {
          const typingAhead = row.level.search() !== ``
          if (untrack(disabled) || (typingAhead && e.key === ` `)) return
          if (openKeys().includes(e.key)) {
            if (untrack(open)) content()?.focus({ preventScroll: true })
            else setOpen(true)
            e.preventDefault()
            return
          }
          row.handlers.onKeyDown(e)
        }}
        onFocus={row.handlers.onFocus}
        onBlur={row.handlers.onBlur}
      >
        <ItemIcon name={props.item.icon} />
        <span class="xui-menu-label">{str(props.item.label)}</span>
        <span {...props.root.part(`submenuIndicator`)}>
          <BuiltinIcon slot="Menu.submenuIndicator" />
        </span>
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <MenuContent
            anchor={trigger()}
            side={rtl() ? `left` : `right`}
            align="start"
            sideOffset={2}
            root={false}
            id={contentId}
            labelledBy={triggerId}
            state={() => (open() ? `open` : `closed`)}
            attrs={() => ({ ...props.root.part(`content`), "data-xui-overlay": `${props.root.overlay}.sub`, "data-searchable": props.item.searchable ? `true` : undefined })}
            setContent={(el) => {
              setContent(el)
              if (el) presence.ref(el)
            }}
            onDismiss={() => setOpen(false)}
            onEscapeKeyDown={(e) => {
              props.root.close()
              e.preventDefault()
            }}
            onFocusOutside={(e) => {
              if (e.target !== trigger()) setOpen(false)
            }}
            onCloseAutoFocus={(e) => e.preventDefault()}
            onKeyDown={(e) => {
              const el = content()
              if (el?.contains(e.target as Node) && e.key === closeKey()) {
                setOpen(false)
                trigger()?.focus()
                e.preventDefault()
              }
            }}
          >
            <Show when={props.item.searchable} fallback={<MenuEntries items={objects<MenuItem>(props.item.items)} raws={objects<Record<string, unknown>>(props.raw?.items)} path={`${props.path}.`} />}>
              <MenuSearch item={props.item} i={props.path} />
            </Show>
          </MenuContent>
        </Show>
      </LayerPortal>
    </>
  )
}

/** EXP-1259: a searchable submenu = the search list (head, the entries that
 *  match as menu rows, the highlighted one `selected`, the fade): its field
 *  takes the focus when it opens (and when the pointer enters), ↑/↓ move,
 *  Enter or Tab pick (the menu closes), ← at the caret's start returns to
 *  the parent row. The list itself is the SearchList's body (search.tsx). */
function MenuSearch(props: { item: MenuItem; i: string }): JSX.Element {
  const ctx = useSurfaceContext()
  const root = useContext(MenuRootContext)!
  const [query, setQuery] = createSignal(``)
  let box: HTMLDivElement | undefined
  // The field takes the focus once the opened content has it.
  onMount(() => {
    const frame = requestAnimationFrame(() => box?.querySelector<HTMLInputElement>(`input`)?.focus({ preventScroll: true }))
    onCleanup(() => cancelAnimationFrame(frame))
  })
  const rows = createMemo(() =>
    objects<MenuItem>(props.item.items)
      .map((entry, index) => ({ index, entry }))
      .filter(({ entry }) => entry.kind !== `separator` && entry.kind !== `label` && !entry.separator && searchMatches({ label: str(entry.label) }, query()))
      .map(({ index, entry }) => ({ index, item: { label: str(entry.label), value: str(entry.value ?? entry.label), icon: entry.icon, disabled: bool(entry.disabled) } as SearchItem }))
  )
  return (
    <div
      ref={box}
      class="xui-menu-search"
      onPointerEnter={(e) => {
        const input = e.currentTarget.querySelector<HTMLInputElement>(`input`)
        if (input && !e.currentTarget.contains(document.activeElement)) input.focus({ preventScroll: true })
      }}
    >
      <SearchBody
        part={root.part}
        owner="Menu"
        suffix={`.${props.i}`}
        rows={rows()}
        query={query()}
        onQuery={setQuery}
        placeholder={str(props.item.placeholder) || ctx.t(`search`)}
        emptyText={ctx.t(`noResults`)}
        maxRows={8}
        showCount
        autoFocus
        leaveBubbles
        onPick={(picked) => {
          void root.emit(`select`, { value: picked.value })
          root.close()
        }}
        rowPart={(_, __, active) => root.part(`item`, active && `selected`)}
        renderRow={(row) => (
          <>
            {row.icon ? <IconGlyph icons={ctx.host.icons} name={row.icon} /> : null}
            <span class="xui-menu-label">{row.label}</span>
          </>
        )}
      />
    </div>
  )
}

/** The entries of one level. A submenu's `items` may be a binding (`{path}`
 *  to rows a host source feeds): the bind pass resolved it already. */
function MenuEntries(props: { items: MenuItem[]; raws: Record<string, unknown>[]; path: string }): JSX.Element {
  const indexes = createMemo(() => props.items.map((_, i) => i), undefined, { equals: (a, b) => a.length === b.length })
  return <For each={indexes()}>{(i) => <MenuEntry item={props.items[i]} raw={props.raws[i]} path={`${props.path}${i}`} />}</For>
}

/** Menu: `openOn: press` (default) = a dropdown menu, its ONE child the
 *  trigger (no child = the default outline Button from label/icon, no
 *  chevron), anchored bottom-start; `openOn: contextmenu` = a context menu
 *  over the child (right-click / long-press / Shift+F10 / the Menu key),
 *  opened at the pointer (the child's corner from the keyboard). */
export function MenuNative(props: NativeProps): JSX.Element {
  return (
    <Show when={props.props.openOn === `contextmenu`} fallback={<PressMenuOpening {...props} />}>
      <ContextMenuOpening {...props} />
    </Show>
  )
}

function createMenuRoot(props: NativeProps, part: PartFn, close: () => void, openOn: string): MenuRoot {
  const ctx = useSurfaceContext()
  useKeyboardTracking()
  return { close, dir: () => ctx.direction, overlay: `Menu`, openOn, part, emit: props.emit, scope: () => props.scope }
}

function PressMenuOpening(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const items = () => objects<MenuItem>(props.props.items)
  const label = () => str(props.props.label)
  const icon = () => str(props.props.icon)
  const [open, setOpen] = createSignal(false)
  const hasChild = () => props.node.children.length > 0
  const triggerId = createUniqueId()
  const contentId = createUniqueId()
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const root = createMenuRoot(props, part, () => setOpen(false), `press`)
  const presence = createPresence(open)
  let interactedOutside = false
  const triggerAttrs = (): Attrs => ({
    type: `button`,
    id: triggerId,
    "aria-haspopup": `menu`,
    "aria-expanded": open() ? `true` : `false`,
    "aria-controls": open() ? contentId : undefined,
    "data-state": open() ? `open` : `closed`,
  })
  const onPointerDown = (e: PointerEvent) => {
    if (e.button === 0 && e.ctrlKey === false) {
      const was = untrack(open)
      setOpen(!was)
      if (!was) e.preventDefault()
    }
  }
  const onKeyDown = (e: KeyboardEvent) => {
    if ([`Enter`, ` `].includes(e.key)) setOpen(!untrack(open))
    if (e.key === `ArrowDown`) setOpen(true)
    if ([`Enter`, ` `, `ArrowDown`].includes(e.key)) e.preventDefault()
  }
  return (
    <MenuRootContext.Provider value={root}>
      <div {...overlayRootProps(props.rootProps, true, props.node.children[0] ?? null)}>
        <Show
          when={hasChild()}
          fallback={
            // Round 2 §7: the default trigger = an outline button with the
            // icon (when set) and the label (`$string.menu` without one and
            // without an icon), no chevron.
            <button ref={setTrigger} {...triggerAttrs()} {...part(`trigger`, open() && `open`)} aria-label={label() || ctx.t(`menu`)} onPointerDown={onPointerDown} onKeyDown={onKeyDown}>
              {icon() ? <IconGlyph icons={ctx.host.icons} name={icon()} class="xui-icon" width={16} height={16} /> : null}
              {label() || !icon() ? <span>{label() || ctx.t(`menu`)}</span> : null}
            </button>
          }
        >
          <span ref={setTrigger} class="xui-trigger" {...triggerAttrs()} onPointerDown={onPointerDown} onKeyDown={onKeyDown}>
            {props.children}
          </span>
        </Show>
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <MenuContent
            anchor={trigger()}
            side="bottom"
            align="start"
            sideOffset={OVERLAY_OFFSET}
            root
            id={contentId}
            labelledBy={triggerId}
            state={() => (open() ? `open` : `closed`)}
            attrs={() => ({ ...part(`content`, open() && `open`), "data-xui-overlay": `Menu`, "data-open-on": `press` })}
            setContent={(el) => el && presence.ref(el)}
            onDismiss={() => setOpen(false)}
            onInteractOutside={(e) => {
              const original = e.detail.originalEvent as PointerEvent
              const ctrlLeftClick = original.button === 0 && original.ctrlKey === true
              if (original.button === 2 || ctrlLeftClick) interactedOutside = true
            }}
            onCloseAutoFocus={(e) => {
              if (!interactedOutside) trigger()?.focus()
              interactedOutside = false
              e.preventDefault()
            }}
          >
            <MenuEntries items={items()} raws={objects<Record<string, unknown>>(props.node.props.items)} path="" />
          </MenuContent>
        </Show>
      </LayerPortal>
    </MenuRootContext.Provider>
  )
}

function ContextMenuOpening(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const items = () => objects<MenuItem>(props.props.items)
  const [open, setOpen] = createSignal(false)
  const [point, setPoint] = createSignal({ x: 0, y: 0 })
  const root = createMenuRoot(props, part, () => setOpen(false), `contextmenu`)
  const presence = createPresence(open)
  let longPress: ReturnType<typeof setTimeout> | undefined
  const clearLongPress = () => clearTimeout(longPress)
  onCleanup(clearLongPress)
  const openAt = (e: { clientX: number; clientY: number }) => {
    setPoint({ x: e.clientX, y: e.clientY })
    setOpen(true)
  }
  const anchor: VirtualElement = { getBoundingClientRect: () => DOMRect.fromRect({ width: 0, height: 0, ...point() }) }
  return (
    <MenuRootContext.Provider value={root}>
      <div
        {...props.rootProps}
        data-state={open() ? `open` : `closed`}
        on:contextmenu={(e: MouseEvent) => {
          clearLongPress()
          openAt(e)
          e.preventDefault()
        }}
        onPointerDown={(e: PointerEvent) => {
          if (e.pointerType === `mouse`) return
          clearLongPress()
          longPress = setTimeout(() => openAt(e), 700)
        }}
        onPointerMove={(e: PointerEvent) => e.pointerType !== `mouse` && clearLongPress()}
        on:pointercancel={(e: PointerEvent) => e.pointerType !== `mouse` && clearLongPress()}
        onPointerUp={(e: PointerEvent) => e.pointerType !== `mouse` && clearLongPress()}
        onKeyDown={(e: KeyboardEvent) => {
          if ((e.key === `F10` && e.shiftKey) || e.key === `ContextMenu`) {
            e.preventDefault()
            const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
            e.currentTarget!.dispatchEvent(new MouseEvent(`contextmenu`, { bubbles: true, cancelable: true, clientX: r.left + 8, clientY: r.top + 8 }))
          }
        }}
      >
        {props.children}
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <MenuContent
            anchor={anchor}
            side="right"
            align="start"
            sideOffset={2}
            root
            state={() => (open() ? `open` : `closed`)}
            attrs={() => ({ ...part(`content`, open() && `open`), "data-xui-overlay": `Menu`, "data-open-on": `contextmenu` })}
            setContent={(el) => el && presence.ref(el)}
            onDismiss={() => setOpen(false)}
            onCloseAutoFocus={() => undefined}
          >
            <MenuEntries items={items()} raws={objects<Record<string, unknown>>(props.node.props.items)} path="" />
          </MenuContent>
        </Show>
      </LayerPortal>
    </MenuRootContext.Provider>
  )
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

/** Toast (contract §3): in the TOAST layer, newest nearest the edge, at most
 *  3 visible (base CSS). The renderer runs `duration` (0 = sticky), PAUSED
 *  while hovered or focused, then writes `open: false` and fires `dismiss`
 *  + `change`. `error` is assertive, the rest polite; never takes focus. */
export function ToastNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const [open, setOpenState] = createBoundState(
    () => props.node,
    () => props.scope,
    `open`,
    () => (props.props.open === undefined ? true : bool(props.props.open))
  )
  const [leaving, setLeaving] = createSignal(false)
  const [paused, setPaused] = createSignal(false)
  const duration = () => (props.props.duration === undefined ? 5000 : Math.max(0, num(props.props.duration, 5000)))
  const type = () => str(props.props.type, `info`)
  let remaining = untrack(duration)
  let startedAt = 0
  const close = () => {
    setOpenState(false)
    void props.emit(`dismiss`, undefined, { open: false })
    void props.emit(`change`, { open: false }, { open: false })
  }
  // Exit: keep the element for the motion duration with data-state=closed.
  const [shown, setShown] = createSignal(untrack(open))
  createEffect(
    on(open, (o) => {
      if (o) {
        setShown(true)
        setLeaving(false)
        remaining = untrack(duration)
        return
      }
      if (!untrack(shown)) return
      setLeaving(true)
      const ms = ctx.reducedMotion ? 0 : (ctx.theme.tokens.motion?.standard ?? 180)
      const t = setTimeout(() => {
        setShown(false)
        setLeaving(false)
      }, ms)
      onCleanup(() => clearTimeout(t))
    }, { defer: true })
  )
  createEffect(() => {
    if (!open() || duration() === 0 || paused()) return
    startedAt = Date.now()
    const t = setTimeout(close, remaining)
    onCleanup(() => {
      clearTimeout(t)
      remaining = Math.max(0, remaining - (Date.now() - startedAt))
    })
  })
  const assertive = () => type() === `error`
  return (
    <LayerPortal mount={shown() ? ctx.toastLayer : null}>
      <div
        {...props.rootProps}
        role={assertive() ? `alert` : `status`}
        aria-live={assertive() ? `assertive` : `polite`}
        aria-atomic="true"
        data-state={leaving() ? `closed` : `open`}
        data-xui-overlay="Toast"
        onPointerEnter={() => setPaused(true)}
        onPointerLeave={() => setPaused(false)}
        onFocusIn={() => setPaused(true)}
        onFocusOut={() => setPaused(false)}
        onKeyDown={(e: KeyboardEvent) => {
          if (e.key === `Escape` && props.props.dismissible !== false) {
            e.stopPropagation()
            close()
          }
        }}
      >
        <span {...part(`icon`)}>
          <BuiltinIcon slot={`Toast.icon.${type()}`} />
        </span>
        <div class="xui-toast-text">
          <span {...part(`title`)}>{str(props.props.title)}</span>
          {str(props.props.description) ? <span {...part(`description`)}>{str(props.props.description)}</span> : null}
        </div>
        {str(props.props.actionLabel) ? (
          <button type="button" {...part(`action`)} onClick={() => void props.emit(`action`)}>
            {str(props.props.actionLabel)}
          </button>
        ) : null}
        {props.props.dismissible !== false ? (
          <button type="button" {...part(`close`)} aria-label={ctx.t(`dismiss`)} onClick={close}>
            <BuiltinIcon slot="Toast.close" />
          </button>
        ) : null}
      </div>
    </LayerPortal>
  )
}

export const NATIVES_OVERLAYS: Record<string, Component<NativeProps>> = {
  Dialog: DialogNative,
  Drawer: DrawerNative,
  Popover: PopoverNative,
  Tooltip: TooltipNative,
  Menu: MenuNative,
  Toast: ToastNative,
}
