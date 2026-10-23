import { Show, createContext, createEffect, createSignal, on, onCleanup, onMount, splitProps, untrack, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"
import {
  SCROLL_AREA_CLASSES,
  SCROLL_AREA_CONTENT_STYLE,
  SCROLL_AREA_SCROLL_INITIAL,
  SCROLL_AREA_SCROLL_MACHINE,
  SCROLL_AREA_THUMB_STYLE,
  SCROLL_AREA_VIEWPORT_CSS,
  scrollAreaCornerStyle,
  scrollAreaRootStyle,
  scrollAreaScrollbarStyle,
  scrollAreaScrollFromPointer,
  scrollAreaThumbOffset,
  scrollAreaThumbRatio,
  scrollAreaThumbSize,
  scrollAreaViewportStyle,
  scrollAreaVisibility,
  type ScrollAreaDir,
  type ScrollAreaOrientation,
  type ScrollAreaScrollState,
  type ScrollAreaSizes,
} from "@exponential_app/ui/web/primitives"

import { Dynamic } from "solid-js/web"
import { createPresence } from "../behaviors"
import { cn } from "./cn"

// Radix ScrollArea on plain DOM: the root (`position: relative` + the corner
// variables), the viewport (Radix's hide-the-native-bar <style>, `overflow`
// per enabled scrollbar, the `display: table` sizer), the scrollbars by
// `type` (`hover` default: shown while the pointer is over the root, hidden
// `scrollHideDelay` ms after it leaves; `scroll`, `auto`, `always`), each
// sizing its thumb from the viewport/content ratio (min 18px) and moving it
// with `translate3d` as the viewport scrolls, the corner when both bars show.
// The timings are Radix's (ResizeObserver → a frame → a 10 ms debounce).

type Type = `auto` | `always` | `scroll` | `hover`
type Dir = ScrollAreaDir
type Orientation = ScrollAreaOrientation
type Sizes = ScrollAreaSizes

interface AreaState {
  type: Accessor<Type>
  dir: Accessor<Dir>
  scrollHideDelay: Accessor<number>
  scrollArea: Accessor<HTMLDivElement | undefined>
  viewport: Accessor<HTMLDivElement | undefined>
  setViewport: (el: HTMLDivElement) => void
  content: Accessor<HTMLDivElement | undefined>
  setContent: (el: HTMLDivElement) => void
  scrollbarX: Accessor<HTMLDivElement | undefined>
  setScrollbarX: (el: HTMLDivElement | undefined) => void
  scrollbarY: Accessor<HTMLDivElement | undefined>
  setScrollbarY: (el: HTMLDivElement | undefined) => void
  xEnabled: Accessor<boolean>
  setXEnabled: (v: boolean) => void
  yEnabled: Accessor<boolean>
  setYEnabled: (v: boolean) => void
  setCornerWidth: (v: number) => void
  setCornerHeight: (v: number) => void
}
const AreaContext = createContext<AreaState>()

const call = (handler: unknown, e: Event) => {
  if (typeof handler === `function`) handler(e)
  else if (Array.isArray(handler)) handler[0](handler[1], e)
}

/** A state update the way React schedules one made outside its own event
 *  system (Radix's native listeners and timers): a scheduler task, so a
 *  pointerenter that lands right before a frame renders after it. */
function reactTask(fn: () => void) {
  const channel = new MessageChannel()
  channel.port1.onmessage = () => {
    channel.port1.close()
    fn()
  }
  channel.port2.postMessage(null)
}

/** Radix's `useDebounceCallback`. */
function debounced(fn: () => void, delay: number): () => void {
  let timer = 0
  onCleanup(() => window.clearTimeout(timer))
  return () => {
    window.clearTimeout(timer)
    timer = window.setTimeout(fn, delay)
  }
}

/** Radix's `useResizeObserver`: a resize → the next frame → `fn`. */
function observeResize(el: Accessor<Element | undefined>, fn: () => void) {
  createEffect(() => {
    const node = el()
    if (!node || typeof ResizeObserver === `undefined`) return
    let raf = 0
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf)
      raf = window.requestAnimationFrame(fn)
    })
    ro.observe(node)
    onCleanup(() => {
      window.cancelAnimationFrame(raf)
      ro.unobserve(node)
    })
  })
}

