// VAPP-276: an overlay's `anchor` prop names a node ELSEWHERE in the surface
// (a bottom-panel button opening the start menu or a calendar applet): the
// overlay is placed against that node's box, and the node becomes the
// overlay's trigger, exactly as the Rust core links it (`link_anchors`):
// a press toggles it, it carries `aria-haspopup` / `aria-expanded` /
// `aria-controls` / `data-state`, a press on it is never an "outside" press
// for a non-modal overlay, and Escape returns focus to it. The React and
// Solid natives share this half; the positioning stays the popper's.

/** The element of node `id` in the surface that holds `from` (the overlay's
 *  own wrapper or its portal layer); `null` while the node is not painted. */
export function findAnchor(from: Element | null | undefined, surfaceId: string, id: string): HTMLElement | null {
  const doc = from?.ownerDocument ?? (typeof document === `undefined` ? null : document)
  if (!doc || !id) return null
  const root = from?.closest(`[data-xui-surface]`) ?? doc.querySelector(`[data-xui-surface="${cssString(surfaceId)}"]`) ?? doc
  return root.querySelector<HTMLElement>(`[data-xui-id="${cssString(id)}"]`)
}

function cssString(s: string): string {
  return s.replace(/["\\]/g, `\\$&`)
}

/** A popper reference that follows `el` (floating-ui's virtual element:
 *  the box, the scroll context, and `focus` for a focus return). */
export interface VirtualAnchor {
  getBoundingClientRect: () => DOMRect
  readonly contextElement: Element | undefined
  focus: (options?: FocusOptions) => void
}

export function virtualAnchor(get: () => HTMLElement | null | undefined): VirtualAnchor {
  return {
    getBoundingClientRect: () => get()?.getBoundingClientRect() ?? new DOMRect(),
    get contextElement() {
      return get() ?? undefined
    },
    focus: (options) => get()?.focus(options),
  }
}

export interface AnchorLink {
  /** `dialog` (Popover) or `menu`. */
  haspopup: `dialog` | `menu`
  /** Toggle on a press (pointer, Enter / Space on a button). */
  toggle: () => void
  /** Open on ArrowDown (menus). */
  open?: () => void
  /** `openOn: hover`: while `active()`, the pointer and keyboard focus
   *  drive it instead of a press (touch keeps the press). */
  hover?: { active: () => boolean; enter: () => void; leave: () => void }
}

/** Wire `el` as the overlay's trigger; returns the cleanup (attributes the
 *  link set are removed). */
export function linkAnchor(el: HTMLElement, link: AnchorLink): () => void {
  el.setAttribute(`aria-haspopup`, link.haspopup)
  el.setAttribute(`data-xui-anchor-for`, ``)
  const hovering = () => link.hover?.active() ?? false
  const click = () => {
    if (!hovering()) link.toggle()
  }
  const key = (e: KeyboardEvent) => {
    if (e.key === `ArrowDown` && link.open && !hovering()) {
      e.preventDefault()
      link.open()
    }
  }
  const enter = (e: PointerEvent) => e.pointerType !== `touch` && hovering() && link.hover?.enter()
  const leave = (e: PointerEvent) => e.pointerType !== `touch` && hovering() && link.hover?.leave()
  const focus = () => hovering() && link.hover?.enter()
  const blur = () => hovering() && link.hover?.leave()
  el.addEventListener(`click`, click)
  el.addEventListener(`keydown`, key)
  if (link.hover) {
    el.addEventListener(`pointerenter`, enter)
    el.addEventListener(`pointerleave`, leave)
    el.addEventListener(`focus`, focus)
    el.addEventListener(`blur`, blur)
  }
  return () => {
    el.removeEventListener(`click`, click)
    el.removeEventListener(`keydown`, key)
    el.removeEventListener(`pointerenter`, enter)
    el.removeEventListener(`pointerleave`, leave)
    el.removeEventListener(`focus`, focus)
    el.removeEventListener(`blur`, blur)
    for (const a of [`aria-haspopup`, `aria-expanded`, `aria-controls`, `data-state`, `data-xui-anchor-for`]) el.removeAttribute(a)
  }
}

/** The anchor's open state on the element (after every open change). */
export function markAnchor(el: HTMLElement, open: boolean, contentId?: string | null): void {
  el.setAttribute(`aria-expanded`, String(open))
  el.setAttribute(`data-state`, open ? `open` : `closed`)
  if (open && contentId) el.setAttribute(`aria-controls`, contentId)
  else el.removeAttribute(`aria-controls`)
}

/** True when `target` is inside the anchor (a press there is the trigger's,
 *  not an outside press). */
export function withinAnchor(el: HTMLElement | null | undefined, target: EventTarget | null): boolean {
  return !!el && target instanceof Node && el.contains(target)
}

/** True when `target` is inside ANOTHER open overlay's content (the core's
 *  outside-press rule: a press or focus landing in another open layer never
 *  dismisses a non-modal popup, so two panel applets can stay open). */
export function withinOtherOverlay(own: Element | null | undefined, target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false
  const layer = target.closest(`[data-xui-overlay]`)
  return !!layer && layer !== own && !own?.contains(layer)
}

/** #EXP-1739 (overlayAnchor, the core's rule): an open anchored overlay
 *  closes once its anchor SCROLLS (a scroll container holding it scrolls),
 *  MOVES (its box's origin changes, checked every animation frame) or GOES
 *  (it leaves the document, or another element replaces it). `get` is read
 *  every frame; `onLost` runs once.
 *  Returns the cleanup (call it when the overlay closes). */
export function watchAnchor(get: () => Element | null | undefined, onLost: () => void): () => void {
  const first = get()
  if (!first || typeof window === `undefined`) return () => {}
  // A web font still loading lays text out in its fallback first: the swap
  // re-lays the page out without anything having moved (the core lays out
  // with its fonts already loaded), so the box the anchor ends up in once the
  // fonts settle is the one it opened at (none yet = re-measure next frame).
  const loading = () => (document as Document & { fonts?: FontFaceSet }).fonts?.status === `loading`
  let opened = loading() ? undefined : first.getBoundingClientRect()
  let done = false
  let frame = 0
  const lose = () => {
    if (done) return
    stop()
    onLost()
  }
  const scrolled = (e: Event) => {
    const el = get()
    const target = e.target
    if (el && (target === document || target === window || (target instanceof Node && target !== el && target.contains(el)))) lose()
  }
  const tick = () => {
    // Gone, or another element took its place (the same place is not the same anchor).
    if (get() !== first || !first.isConnected) return lose()
    const rect = first.getBoundingClientRect()
    opened = loading() ? undefined : (opened ?? rect)
    if (opened && (Math.abs(rect.x - opened.x) >= 0.5 || Math.abs(rect.y - opened.y) >= 0.5)) return lose()
    frame = requestAnimationFrame(tick)
  }
  const stop = () => {
    done = true
    cancelAnimationFrame(frame)
    window.removeEventListener(`scroll`, scrolled, true)
  }
  // Capture only: a scroll is never cancelable, so `passive` changes nothing.
  window.addEventListener(`scroll`, scrolled, true)
  frame = requestAnimationFrame(tick)
  return stop
}

/** Popover `matchAnchorWidth`: the panel is EXACTLY its anchor's width (the
 *  `anchor` node, else the trigger), re-measured by the popper when the
 *  anchor resizes (the core's exact layer width). `triggerWidthVar` = the
 *  popper's anchor-width variable (`--radix-popover-trigger-width`); unset =
 *  the content's own width. */
export function popoverPanelWidth(props: Record<string, unknown>, triggerWidthVar: string): string | undefined {
  return props.matchAnchorWidth === true ? `var(${triggerWidthVar})` : undefined
}
