// FEED-131: Radix `DismissableLayer`: the layer stack (Escape answers the
// highest layer only), a pointer down or a focus outside dismisses, a modal
// layer turns the body's pointer events off. `dismissable` is the Select's
// and Combobox's lighter variant with its OWN stack (no modal layers, no
// nested-layer containment), kept as it was ported.

import { createContext, createSignal, onCleanup, onMount, untrack, useContext, type Accessor } from "solid-js"

export interface Layer {
  el?: HTMLElement
  parent?: Layer
  kids: Set<Layer>
}
export const LAYERS: Layer[] = []
export const MODAL_LAYERS: Layer[] = []
const [layersVersion, setLayersVersion] = createSignal(0)
let originalBodyPointerEvents = ``
export const LayerContext = createContext<Layer | undefined>()

/** Inside this layer or one it opened (Radix checks the React tree, where a
 *  portalled child layer still sits under its parent). */
export function layerContains(layer: Layer, target: Node | null): boolean {
  if (!target) return false
  if (layer.el?.contains(target)) return true
  for (const kid of layer.kids) if (layerContains(kid, target)) return true
  return false
}

export interface OutsideEvent {
  target: EventTarget | null
  detail: { originalEvent: Event }
  defaultPrevented: boolean
  preventDefault: () => void
}
export const outsideEvent = (originalEvent: Event): OutsideEvent => {
  const e: OutsideEvent = { target: originalEvent.target, detail: { originalEvent }, defaultPrevented: false, preventDefault: () => (e.defaultPrevented = true) }
  return e
}

export interface LayerOptions {
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
export function createLayer(o: LayerOptions): { layer: Layer; attach: (el: HTMLElement) => void; pointerEvents: (registered: boolean) => string | undefined } {
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

/** The open overlay layers, innermost last: only the top one answers
 *  Escape (Radix's DismissableLayer stack). */
const layers: HTMLElement[] = []

/** Radix's DismissableLayer: Escape, a pointerdown outside (and, unless
 *  `keepOnFocusOutside`, focus moving outside) dismiss. `isInside` widens
 *  "inside" (the trigger of a non-modal popover). */
export function dismissable(opts: { layer: () => HTMLElement | undefined; onDismiss: () => void; isInside?: (target: Node) => boolean; keepOnFocusOutside?: boolean; onOutside?: () => void }) {
  onMount(() => {
    const layer = opts.layer()
    if (!layer) return
    layers.push(layer)
    const inside = (t: EventTarget | null) => t instanceof Node && (layer.contains(t) || Boolean(opts.isInside?.(t)))
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== `Escape` || layers[layers.length - 1] !== layer) return
      e.preventDefault()
      e.stopImmediatePropagation()
      opts.onDismiss()
    }
    const onPointer = (e: PointerEvent) => {
      if (inside(e.target)) return
      opts.onOutside?.()
      opts.onDismiss()
    }
    const onFocusIn = (e: FocusEvent) => {
      if (opts.keepOnFocusOutside || inside(e.target)) return
      opts.onOutside?.()
      opts.onDismiss()
    }
    document.addEventListener(`keydown`, onKey, { capture: true })
    document.addEventListener(`focusin`, onFocusIn)
    // Radix registers the pointer listener a tick later: the event that
    // opened the layer must not close it.
    const timer = window.setTimeout(() => document.addEventListener(`pointerdown`, onPointer))
    onCleanup(() => {
      window.clearTimeout(timer)
      document.removeEventListener(`keydown`, onKey, { capture: true })
      document.removeEventListener(`focusin`, onFocusIn)
      document.removeEventListener(`pointerdown`, onPointer)
      const i = layers.indexOf(layer)
      if (i >= 0) layers.splice(i, 1)
    })
  })
}