const toInt = (value: string | undefined) => (value ? parseInt(value, 10) : 0)

/** Radix's `addUnlinkedScrollListener`: a frame loop reporting position changes. */
function unlinkedScroll(node: HTMLElement, handler: () => void) {
  let prev = { left: node.scrollLeft, top: node.scrollTop }
  let raf = 0
  const loop = () => {
    const pos = { left: node.scrollLeft, top: node.scrollTop }
    if (prev.left !== pos.left || prev.top !== pos.top) handler()
    prev = pos
    raf = window.requestAnimationFrame(loop)
  }
  loop()
  return () => window.cancelAnimationFrame(raf)
}

export type ScrollAreaProps = Omit<ComponentProps<`div`>, `dir` | `style`> & { type?: Type; dir?: Dir; scrollHideDelay?: number; style?: JSX.CSSProperties }

function ScrollArea(props: ScrollAreaProps) {
  const [local, rest] = splitProps(props, [`class`, `children`, `type`, `dir`, `scrollHideDelay`, `style`, `ref`])
  const [scrollArea, setScrollArea] = createSignal<HTMLDivElement>()
  const [viewport, setViewport] = createSignal<HTMLDivElement>()
  const [content, setContent] = createSignal<HTMLDivElement>()
  const [scrollbarX, setScrollbarX] = createSignal<HTMLDivElement>()
  const [scrollbarY, setScrollbarY] = createSignal<HTMLDivElement>()
  const [xEnabled, setXEnabled] = createSignal(false)
  const [yEnabled, setYEnabled] = createSignal(false)
  const [cornerWidth, setCornerWidth] = createSignal(0)
  const [cornerHeight, setCornerHeight] = createSignal(0)
  const state: AreaState = {
    type: () => local.type ?? `hover`,
    dir: () => local.dir ?? `ltr`,
    scrollHideDelay: () => local.scrollHideDelay ?? 600,
    scrollArea,
    viewport,
    setViewport,
    content,
    setContent,
    scrollbarX,
    setScrollbarX,
    scrollbarY,
    setScrollbarY,
    xEnabled,
    setXEnabled,
    yEnabled,
    setYEnabled,
    setCornerWidth,
    setCornerHeight,
  }
  return (
    <AreaContext.Provider value={state}>
      <div
        dir={state.dir()}
        data-slot="scroll-area"
        class={cn(SCROLL_AREA_CLASSES.root, local.class)}
        {...rest}
        ref={(el) => {
          setScrollArea(el)
          if (typeof local.ref === `function`) (local.ref as (el: HTMLDivElement) => void)(el)
        }}
        style={{ ...scrollAreaRootStyle(cornerWidth(), cornerHeight()), ...(local.style ?? {}) }}
      >
        <ScrollAreaViewport data-slot="scroll-area-viewport" class={SCROLL_AREA_CLASSES.viewport}>
          {local.children}
        </ScrollAreaViewport>
        <ScrollBar />
        <ScrollAreaCorner />
      </div>
    </AreaContext.Provider>
  )
}

function ScrollAreaViewport(props: Omit<ComponentProps<`div`>, `style`> & { style?: JSX.CSSProperties }) {
  const ctx = useContext(AreaContext)!
  const [local, rest] = splitProps(props, [`children`, `style`])
  return (
    <>
      <style>{SCROLL_AREA_VIEWPORT_CSS}</style>
      <div data-radix-scroll-area-viewport="" {...rest} ref={ctx.setViewport} style={{ ...scrollAreaViewportStyle(ctx.xEnabled(), ctx.yEnabled()), ...(local.style ?? {}) }}>
        <div ref={ctx.setContent} style={{ ...SCROLL_AREA_CONTENT_STYLE }}>
          {local.children}
        </div>
      </div>
    </>
  )
}

