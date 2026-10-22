import { Show, createContext, createSignal, onCleanup, onMount, splitProps, untrack, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { TOOLTIP_CLASSES } from "@exponential-at/ui/web/primitives"

import { Popper, VisuallyHidden, createLayer, createPresence, graceArea, inPolygon, popperContentStyle, type Align, type OutsideEvent, type Point, type Side } from "../behaviors"
import { DialogPortalPart, layerMountedThisTask, layerPointerEvents, noteLayerMount, callRef, composeHandlers, createId, createOpenState, mergeRefs, styleObject, type AsProps } from "../behaviors/dialog"
import { bindStyle } from "../behaviors/style-writer"
import { cn } from "./cn"

// Radix Tooltip on plain elements: the provider's delay + skip-delay, the
// trigger (hover after `delayDuration`, focus at once, a press closes), the
// portalled content in the popper wrapper with its VisuallyHidden
// `role="tooltip"` copy, the hoverable grace area between the two, another
// tooltip opening or the trigger scrolling away closes it.

const TOOLTIP_OPEN = `tooltip.open`

interface ProviderValue {
  isOpenDelayed: () => boolean
  delayDuration: number
  onOpen: () => void
  onClose: () => void
  inTransit: () => boolean
  setInTransit: (v: boolean) => void
  disableHoverableContent: boolean
}

function createProvider(o: { delayDuration?: number; skipDelayDuration?: number; disableHoverableContent?: boolean }): ProviderValue {
  let openDelayed = true
  let inTransit = false
  let skipTimer: ReturnType<typeof setTimeout> | undefined
  onCleanup(() => clearTimeout(skipTimer))
  return {
    isOpenDelayed: () => openDelayed,
    delayDuration: o.delayDuration ?? 700,
    onOpen: () => {
      clearTimeout(skipTimer)
      openDelayed = false
    },
    onClose: () => {
      clearTimeout(skipTimer)
      skipTimer = setTimeout(() => (openDelayed = true), o.skipDelayDuration ?? 300)
    },
    inTransit: () => inTransit,
    setInTransit: (v) => (inTransit = v),
    disableHoverableContent: o.disableHoverableContent ?? false,
  }
}

const ProviderContext = createContext<ProviderValue>()

export interface TooltipProviderProps {
  delayDuration?: number
  skipDelayDuration?: number
  disableHoverableContent?: boolean
  children?: JSX.Element
}

function TooltipProvider(props: TooltipProviderProps) {
  const value = createProvider({ delayDuration: untrack(() => props.delayDuration) ?? 0, skipDelayDuration: untrack(() => props.skipDelayDuration), disableHoverableContent: untrack(() => props.disableHoverableContent) })
  return <ProviderContext.Provider value={value}>{props.children}</ProviderContext.Provider>
}

interface TooltipContextValue {
  contentId: string
  open: Accessor<boolean>
  stateAttribute: Accessor<string>
  trigger: Accessor<HTMLElement | undefined>
  setTrigger: (el: HTMLElement | undefined) => void
  onTriggerEnter: () => void
  onTriggerLeave: () => void
  onOpen: () => void
  onClose: () => void
  disableHoverableContent: boolean
  provider: ProviderValue
}
const TooltipContext = createContext<TooltipContextValue>()
const useTooltip = (part: string) => {
  const ctx = useContext(TooltipContext)
  if (!ctx) throw new Error(`\`${part}\` must be used within \`Tooltip\``)
  return ctx
}

export interface TooltipProps {
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  delayDuration?: number
  disableHoverableContent?: boolean
  children?: JSX.Element
}

function Tooltip(props: TooltipProps) {
  // Radix needs a TooltipProvider; without one this tooltip has its own.
  const provider = useContext(ProviderContext) ?? createProvider({})
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const [wasDelayed, setWasDelayed] = createSignal(false)
  let openTimer: ReturnType<typeof setTimeout> | undefined
  const disableHoverableContent = untrack(() => props.disableHoverableContent) ?? provider.disableHoverableContent
  const delayDuration = () => props.delayDuration ?? provider.delayDuration
  const [open, setOpenState] = createOpenState({
    open: () => props.open,
    defaultOpen: props.defaultOpen,
    onOpenChange: () => (next: boolean) => {
      if (next) {
        provider.onOpen()
        document.dispatchEvent(new CustomEvent(TOOLTIP_OPEN))
      } else provider.onClose()
      props.onOpenChange?.(next)
    },
  })
  const handleOpen = () => {
    clearTimeout(openTimer)
    openTimer = undefined
    setWasDelayed(false)
    setOpenState(true)
  }
  const handleClose = () => {
    clearTimeout(openTimer)
    openTimer = undefined
    setOpenState(false)
  }
  const handleDelayedOpen = () => {
    clearTimeout(openTimer)
    openTimer = setTimeout(() => {
      setWasDelayed(true)
      setOpenState(true)
      openTimer = undefined
    }, untrack(delayDuration))
  }
  onCleanup(() => clearTimeout(openTimer))
  const value: TooltipContextValue = {
    contentId: `radix-${createId()}`,
    open,
    stateAttribute: () => (open() ? (wasDelayed() ? `delayed-open` : `instant-open`) : `closed`),
    trigger,
    setTrigger,
    onTriggerEnter: () => (provider.isOpenDelayed() ? handleDelayedOpen() : handleOpen()),
    onTriggerLeave: () => {
      if (disableHoverableContent) handleClose()
      else {
        clearTimeout(openTimer)
        openTimer = undefined
      }
    },
    onOpen: handleOpen,
    onClose: handleClose,
    disableHoverableContent,
    provider,
  }
  return <TooltipContext.Provider value={value}>{props.children}</TooltipContext.Provider>
}

function TooltipTrigger(props: AsProps) {
  const ctx = useTooltip(`TooltipTrigger`)
  const [local, rest] = splitProps(props, [`as`, `ref`, `onPointerMove`, `onPointerLeave`, `onPointerDown`, `onFocus`, `onBlur`, `onClick`])
  let pointerDown = false
  let pointerMoveOpened = false
  const onPointerUp = () => (pointerDown = false)
  onCleanup(() => document.removeEventListener(`pointerup`, onPointerUp))
  return (
    <Dynamic
      component={local.as ?? `button`}
      data-slot="tooltip-trigger"
      aria-describedby={ctx.open() ? ctx.contentId : undefined}
      data-state={ctx.stateAttribute()}
      {...rest}
      ref={mergeRefs(local.ref, ctx.setTrigger)}
      onPointerMove={composeHandlers(local.onPointerMove, (e: PointerEvent) => {
        if (e.pointerType === `touch`) return
        if (!pointerMoveOpened && !ctx.provider.inTransit()) {
          ctx.onTriggerEnter()
          pointerMoveOpened = true
        }
      })}
      onPointerLeave={composeHandlers(local.onPointerLeave, () => {
        ctx.onTriggerLeave()
        pointerMoveOpened = false
      })}
      onPointerDown={composeHandlers(local.onPointerDown, () => {
        if (untrack(ctx.open)) ctx.onClose()
        pointerDown = true
        document.addEventListener(`pointerup`, onPointerUp, { once: true })
      })}
      onFocus={composeHandlers(local.onFocus, () => {
        if (!pointerDown) ctx.onOpen()
      })}
      onBlur={composeHandlers(local.onBlur, () => ctx.onClose())}
      onClick={composeHandlers(local.onClick, () => ctx.onClose())}
    />
  )
}

export type TooltipContentProps = Omit<ComponentProps<`div`>, `style`> & {
  side?: Side
  align?: Align
  sideOffset?: number
  alignOffset?: number
  collisionPadding?: number
  container?: HTMLElement | null
  style?: JSX.CSSProperties
  "aria-label"?: string
  onEscapeKeyDown?: (e: KeyboardEvent) => void
  onPointerDownOutside?: (e: OutsideEvent) => void
}

function TooltipContent(props: TooltipContentProps) {
  const ctx = useTooltip(`TooltipContent`)
  const [local, rest] = splitProps(props, [`class`, `side`, `align`, `sideOffset`, `alignOffset`, `collisionPadding`, `container`, `style`, `ref`, `children`, `aria-label`, `onEscapeKeyDown`, `onPointerDownOutside`])
  const presence = createPresence(ctx.open)
  const Content = () => {
    const layer = createLayer({
      onEscapeKeyDown: (e) => local.onEscapeKeyDown?.(e),
      onPointerDownOutside: (e) => local.onPointerDownOutside?.(e),
      onFocusOutside: (e) => e.preventDefault(),
      onDismiss: () => ctx.onClose(),
    })
    const [registered, setRegistered] = createSignal(false)
    const lag = layerMountedThisTask()
    let contentEl: HTMLElement | undefined
    // The hoverable content: leaving the trigger toward the content (or back)
    // keeps it open while the pointer stays in the grace area.
    let grace: Point[] | null = null
    let graceTimer: ReturnType<typeof setTimeout> | undefined
    const removeGrace = () => {
      clearTimeout(graceTimer)
      grace = null
      ctx.provider.setInTransit(false)
    }
    // Radix tracks the grace area from an effect after the leave: the
    // pointer move that caused the leave itself is never judged.
    const createGrace = (e: PointerEvent, from: HTMLElement, to: HTMLElement) => {
      const area = graceArea({ x: e.clientX, y: e.clientY }, from.getBoundingClientRect(), to.getBoundingClientRect())
      ctx.provider.setInTransit(true)
      clearTimeout(graceTimer)
      graceTimer = setTimeout(() => (grace = area), 0)
    }
    const track = (e: PointerEvent) => {
      if (!grace) return
      const target = e.target as Node | null
      const entered = Boolean(ctx.trigger()?.contains(target) || contentEl?.contains(target))
      if (entered) removeGrace()
      else if (!inPolygon({ x: e.clientX, y: e.clientY }, grace)) {
        removeGrace()
        ctx.onClose()
      }
    }
    onCleanup(removeGrace)
    return (
      <Popper anchor={ctx.trigger()} side={local.side ?? `top`} align={local.align ?? `center`} sideOffset={local.sideOffset ?? 0} alignOffset={local.alignOffset} collisionPadding={local.collisionPadding ?? 0}>
        {(popper) => {
          const el = (
            <div data-side={popper.placedSide()} data-align={popper.placedAlign()} data-state={ctx.stateAttribute()} data-slot="tooltip-content" class={cn(TOOLTIP_CLASSES.content, local.class)} {...rest}>
              {local.children}
              <VisuallyHidden id={ctx.contentId}>{local[`aria-label`] || local.children}</VisuallyHidden>
            </div>
          ) as HTMLDivElement
          contentEl = el
          presence.ref(el)
          popper.setContent(el)
          bindStyle(el, () => popperContentStyle(`tooltip`, `trigger`, layerPointerEvents(layer, registered(), lag), popper.isPositioned(), styleObject(local.style)))
          callRef(local.ref, el)
          onMount(() => {
            layer.attach(el)
            setRegistered(true)
            noteLayerMount()
            const close = () => ctx.onClose()
            document.addEventListener(TOOLTIP_OPEN, close)
            const onScroll = (e: Event) => {
              const target = e.target as Node | null
              const t = ctx.trigger()
              if (t && target?.contains?.(t)) ctx.onClose()
            }
            window.addEventListener(`scroll`, onScroll, { capture: true })
            const t = ctx.trigger()
            const onTriggerLeave = (e: PointerEvent) => t && createGrace(e, t, el)
            const onContentLeave = (e: PointerEvent) => t && createGrace(e, el, t)
            if (!ctx.disableHoverableContent && t) {
              t.addEventListener(`pointerleave`, onTriggerLeave)
              el.addEventListener(`pointerleave`, onContentLeave)
              document.addEventListener(`pointermove`, track)
            }
            onCleanup(() => {
              document.removeEventListener(TOOLTIP_OPEN, close)
              window.removeEventListener(`scroll`, onScroll, { capture: true })
              t?.removeEventListener(`pointerleave`, onTriggerLeave)
              el.removeEventListener(`pointerleave`, onContentLeave)
              document.removeEventListener(`pointermove`, track)
            })
          })
          return el
        }}
      </Popper>
    )
  }
  return (
    <DialogPortalPart container={local.container}>
      <Show when={presence.mounted()}>
        <Content />
      </Show>
    </DialogPortalPart>
  )
}

export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider }
