import { Show, createContext, createEffect, createSignal, onCleanup, onMount, splitProps, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { HOVER_CARD_CLASSES, HOVER_CARD_DELAYS } from "@exponential_app/ui/web/primitives"

import { LayerContext, Popper, createLayer, createPresence, popperContentStyle, type Align, type OutsideEvent, type Side } from "../behaviors"
import { DialogPortalPart, layerMountedThisTask, layerPointerEvents, noteLayerMount, callRef, composeHandlers, createOpenState, mergeRefs, styleObject, type AsProps } from "../behaviors/dialog"
import { bindStyle } from "../behaviors/style-writer"
import { cn } from "./cn"

// Radix HoverCard on plain elements: the trigger (`<a>`) opens after
// `openDelay` on a mouse-like pointer or on focus, closes `closeDelay` after
// the pointer leaves both; the portalled content in the popper wrapper, its
// tabbables out of the tab order, a text selection inside keeps it open.

interface HoverCardContextValue {
  open: Accessor<boolean>
  setOpen: (open: boolean) => void
  onOpen: () => void
  onClose: () => void
  trigger: Accessor<HTMLElement | undefined>
  setTrigger: (el: HTMLElement | undefined) => void
  flags: { hasSelection: boolean; pointerDownOnContent: boolean }
}
const HoverCardContext = createContext<HoverCardContextValue>()
const useHoverCard = (part: string) => {
  const ctx = useContext(HoverCardContext)
  if (!ctx) throw new Error(`\`${part}\` must be used within \`HoverCard\``)
  return ctx
}

export interface HoverCardProps {
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  openDelay?: number
  closeDelay?: number
  children?: JSX.Element
}

const excludeTouch = (fn: () => void) => (e: PointerEvent) => (e.pointerType === `touch` ? undefined : fn())

function HoverCard(props: HoverCardProps) {
  const [open, setOpen] = createOpenState({ open: () => props.open, defaultOpen: props.defaultOpen, onOpenChange: () => props.onOpenChange })
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const flags = { hasSelection: false, pointerDownOnContent: false }
  let openTimer: ReturnType<typeof setTimeout> | undefined
  let closeTimer: ReturnType<typeof setTimeout> | undefined
  onCleanup(() => {
    clearTimeout(openTimer)
    clearTimeout(closeTimer)
  })
  const value: HoverCardContextValue = {
    open,
    setOpen,
    onOpen: () => {
      clearTimeout(closeTimer)
      openTimer = setTimeout(() => setOpen(true), props.openDelay ?? HOVER_CARD_DELAYS.openDelay)
    },
    onClose: () => {
      clearTimeout(openTimer)
      if (!flags.hasSelection && !flags.pointerDownOnContent) closeTimer = setTimeout(() => setOpen(false), props.closeDelay ?? HOVER_CARD_DELAYS.closeDelay)
    },
    trigger,
    setTrigger,
    flags,
  }
  return <HoverCardContext.Provider value={value}>{props.children}</HoverCardContext.Provider>
}

function HoverCardTrigger(props: AsProps) {
  const ctx = useHoverCard(`HoverCardTrigger`)
  const [local, rest] = splitProps(props, [`as`, `ref`, `onPointerEnter`, `onPointerLeave`, `onFocus`, `onBlur`, `onTouchStart`])
  return (
    <Dynamic
      component={local.as ?? `a`}
      data-slot="hover-card-trigger"
      data-state={ctx.open() ? `open` : `closed`}
      {...rest}
      ref={mergeRefs(local.ref, ctx.setTrigger)}
      onPointerEnter={composeHandlers(local.onPointerEnter, excludeTouch(ctx.onOpen))}
      onPointerLeave={composeHandlers(local.onPointerLeave, excludeTouch(ctx.onClose))}
      onFocus={composeHandlers(local.onFocus, ctx.onOpen)}
      onBlur={composeHandlers(local.onBlur, ctx.onClose)}
      onTouchStart={composeHandlers(local.onTouchStart, (e: TouchEvent) => e.preventDefault())}
    />
  )
}

export type HoverCardContentProps = Omit<ComponentProps<`div`>, `style`> & {
  side?: Side
  align?: Align
  sideOffset?: number
  alignOffset?: number
  collisionPadding?: number
  container?: HTMLElement | null
  style?: JSX.CSSProperties
  onEscapeKeyDown?: (e: KeyboardEvent) => void
  onPointerDownOutside?: (e: OutsideEvent) => void
  onFocusOutside?: (e: OutsideEvent) => void
  onInteractOutside?: (e: OutsideEvent) => void
}

function HoverCardContent(props: HoverCardContentProps) {
  const ctx = useHoverCard(`HoverCardContent`)
  const [local, rest] = splitProps(props, [`class`, `side`, `align`, `sideOffset`, `alignOffset`, `collisionPadding`, `container`, `style`, `ref`, `children`, `onPointerEnter`, `onPointerLeave`, `onPointerDown`, `onEscapeKeyDown`, `onPointerDownOutside`, `onFocusOutside`, `onInteractOutside`])
  const presence = createPresence(ctx.open)
  const Content = () => {
    const layer = createLayer({
      onEscapeKeyDown: (e) => local.onEscapeKeyDown?.(e),
      onPointerDownOutside: (e) => local.onPointerDownOutside?.(e),
      onFocusOutside: (e) => {
        local.onFocusOutside?.(e)
        if (!e.defaultPrevented) e.preventDefault()
      },
      onInteractOutside: (e) => local.onInteractOutside?.(e),
      onDismiss: () => ctx.setOpen(false),
    })
    const [registered, setRegistered] = createSignal(false)
    const lag = layerMountedThisTask()
    const [containSelection, setContainSelection] = createSignal(false)
    createEffect(() => {
      if (!containSelection()) return
      const body = document.body
      const original = body.style.userSelect || body.style.webkitUserSelect
      body.style.userSelect = `none`
      body.style.webkitUserSelect = `none`
      onCleanup(() => {
        body.style.userSelect = original
        body.style.webkitUserSelect = original
      })
    })
    return (
      <Popper anchor={ctx.trigger()} side={local.side ?? `bottom`} align={local.align ?? `start`} sideOffset={local.sideOffset ?? 6} alignOffset={local.alignOffset} collisionPadding={local.collisionPadding ?? 0}>
        {(popper) => {
          const el = (
            <div
              data-side={popper.placedSide()}
              data-align={popper.placedAlign()}
              data-state={ctx.open() ? `open` : `closed`}
              data-slot="hover-card-content"
              class={cn(HOVER_CARD_CLASSES.content, local.class)}
              {...rest}
              onPointerEnter={composeHandlers(local.onPointerEnter, excludeTouch(ctx.onOpen))}
              onPointerLeave={composeHandlers(local.onPointerLeave, excludeTouch(ctx.onClose))}
              onPointerDown={composeHandlers(local.onPointerDown, (e: PointerEvent) => {
                if ((e.currentTarget as HTMLElement).contains(e.target as Node)) setContainSelection(true)
                ctx.flags.hasSelection = false
                ctx.flags.pointerDownOnContent = true
              })}
            >
              <LayerContext.Provider value={layer.layer}>{local.children}</LayerContext.Provider>
            </div>
          ) as HTMLDivElement
          presence.ref(el)
          popper.setContent(el)
          bindStyle(el, () => {
            const sel = containSelection() ? `text` : undefined
            return popperContentStyle(`hover-card`, `trigger`, layerPointerEvents(layer, registered(), lag), popper.isPositioned(), { ...styleObject(local.style), "user-select": sel, "-webkit-user-select": sel })
          })
          callRef(local.ref, el)
          onMount(() => {
            layer.attach(el)
            setRegistered(true)
            noteLayerMount()
            const onPointerUp = () => {
              setContainSelection(false)
              ctx.flags.pointerDownOnContent = false
              setTimeout(() => {
                if (document.getSelection()?.toString() !== ``) ctx.flags.hasSelection = true
              })
            }
            document.addEventListener(`pointerup`, onPointerUp)
            onCleanup(() => {
              document.removeEventListener(`pointerup`, onPointerUp)
              ctx.flags.hasSelection = false
              ctx.flags.pointerDownOnContent = false
            })
            // The card is not in the tab order (Radix, after every render).
            const untab = () => {
              const walker = document.createTreeWalker(el, NodeFilter.SHOW_ELEMENT, { acceptNode: (n) => ((n as HTMLElement).tabIndex >= 0 ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP) })
              const nodes: HTMLElement[] = []
              while (walker.nextNode()) nodes.push(walker.currentNode as HTMLElement)
              for (const n of nodes) n.setAttribute(`tabindex`, `-1`)
            }
            untab()
            const observer = new MutationObserver(untab)
            observer.observe(el, { childList: true, subtree: true })
            onCleanup(() => observer.disconnect())
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

export { HoverCard, HoverCardTrigger, HoverCardContent }
