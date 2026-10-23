// FEED-131: Radix Popover's DOM painted by hand for the date and time
// pickers (React's PopoverPrimitive.Portal + Content, align start,
// `OVERLAY_OFFSET`, `OVERLAY_PADDING`): the portalled
// `data-radix-popper-content-wrapper` (imperative style writes), the
// content's `data-side`/`data-align`/`tabindex=-1`/`--radix-popover-*`
// variables, non-modal dismissal (outside pointer/focus, Escape), the Tab
// loop, the focus return and Radix `Presence`. The overlays' `Popper` +
// `createLayer` + `createFocusScope` are the general machinery; this one is
// kept as the pickers were ported (its own `tabbables` selector).

import { Show, createEffect, createSignal, on, onCleanup, onMount, untrack, type JSX } from "solid-js"
import { Portal } from "solid-js/web"
import { autoUpdate, computePosition, flip, limitShift, offset, shift, size, type Placement } from "@floating-ui/dom"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential_app/ui"
import { transformOrigin } from "./popper"

const TABBABLE = `button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),a[href],[tabindex]`

function tabbables(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(TABBABLE)).filter((el) => el.tabIndex >= 0 && !el.closest(`[hidden],[inert]`))
}

export interface PickerPopoverProps {
  open: boolean
  setOpen: (open: boolean) => void
  trigger: () => HTMLElement | undefined
  contentId: string
  /** The content's own attributes (the part, role, aria-label, overlay…). */
  attrs: Record<string, unknown>
  /** Radix's `onOpenAutoFocus` (the default prevented): where focus goes. */
  onOpenAutoFocus: (content: HTMLElement) => void
  onPointerOver?: (e: PointerEvent) => void
  onFocusIn?: (e: FocusEvent) => void
  /** The surface's overlay layer (the portal's mount) and reading direction. */
  portal: HTMLElement | null
  dir: `ltr` | `rtl`
  children: JSX.Element
}

/** PopoverPrimitive.Portal + Content (align start, `OVERLAY_OFFSET`,
 *  `OVERLAY_PADDING`), non-modal, inside Radix's `Presence`: a close keeps
 *  the content (`data-state="closed"`) mounted while its exit animation
 *  runs, then unmounts it. */
export function PickerPopover(props: PickerPopoverProps): JSX.Element {
  const [present, setPresent] = createSignal(untrack(() => props.open))
  let node: HTMLElement | undefined
  let prevAnimation = `none`
  const animationName = () => (node ? getComputedStyle(node).animationName || `none` : `none`)
  createEffect(
    on(
      () => props.open,
      (open, was) => {
        if (open) {
          setPresent(true)
          return
        }
        if (!node || !was) {
          setPresent(false)
          return
        }
        const current = animationName()
        if (current === `none` || getComputedStyle(node).display === `none` || prevAnimation === current) setPresent(false)
      },
      { defer: true }
    )
  )
  const presence: PresenceHooks = {
    mount: (el) => {
      node = el
      prevAnimation = animationName()
    },
    unmount: () => {
      node = undefined
      prevAnimation = `none`
    },
    start: (e) => {
      if (e.target === node) prevAnimation = animationName()
    },
    end: (e) => {
      if (!node || e.target !== node || !animationName().includes(CSS.escape(e.animationName)) || untrack(() => props.open)) return
      const fill = node.style.animationFillMode
      const el = node
      el.style.animationFillMode = `forwards`
      setTimeout(() => {
        if (el.style.animationFillMode === `forwards`) el.style.animationFillMode = fill
      })
      setPresent(false)
    },
  }
  return (
    <Show when={present()}>
      <PopoverContent {...props} presence={presence} />
    </Show>
  )
}

interface PresenceHooks {
  mount: (el: HTMLElement) => void
  unmount: () => void
  start: (e: AnimationEvent) => void
  end: (e: AnimationEvent) => void
}