type ScrollbarProps = Omit<ComponentProps<`div`>, `style`> & { orientation?: Orientation; forceMount?: boolean; style?: JSX.CSSProperties }

function ScrollBar(props: ScrollbarProps) {
  const ctx = useContext(AreaContext)!
  const [local, rest] = splitProps(props, [`class`, `orientation`])
  const orientation = () => local.orientation ?? `vertical`
  // Radix's ScrollAreaScrollbar: the orientation's overflow turns on while mounted.
  onMount(() => {
    const horizontal = orientation() === `horizontal`
    if (horizontal) ctx.setXEnabled(true)
    else ctx.setYEnabled(true)
    onCleanup(() => (horizontal ? ctx.setXEnabled(false) : ctx.setYEnabled(false)))
  })
  const impl = () => ({ hover: ScrollbarHover, scroll: ScrollbarScroll, auto: ScrollbarAuto, always: ScrollbarVisible })[ctx.type()]
  return <Dynamic component={impl()} data-slot="scroll-area-scrollbar" orientation={orientation()} class={cn(SCROLL_AREA_CLASSES.scrollbar, SCROLL_AREA_CLASSES.orientations[orientation()], local.class)} {...rest} />
}

type InnerProps = ScrollbarProps & { "data-state"?: string }

function ScrollbarHover(props: InnerProps) {
  const ctx = useContext(AreaContext)!
  const [local, rest] = splitProps(props, [`forceMount`])
  const [visible, setVisible] = createSignal(false)
  createEffect(() => {
    const area = ctx.scrollArea()
    const delay = ctx.scrollHideDelay()
    if (!area) return
    let hideTimer = 0
    const enter = () => {
      window.clearTimeout(hideTimer)
      reactTask(() => setVisible(true))
    }
    const leave = () => {
      hideTimer = window.setTimeout(() => reactTask(() => setVisible(false)), delay)
    }
    area.addEventListener(`pointerenter`, enter)
    area.addEventListener(`pointerleave`, leave)
    onCleanup(() => {
      window.clearTimeout(hideTimer)
      area.removeEventListener(`pointerenter`, enter)
      area.removeEventListener(`pointerleave`, leave)
    })
  })
  const presence = createPresence(() => Boolean(local.forceMount) || visible())
  return (
    <Show when={presence.mounted()}>
      <ScrollbarAuto data-state={scrollAreaVisibility(visible())} {...rest} presenceRef={presence.ref} />
    </Show>
  )
}

function ScrollbarScroll(props: InnerProps) {
  const ctx = useContext(AreaContext)!
  const [local, rest] = splitProps(props, [`forceMount`, `onPointerEnter`, `onPointerLeave`])
  const [state, setState] = createSignal<ScrollAreaScrollState>(SCROLL_AREA_SCROLL_INITIAL)
  const send = (event: string) => setState((s) => SCROLL_AREA_SCROLL_MACHINE[s][event] ?? s)
  const scrollEnd = debounced(() => send(`SCROLL_END`), 100)
  createEffect(() => {
    if (state() !== `idle`) return
    const timer = window.setTimeout(() => send(`HIDE`), ctx.scrollHideDelay())
    onCleanup(() => window.clearTimeout(timer))
  })
  createEffect(() => {
    const viewport = ctx.viewport()
    if (!viewport) return
    const key = props.orientation === `horizontal` ? `scrollLeft` : `scrollTop`
    let prev = viewport[key]
    const onScroll = () => {
      const pos = viewport[key]
      if (prev !== pos) {
        send(`SCROLL`)
        scrollEnd()
      }
      prev = pos
    }
    viewport.addEventListener(`scroll`, onScroll)
    onCleanup(() => viewport.removeEventListener(`scroll`, onScroll))
  })
  const presence = createPresence(() => Boolean(local.forceMount) || state() !== SCROLL_AREA_SCROLL_INITIAL)
  return (
    <Show when={presence.mounted()}>
      <ScrollbarVisible
        data-state={scrollAreaVisibility(state() !== SCROLL_AREA_SCROLL_INITIAL)}
        {...rest}
        presenceRef={presence.ref}
        onPointerEnter={(e) => {
          call(local.onPointerEnter, e)
          if (!e.defaultPrevented) send(`POINTER_ENTER`)
        }}
        onPointerLeave={(e) => {
          call(local.onPointerLeave, e)
          if (!e.defaultPrevented) send(`POINTER_LEAVE`)
        }}
      />
    </Show>
  )
}

