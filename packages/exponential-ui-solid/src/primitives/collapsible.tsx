import { Show, createContext, createEffect, createSignal, createUniqueId, on, onCleanup, onMount, splitProps, untrack, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"

// Radix Collapsible on plain DOM: the root and the trigger carry
// `data-state`/`data-disabled` (the trigger `aria-controls` +
// `aria-expanded`), the content is always mounted (`hidden` + no children
// while closed and its exit animation has ended) and measures itself on every
// toggle into `--radix-collapsible-content-height/width`. Radix's render
// reads the measurement of the PREVIOUS layout pass; its Presence re-renders
// once after mount (the node lands in its state), which paints the mount
// measurement. A content open at mount keeps `transition-duration: 0s;
// animation-name: none` until its first toggle.

interface CollapsibleState {
  contentId: string
  open: Accessor<boolean>
  disabled: Accessor<boolean>
  toggle: () => void
}
const CollapsibleContext = createContext<CollapsibleState>()

const call = (handler: unknown, e: Event) => {
  if (typeof handler === `function`) handler(e)
  else if (Array.isArray(handler)) handler[0](handler[1], e)
}

export type CollapsibleProps = ComponentProps<`div`> & {
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  disabled?: boolean
}

function Collapsible(props: CollapsibleProps) {
  const [local, rest] = splitProps(props, [`open`, `defaultOpen`, `onOpenChange`, `disabled`])
  const [inner, setInner] = createSignal(untrack(() => local.defaultOpen) ?? false)
  const controlled = () => local.open !== undefined
  const open = () => (controlled() ? Boolean(local.open) : inner())
  const state: CollapsibleState = {
    contentId: createUniqueId(),
    open,
    disabled: () => Boolean(local.disabled),
    toggle: () => {
      const next = !untrack(open)
      if (!untrack(controlled)) setInner(next)
      local.onOpenChange?.(next)
    },
  }
  return (
    <CollapsibleContext.Provider value={state}>
      <div data-state={open() ? `open` : `closed`} data-disabled={local.disabled ? `` : undefined} data-slot="collapsible" {...rest} />
    </CollapsibleContext.Provider>
  )
}

function CollapsibleTrigger(props: ComponentProps<`button`>) {
  const ctx = useContext(CollapsibleContext)!
  const [local, rest] = splitProps(props, [`onClick`])
  return (
    <button
      type="button"
      aria-controls={ctx.contentId}
      aria-expanded={ctx.open() || false}
      data-state={ctx.open() ? `open` : `closed`}
      data-disabled={ctx.disabled() ? `` : undefined}
      disabled={ctx.disabled()}
      data-slot="collapsible-trigger"
      {...rest}
      onClick={(e) => {
        call(local.onClick, e)
        if (!e.defaultPrevented) ctx.toggle()
      }}
    />
  )
}

export type CollapsibleContentProps = Omit<ComponentProps<`div`>, `style`> & { forceMount?: boolean; style?: JSX.CSSProperties }

function CollapsibleContent(props: CollapsibleContentProps) {
  const ctx = useContext(CollapsibleContext)!
  const [local, rest] = splitProps(props, [`forceMount`, `children`, `style`, `ref`])
  const present = () => Boolean(local.forceMount) || ctx.open()
  // Radix Presence + `isPresent`: painted while open, and while closing until
  // the exit animation the closed state starts has ended.
  const [isPresent, setIsPresent] = createSignal(untrack(present))
  const isOpen = () => ctx.open() || isPresent()
  const [dims, setDims] = createSignal({ h: 0, w: 0 })
  let node: HTMLDivElement | undefined
  let original: { transitionDuration: string; animationName: string } | undefined
  let measured = { h: 0, w: 0 }
  let mountPrevented = untrack(isOpen)
  const measure = () => {
    if (!node) return
    original ??= { transitionDuration: node.style.transitionDuration, animationName: node.style.animationName }
    node.style.transitionDuration = `0s`
    node.style.animationName = `none`
    const rect = node.getBoundingClientRect()
    measured = { h: rect.height, w: rect.width }
    if (!mountPrevented) {
      node.style.transitionDuration = original.transitionDuration
      node.style.animationName = original.animationName
    }
  }
  let prevAnimation = `none`
  let stopWaiting: (() => void) | undefined
  const animationName = () => (node ? getComputedStyle(node).animationName || `none` : `none`)
  onMount(() => {
    measure()
    setDims(measured)
    prevAnimation = untrack(present) ? animationName() : `none`
    const onStart = (e: AnimationEvent) => {
      if (e.target === node) prevAnimation = animationName()
    }
    node?.addEventListener(`animationstart`, onStart)
    const raf = requestAnimationFrame(() => (mountPrevented = false))
    onCleanup(() => {
      cancelAnimationFrame(raf)
      node?.removeEventListener(`animationstart`, onStart)
      stopWaiting?.()
    })
  })
  createEffect(
    on(
      present,
      (p) => {
        stopWaiting?.()
        // The toggle's render paints the previous measurement, its layout
        // pass measures again; the next render (Presence settling) paints it.
        measure()
        if (p) {
          setIsPresent(true)
          prevAnimation = animationName()
          setDims(measured)
          return
        }
        const settle = () => {
          measure()
          setIsPresent(false)
          setDims(measured)
        }
        const el = node
        const name = animationName()
        if (!el || name === `none` || getComputedStyle(el).display === `none` || prevAnimation === name) return settle()
        const done = (e: AnimationEvent) => {
          if (e.target !== el || !animationName().includes(CSS.escape(e.animationName))) return
          stopWaiting?.()
          prevAnimation = `none`
          settle()
        }
        el.addEventListener(`animationend`, done)
        el.addEventListener(`animationcancel`, done)
        setDims(measured)
        stopWaiting = () => {
          el.removeEventListener(`animationend`, done)
          el.removeEventListener(`animationcancel`, done)
          stopWaiting = undefined
        }
      },
      { defer: true }
    )
  )
  const style = (): JSX.CSSProperties => {
    const d = dims()
    const out: Record<string, string> = {}
    if (d.h) out[`--radix-collapsible-content-height`] = `${d.h}px`
    if (d.w) out[`--radix-collapsible-content-width`] = `${d.w}px`
    return { ...out, ...(local.style ?? {}) }
  }
  return (
    <div
      data-state={ctx.open() ? `open` : `closed`}
      data-disabled={ctx.disabled() ? `` : undefined}
      id={ctx.contentId}
      hidden={!isOpen()}
      data-slot="collapsible-content"
      {...rest}
      ref={(el) => {
        node = el
        if (typeof local.ref === `function`) (local.ref as (el: HTMLDivElement) => void)(el)
      }}
      style={style()}
    >
      <Show when={isOpen()}>{local.children}</Show>
    </div>
  )
}

export { Collapsible, CollapsibleTrigger, CollapsibleContent }