function PopoverContent(props: PickerPopoverProps & { presence: PresenceHooks }): JSX.Element {
  const [placement, setPlacement] = createSignal<Placement>(`bottom-start`)
  const [positioned, setPositioned] = createSignal(false)
  const [wrapper, setWrapper] = createSignal<HTMLDivElement>()
  let content: HTMLDivElement | undefined
  let interactedOutside = false
  let pointerDownOutside = false
  const side = () => placement().split(`-`)[0]
  const align = () => placement().split(`-`)[1] ?? `center`
  const mountWrapper = (el: HTMLDivElement) => {
    // PopperContent's wrapper: `floatingStyles` before the first placement
    // (`translate(0, -200%)` keeps it off the page while measuring).
    el.setAttribute(`data-radix-popper-content-wrapper`, ``)
    el.style.position = `fixed`
    el.style.left = `0px`
    el.style.top = `0px`
    el.style.transform = `translate(0, -200%)`
    el.style.minWidth = `max-content`
    setWrapper(el)
  }
  createEffect(() => wrapper()?.setAttribute(`dir`, props.dir))
  // Radix Popover's non-modal `onInteractOutside`.
  const interactOutside = (target: EventTarget | null, kind: `pointerdown` | `focusin`): boolean => {
    interactedOutside = true
    if (kind === `pointerdown`) pointerDownOutside = true
    const t = props.trigger()
    if (t && target instanceof Node && t.contains(target)) return false
    if (kind === `focusin` && pointerDownOutside) return false
    return true
  }
  const inside = (target: EventTarget | null) => target instanceof Node && Boolean(content?.contains(target))
  onMount(() => {
    const floating = wrapper()
    const anchor = props.trigger()
    if (!floating || !anchor || !content) return
    const z = getComputedStyle(content).zIndex
    if (z) floating.style.zIndex = z
    let live = true
    const detect = { padding: OVERLAY_PADDING, boundary: [] as Element[], altBoundary: false }
    const update = () =>
      void computePosition(anchor, floating, {
        strategy: `fixed`,
        placement: `bottom-start`,
        middleware: [
          offset({ mainAxis: OVERLAY_OFFSET, alignmentAxis: 0 }),
          shift({ mainAxis: true, crossAxis: false, limiter: limitShift(), ...detect }),
          flip({ ...detect }),
          size({
            ...detect,
            apply: ({ elements, rects, availableWidth, availableHeight }) => {
              const s = elements.floating.style
              s.setProperty(`--radix-popper-available-width`, `${availableWidth}px`)
              s.setProperty(`--radix-popper-available-height`, `${availableHeight}px`)
              s.setProperty(`--radix-popper-anchor-width`, `${rects.reference.width}px`)
              s.setProperty(`--radix-popper-anchor-height`, `${rects.reference.height}px`)
            },
          }),
          transformOrigin,
        ],
      }).then(({ x, y, placement: placed, middlewareData }) => {
        if (!live) return
        const dpr = window.devicePixelRatio || 1
        const round = (v: number) => Math.round(v * dpr) / dpr
        floating.style.transform = `translate(${round(x)}px, ${round(y)}px)`
        if (dpr >= 1.5) floating.style.willChange = `transform`
        const origin = middlewareData.transformOrigin as { x: string; y: string } | undefined
        floating.style.setProperty(`--radix-popper-transform-origin`, [origin?.x, origin?.y].join(` `))
        setPlacement(placed)
        setPositioned(true)
      })
    const stop = autoUpdate(anchor, floating, update)
    const doc = content.ownerDocument
    const onPointerDown = (e: PointerEvent) => {
      if (inside(e.target)) return
      if (interactOutside(e.target, `pointerdown`)) props.setOpen(false)
    }
    const onFocusIn = (e: FocusEvent) => {
      if (inside(e.target)) return
      if (interactOutside(e.target, `focusin`)) props.setOpen(false)
    }
    const onEscape = (e: KeyboardEvent) => {
      if (e.key !== `Escape` || !props.open) return
      e.stopPropagation()
      props.setOpen(false)
    }
    // Radix registers the outside pointer listener a tick late (the opening
    // click is not an outside one).
    const timer = setTimeout(() => doc.addEventListener(`pointerdown`, onPointerDown))
    doc.addEventListener(`focusin`, onFocusIn)
    doc.addEventListener(`keydown`, onEscape, { capture: true })
    props.presence.mount(content)
    props.onOpenAutoFocus(content)
    onCleanup(() => {
      props.presence.unmount()
      live = false
      stop()
      clearTimeout(timer)
      doc.removeEventListener(`pointerdown`, onPointerDown)
      doc.removeEventListener(`focusin`, onFocusIn)
      doc.removeEventListener(`keydown`, onEscape, { capture: true })
      // FocusScope's unmount auto-focus (a tick late): back to the trigger
      // unless the person interacted outside.
      const back = !interactedOutside
      const t = props.trigger()
      setTimeout(() => {
        if (back) t?.focus()
      })
    })
  })
  // FocusScope `loop` (not trapped): Tab wraps inside the content.
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== `Tab` || e.altKey || e.ctrlKey || e.metaKey || !content) return
    const focused = document.activeElement as HTMLElement | null
    const all = tabbables(content)
    if (!all.length) {
      if (focused === content) e.preventDefault()
      return
    }
    const first = all[0]!
    const last = all[all.length - 1]!
    if (!e.shiftKey && focused === last) {
      e.preventDefault()
      first.focus()
    } else if (e.shiftKey && focused === first) {
      e.preventDefault()
      last.focus()
    }
  }
  const style = (): JSX.CSSProperties => ({
    "--radix-popover-content-transform-origin": `var(--radix-popper-transform-origin)`,
    "--radix-popover-content-available-width": `var(--radix-popper-available-width)`,
    "--radix-popover-content-available-height": `var(--radix-popper-available-height)`,
    "--radix-popover-trigger-width": `var(--radix-popper-anchor-width)`,
    "--radix-popover-trigger-height": `var(--radix-popper-anchor-height)`,
    ...(positioned() ? {} : { animation: `none` }),
  })
  return (
    <Portal mount={props.portal ?? undefined} ref={mountWrapper}>
      <div
        ref={content}
        data-side={side()}
        data-align={align()}
        role="dialog"
        id={props.contentId}
        data-state={props.open ? `open` : `closed`}
        {...props.attrs}
        tabindex="-1"
        style={style()}
        onKeyDown={onKeyDown}
        onPointerOver={(e) => props.onPointerOver?.(e)}
        onFocusIn={(e) => props.onFocusIn?.(e)}
        onAnimationStart={(e) => props.presence.start(e)}
        onAnimationEnd={(e) => props.presence.end(e)}
        onAnimationCancel={(e) => props.presence.end(e)}
      >
        {props.children}
      </div>
    </Portal>
  )
}