function ScrollbarAuto(props: InnerProps & { presenceRef?: (el: HTMLElement) => void }) {
  const ctx = useContext(AreaContext)!
  const [local, rest] = splitProps(props, [`forceMount`, `presenceRef`])
  const [visible, setVisible] = createSignal(false)
  const resize = debounced(() => {
    const viewport = ctx.viewport()
    if (!viewport) return
    const overflowX = viewport.offsetWidth < viewport.scrollWidth
    const overflowY = viewport.offsetHeight < viewport.scrollHeight
    const next = props.orientation === `horizontal` ? overflowX : overflowY
    reactTask(() => setVisible(next))
  }, 10)
  observeResize(ctx.viewport, resize)
  observeResize(ctx.content, resize)
  const presence = createPresence(() => Boolean(local.forceMount) || visible())
  return (
    <Show when={presence.mounted()}>
      <ScrollbarVisible
        data-state={scrollAreaVisibility(visible())}
        {...rest}
        presenceRef={(el) => {
          presence.ref(el)
          local.presenceRef?.(el)
        }}
      />
    </Show>
  )
}

interface BarState {
  hasThumb: Accessor<boolean>
  setThumb: (el: HTMLDivElement | undefined) => void
  onThumbPositionChange: () => void
  onThumbPointerDown: (pos: { x: number; y: number }) => void
  onThumbPointerUp: () => void
}
const BarContext = createContext<BarState>()

