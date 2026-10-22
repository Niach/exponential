import { Show, createContext, createSignal, onMount, splitProps, untrack, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { POPOVER_CLASSES } from "@exponential-at/ui/web/primitives"

import { LayerContext, Popper, createFocusScope, createLayer, createPresence, popperContentStyle, useFocusGuards, useModalEffects, type Align, type OutsideEvent, type Side } from "../behaviors"
import { DialogPortalPart, layerMountedThisTask, layerPointerEvents, noteLayerMount, callHandler, callRef, composeHandlers, createId, createOpenState, mergeRefs, stateOf, styleObject, type AsProps } from "../behaviors/dialog"
import { bindStyle } from "../behaviors/style-writer"
import { cn } from "./cn"

// Radix Popover on plain elements: the trigger (the popper anchor unless a
// PopoverAnchor is set), the portalled content in the popper wrapper (the
// dismissable layer, the focus scope with its Tab loop, the focus guards;
// modal = trapped + `hideOthers` + the scroll lock).

interface PopoverContextValue {
  open: Accessor<boolean>
  setOpen: (open: boolean) => void
  modal: boolean
  contentId: string
  trigger: Accessor<HTMLElement | undefined>
  setTrigger: (el: HTMLElement | undefined) => void
  anchor: Accessor<HTMLElement | undefined>
  setAnchor: (el: HTMLElement | undefined) => void
}
const PopoverContext = createContext<PopoverContextValue>()
const usePopover = (part: string) => {
  const ctx = useContext(PopoverContext)
  if (!ctx) throw new Error(`\`${part}\` must be used within \`Popover\``)
  return ctx
}

export interface PopoverProps {
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  modal?: boolean
  children?: JSX.Element
}

function Popover(props: PopoverProps) {
  const [open, setOpen] = createOpenState({ open: () => props.open, defaultOpen: props.defaultOpen, onOpenChange: () => props.onOpenChange })
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const [anchor, setAnchor] = createSignal<HTMLElement>()
  const value: PopoverContextValue = { open, setOpen, modal: untrack(() => props.modal) ?? false, contentId: `radix-${createId()}`, trigger, setTrigger, anchor, setAnchor }
  return <PopoverContext.Provider value={value}>{props.children}</PopoverContext.Provider>
}

function PopoverTrigger(props: AsProps) {
  const ctx = usePopover(`PopoverTrigger`)
  const [local, rest] = splitProps(props, [`as`, `ref`, `onClick`])
  return (
    <Dynamic
      component={local.as ?? `button`}
      data-slot="popover-trigger"
      type="button"
      aria-haspopup="dialog"
      aria-expanded={ctx.open() ? `true` : `false`}
      aria-controls={ctx.contentId}
      data-state={stateOf(ctx.open())}
      {...rest}
      ref={mergeRefs(local.ref, ctx.setTrigger)}
      onClick={composeHandlers(local.onClick, () => ctx.setOpen(!untrack(ctx.open)))}
    />
  )
}

function PopoverAnchor(props: ComponentProps<`div`>) {
  const ctx = usePopover(`PopoverAnchor`)
  const [local, rest] = splitProps(props, [`ref`])
  return <div data-slot="popover-anchor" {...rest} ref={mergeRefs(local.ref, ctx.setAnchor)} />
}

export type PopoverContentProps = Omit<ComponentProps<`div`>, `style` | `onKeyDown`> & {
  side?: Side
  align?: Align
  sideOffset?: number
  alignOffset?: number
  collisionPadding?: number
  container?: HTMLElement | null
  style?: JSX.CSSProperties
  onKeyDown?: unknown
  onOpenAutoFocus?: (e: Event) => void
  onCloseAutoFocus?: (e: Event) => void
  onEscapeKeyDown?: (e: KeyboardEvent) => void
  onPointerDownOutside?: (e: OutsideEvent) => void
  onFocusOutside?: (e: OutsideEvent) => void
  onInteractOutside?: (e: OutsideEvent) => void
}

function PopoverContent(props: PopoverContentProps) {
  const ctx = usePopover(`PopoverContent`)
  const [local, rest] = splitProps(props, [`class`, `side`, `align`, `sideOffset`, `alignOffset`, `collisionPadding`, `container`, `style`, `ref`, `children`, `onKeyDown`, `onOpenAutoFocus`, `onCloseAutoFocus`, `onEscapeKeyDown`, `onPointerDownOutside`, `onFocusOutside`, `onInteractOutside`])
  const presence = createPresence(ctx.open)
  const Content = () => {
    const modal = ctx.modal
    let interactedOutside = false
    let pointerDownOutside = false
    let rightClickOutside = false
    const layer = createLayer({
      disableOutsidePointerEvents: () => modal,
      onEscapeKeyDown: (e) => local.onEscapeKeyDown?.(e),
      onPointerDownOutside: (e) => {
        local.onPointerDownOutside?.(e)
        if (!modal) return
        const original = e.detail.originalEvent as PointerEvent
        rightClickOutside = original.button === 2 || (original.button === 0 && original.ctrlKey === true)
      },
      onFocusOutside: (e) => {
        local.onFocusOutside?.(e)
        if (modal) e.preventDefault()
      },
      onInteractOutside: (e) => {
        local.onInteractOutside?.(e)
        if (modal) return
        if (!e.defaultPrevented) {
          interactedOutside = true
          if (e.detail.originalEvent.type === `pointerdown`) pointerDownOutside = true
        }
        const target = e.target as Node | null
        if (target && ctx.trigger()?.contains(target)) e.preventDefault()
        if (e.detail.originalEvent.type === `focusin` && pointerDownOutside) e.preventDefault()
      },
      onDismiss: () => ctx.setOpen(false),
    })
    const scope = createFocusScope({
      trapped: () => modal && ctx.open(),
      loop: true,
      onMountAutoFocus: (e) => local.onOpenAutoFocus?.(e),
      onUnmountAutoFocus: (e) => {
        if (modal) {
          local.onCloseAutoFocus?.(e)
          if (e.defaultPrevented) return
          e.preventDefault()
          if (!rightClickOutside) ctx.trigger()?.focus()
          return
        }
        local.onCloseAutoFocus?.(e)
        if (!e.defaultPrevented) {
          if (!interactedOutside) ctx.trigger()?.focus()
          e.preventDefault()
        }
        interactedOutside = false
        pointerDownOutside = false
      },
    })
    const [registered, setRegistered] = createSignal(false)
    const lag = layerMountedThisTask()
    return (
      <Popper anchor={ctx.anchor() ?? ctx.trigger()} side={local.side ?? `bottom`} align={local.align ?? `center`} sideOffset={local.sideOffset ?? 4} alignOffset={local.alignOffset} collisionPadding={local.collisionPadding ?? 0}>
        {(popper) => {
          const el = (
            <div
              data-side={popper.placedSide()}
              data-align={popper.placedAlign()}
              data-state={stateOf(ctx.open())}
              role="dialog"
              id={ctx.contentId}
              data-slot="popover-content"
              class={cn(POPOVER_CLASSES.content, local.class)}
              tabindex="-1"
              {...rest}
              onKeyDown={(e: KeyboardEvent) => {
                callHandler(local.onKeyDown, e)
                scope.onKeyDown(e)
              }}
            >
              <LayerContext.Provider value={layer.layer}>{local.children}</LayerContext.Provider>
            </div>
          ) as HTMLDivElement
          presence.ref(el)
          popper.setContent(el)
          bindStyle(el, () => popperContentStyle(`popover`, `trigger`, layerPointerEvents(layer, registered(), lag), popper.isPositioned(), styleObject(local.style)))
          callRef(local.ref, el)
          onMount(() => {
            layer.attach(el)
            setRegistered(true)
            noteLayerMount()
            scope.attach(el)
            useFocusGuards()
            if (modal) useModalEffects(el, true)
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

export { Popover, PopoverTrigger, PopoverContent, PopoverAnchor }