function ScrollbarVisible(props: InnerProps & { presenceRef?: (el: HTMLElement) => void }) {
  const ctx = useContext(AreaContext)!
  const [local, rest] = splitProps(props, [`orientation`, `forceMount`, `presenceRef`, `style`, `onPointerDown`, `onPointerMove`, `onPointerUp`])
  const horizontal = () => local.orientation === `horizontal`
  const [sizes, setSizes] = createSignal<Sizes>({ content: 0, viewport: 0, scrollbar: { size: 0, paddingStart: 0, paddingEnd: 0 } })
  const [scrollbar, setScrollbar] = createSignal<HTMLDivElement>()
  const [computed, setComputed] = createSignal<CSSStyleDeclaration>()
  let thumb: HTMLDivElement | undefined
  let pointerOffset = 0
  let rect: DOMRect | null = null
  let prevUserSelect = ``
  const ratio = () => scrollAreaThumbRatio(sizes().viewport, sizes().content)
  const positionThumb = () => {
    const viewport = ctx.viewport()
    if (!viewport || !thumb) return
    if (horizontal()) thumb.style.transform = `translate3d(${scrollAreaThumbOffset(viewport.scrollLeft, sizes(), ctx.dir())}px, 0, 0)`
    else thumb.style.transform = `translate3d(0, ${scrollAreaThumbOffset(viewport.scrollTop, sizes())}px, 0)`
  }
  const bar: BarState = {
    hasThumb: () => ratio() > 0 && ratio() < 1,
    setThumb: (el) => (thumb = el),
    onThumbPositionChange: positionThumb,
    onThumbPointerDown: (pos) => (pointerOffset = horizontal() ? pos.x : pos.y),
    onThumbPointerUp: () => (pointerOffset = 0),
  }
  const dragScroll = (pos: { x: number; y: number }) => {
    const viewport = ctx.viewport()
    if (!viewport) return
    if (horizontal()) viewport.scrollLeft = scrollAreaScrollFromPointer(pos.x, pointerOffset, sizes(), ctx.dir())
    else viewport.scrollTop = scrollAreaScrollFromPointer(pos.y, pointerOffset, sizes())
  }
  const onResize = () => {
    const el = scrollbar()
    const viewport = ctx.viewport()
    const cs = untrack(computed)
    if (!el || !viewport || !cs) return
    setSizes(
      horizontal()
        ? { content: viewport.scrollWidth, viewport: viewport.offsetWidth, scrollbar: { size: el.clientWidth, paddingStart: toInt(cs.paddingLeft), paddingEnd: toInt(cs.paddingRight) } }
        : { content: viewport.scrollHeight, viewport: viewport.offsetHeight, scrollbar: { size: el.clientHeight, paddingStart: toInt(cs.paddingTop), paddingEnd: toInt(cs.paddingBottom) } }
    )
  }
  const resize = debounced(onResize, 10)
  observeResize(scrollbar, resize)
  observeResize(ctx.content, resize)
  onMount(() => {
    const el = scrollbar()
    if (el) setComputed(getComputedStyle(el))
  })
  createEffect(on(sizes, () => positionThumb()))
  createEffect(() => {
    const el = scrollbar()
    const max = sizes().content - sizes().viewport
    const onWheel = (e: WheelEvent) => {
      if (!el?.contains(e.target as Node)) return
      const viewport = ctx.viewport()
      if (!viewport) return
      const pos = horizontal() ? viewport.scrollLeft + e.deltaX : viewport.scrollTop + e.deltaY
      if (horizontal()) viewport.scrollLeft = pos
      else viewport.scrollTop = pos
      if (pos > 0 && pos < max) e.preventDefault()
    }
    document.addEventListener(`wheel`, onWheel, { passive: false })
    onCleanup(() => document.removeEventListener(`wheel`, onWheel))
  })
  const style = (): JSX.CSSProperties => ({ ...scrollAreaScrollbarStyle(local.orientation ?? `vertical`, ctx.dir(), scrollAreaThumbSize(sizes())), ...(local.style ?? {}) })
  const handleDrag = (e: PointerEvent) => {
    if (rect) dragScroll({ x: e.clientX - rect.left, y: e.clientY - rect.top })
  }
  return (
    <BarContext.Provider value={bar}>
      <div
        data-orientation={local.orientation ?? `vertical`}
        {...rest}
        ref={(el) => {
          setScrollbar(el)
          if (horizontal()) ctx.setScrollbarX(el)
          else ctx.setScrollbarY(el)
          onCleanup(() => (horizontal() ? ctx.setScrollbarX(undefined) : ctx.setScrollbarY(undefined)))
          local.presenceRef?.(el)
        }}
        style={style()}
        onPointerDown={(e) => {
          call(local.onPointerDown, e)
          if (e.defaultPrevented || e.button !== 0) return
          ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
          rect = scrollbar()!.getBoundingClientRect()
          prevUserSelect = document.body.style.webkitUserSelect
          document.body.style.webkitUserSelect = `none`
          const viewport = ctx.viewport()
          if (viewport) viewport.style.scrollBehavior = `auto`
          handleDrag(e)
        }}
        onPointerMove={(e) => {
          call(local.onPointerMove, e)
          if (!e.defaultPrevented) handleDrag(e)
        }}
        onPointerUp={(e) => {
          call(local.onPointerUp, e)
          if (e.defaultPrevented) return
          const target = e.target as HTMLElement
          if (target.hasPointerCapture(e.pointerId)) target.releasePointerCapture(e.pointerId)
          document.body.style.webkitUserSelect = prevUserSelect
          const viewport = ctx.viewport()
          if (viewport) viewport.style.scrollBehavior = ``
          rect = null
        }}
      >
        <ScrollAreaThumb data-slot="scroll-area-thumb" class={SCROLL_AREA_CLASSES.thumb} />
      </div>
    </BarContext.Provider>
  )
}

function ScrollAreaThumb(props: Omit<ComponentProps<`div`>, `style`> & { forceMount?: boolean; style?: JSX.CSSProperties }) {
  const ctx = useContext(AreaContext)!
  const bar = useContext(BarContext)!
  const [local, rest] = splitProps(props, [`forceMount`, `style`, `onPointerUp`])
  const presence = createPresence(() => Boolean(local.forceMount) || bar.hasThumb())
  return (
    <Show when={presence.mounted()}>
      {(() => {
        let removeUnlinked: (() => void) | undefined
        const scrollEnd = debounced(() => {
          removeUnlinked?.()
          removeUnlinked = undefined
        }, 100)
        onCleanup(() => {
          bar.setThumb(undefined)
          removeUnlinked?.()
        })
        onMount(() => {
          const viewport = ctx.viewport()
          if (!viewport) return
          const onScroll = () => {
            scrollEnd()
            if (!removeUnlinked) {
              removeUnlinked = unlinkedScroll(viewport, bar.onThumbPositionChange)
              bar.onThumbPositionChange()
            }
          }
          bar.onThumbPositionChange()
          viewport.addEventListener(`scroll`, onScroll)
          onCleanup(() => viewport.removeEventListener(`scroll`, onScroll))
        })
        return (
          <div
            data-state={scrollAreaVisibility(bar.hasThumb())}
            {...rest}
            ref={(el) => {
              bar.setThumb(el)
              presence.ref(el)
              el.addEventListener(
                `pointerdown`,
                (e) => {
                  if (e.defaultPrevented) return
                  const r = (e.target as HTMLElement).getBoundingClientRect()
                  bar.onThumbPointerDown({ x: e.clientX - r.left, y: e.clientY - r.top })
                },
                { capture: true }
              )
            }}
            style={{ ...SCROLL_AREA_THUMB_STYLE, ...(local.style ?? {}) }}
            onPointerUp={(e) => {
              call(local.onPointerUp, e)
              if (!e.defaultPrevented) bar.onThumbPointerUp()
            }}
          />
        )
      })()}
    </Show>
  )
}

function ScrollAreaCorner(props: Omit<ComponentProps<`div`>, `style`> & { style?: JSX.CSSProperties }) {
  const ctx = useContext(AreaContext)!
  const hasCorner = () => ctx.type() !== `scroll` && Boolean(ctx.scrollbarX() && ctx.scrollbarY())
  return (
    <Show when={hasCorner()}>
      <CornerImpl {...props} />
    </Show>
  )
}

function CornerImpl(props: Omit<ComponentProps<`div`>, `style`> & { style?: JSX.CSSProperties }) {
  const ctx = useContext(AreaContext)!
  const [local, rest] = splitProps(props, [`style`])
  const [width, setWidth] = createSignal(0)
  const [height, setHeight] = createSignal(0)
  observeResize(ctx.scrollbarX, () => {
    const h = ctx.scrollbarX()?.offsetHeight || 0
    ctx.setCornerHeight(h)
    setHeight(h)
  })
  observeResize(ctx.scrollbarY, () => {
    const w = ctx.scrollbarY()?.offsetWidth || 0
    ctx.setCornerWidth(w)
    setWidth(w)
  })
  const style = (): JSX.CSSProperties => ({ ...scrollAreaCornerStyle(width(), height(), ctx.dir()), ...(local.style ?? {}) })
  return (
    <Show when={width() && height()}>
      <div {...rest} style={style()} />
    </Show>
  )
}

export { ScrollArea, ScrollBar }
