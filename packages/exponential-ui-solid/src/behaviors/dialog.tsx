// FEED-131: Radix `Dialog` on plain elements for the primitive set (Dialog,
// Sheet, AlertDialog): the Root's state + ids, the Trigger and Close (an `as`
// element wears Radix's attributes, React's `asChild`), the Portal (one per
// part, no wrapper, `document.body` by default), the Overlay (scroll lock,
// `pointer-events: auto`), the Content (the dismissable layer, the focus
// scope, the focus guards, `hideOthers` when modal), Title and Description.
// Plus the small pieces every overlay twin shares: the controllable open
// state, handler composition, ref merging, React-style inline styles.

import { Show, createContext, createSignal, onMount, splitProps, untrack, useContext, type Accessor, type JSX, type ValidComponent } from "solid-js"
import { Dynamic } from "solid-js/web"
import { createLayer, type OutsideEvent } from "./dismissable-layer"
import { createFocusScope } from "./focus-scope"
import { useFocusGuards } from "./focus-guards"
import { useModalEffects } from "./modal"
import { LayerPortal } from "./portal"
import { createPresence } from "./presence"
import { useScrollLock } from "./scroll-lock"
import { bindStyle, type StyleObject } from "./style-writer"

export type { OutsideEvent }

/** Calls a Solid event handler prop (a function or a `[fn, data]` pair). */
export function callHandler<E extends Event>(handler: unknown, event: E): void {
  if (typeof handler === `function`) handler(event)
  else if (Array.isArray(handler) && typeof handler[0] === `function`) handler[0](handler[1], event)
}

/** Radix's `composeEventHandlers`: the caller's handler first, ours unless it
 *  prevented the default. */
export function composeHandlers<E extends Event>(user: unknown, ours: (event: E) => void, checkForDefaultPrevented = true): (event: E) => void {
  return (event) => {
    callHandler(user, event)
    if (!checkForDefaultPrevented || !event.defaultPrevented) ours(event)
  }
}

/** Calls a Solid `ref` prop (a compiled ref callback) with the element. */
export function callRef(ref: unknown, el: unknown): void {
  if (typeof ref === `function`) ref(el)
}

/** Every ref callback of `refs` receives the element. */
export function mergeRefs<T>(...refs: unknown[]): (el: T) => void {
  return (el) => {
    for (const r of refs) if (typeof r === `function`) r(el)
  }
}

/** A Solid `style` prop (object or string) as a kebab-case object for
 *  `bindStyle` (React's semantics: the caller's style merges into ours). */
export function styleObject(style: unknown): StyleObject {
  if (!style || typeof style !== `object`) return {}
  const out: StyleObject = {}
  for (const [k, v] of Object.entries(style as Record<string, unknown>)) out[k] = v === undefined || v === null ? undefined : String(v)
  return out
}

/** Radix `useControllableState` for an overlay's `open`: controlled when
 *  `open()` is defined, else local from `defaultOpen`; `onOpenChange` fires
 *  on a change only, BEFORE the parts re-render (Radix calls it before its
 *  portalled content mounts: a Tooltip's "another tooltip opened" event
 *  must not reach its own content). */
export function createOpenState(o: { open: () => boolean | undefined; defaultOpen?: boolean; onOpenChange: () => ((open: boolean) => void) | undefined }): [Accessor<boolean>, (next: boolean) => void] {
  const [inner, setInner] = createSignal(untrack(() => o.defaultOpen) ?? false)
  const open = () => o.open() ?? inner()
  const set = (next: boolean) => {
    const prev = untrack(open)
    if (next !== prev) untrack(o.onOpenChange)?.(next)
    if (untrack(o.open) === undefined) setInner(next)
  }
  return [open, set]
}

export const stateOf = (open: boolean) => (open ? `open` : `closed`)

let layerMounted = false
/** A primitive layer attached in this task (see `layerMountedThisTask`). */
export function noteLayerMount(): void {
  if (layerMounted) return
  layerMounted = true
  queueMicrotask(() => (layerMounted = false))
}
/** Whether a primitive layer attached in this same task: a content created
 *  now mounts NESTED in it (Radix registers that outer layer one commit
 *  after this content's first render, so its first `pointer-events` is
 *  unset). */
export function layerMountedThisTask(): boolean {
  return layerMounted
}

/** The layer's `pointer-events` with that lag applied. */
export function layerPointerEvents(layer: { pointerEvents: (registered: boolean) => string | undefined }, registered: boolean, lag: boolean): string | undefined {
  return lag && !registered ? undefined : layer.pointerEvents(registered)
}

export interface DialogContextValue {
  open: Accessor<boolean>
  setOpen: (open: boolean) => void
  modal: boolean
  contentId: string
  titleId: string
  descriptionId: string
  trigger: Accessor<HTMLElement | undefined>
  setTrigger: (el: HTMLElement | undefined) => void
}

const DialogContext = createContext<DialogContextValue>()

export function useDialogContext(part: string): DialogContextValue {
  const ctx = useContext(DialogContext)
  if (!ctx) throw new Error(`\`${part}\` must be used within \`Dialog\``)
  return ctx
}

export interface DialogRootProps {
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  modal?: boolean
  children?: JSX.Element
}

/** Radix `Dialog` (Root): renders no element. */
export function DialogRoot(props: DialogRootProps): JSX.Element {
  const [open, setOpen] = createOpenState({ open: () => props.open, defaultOpen: props.defaultOpen, onOpenChange: () => props.onOpenChange })
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const value: DialogContextValue = {
    open,
    setOpen,
    modal: untrack(() => props.modal) ?? true,
    contentId: `radix-${createId()}`,
    titleId: `radix-${createId()}`,
    descriptionId: `radix-${createId()}`,
    trigger,
    setTrigger,
  }
  return <DialogContext.Provider value={value}>{props.children}</DialogContext.Provider>
}

let idCounter = 0
/** A fresh id: `radix-<n>` keeps the parity rename rule (generated ids). */
export function createId(): string {
  return `s${++idCounter}`
}

export type AsProps = { as?: ValidComponent; ref?: unknown; onClick?: unknown; children?: JSX.Element; [key: string]: unknown }

/** Radix `DialogTrigger`: `type="button"`, `aria-haspopup="dialog"`,
 *  `aria-expanded`, `aria-controls`, `data-state`, a click toggles. */
export function DialogTriggerPart(props: AsProps): JSX.Element {
  const ctx = useDialogContext(`DialogTrigger`)
  const [local, rest] = splitProps(props, [`as`, `ref`, `onClick`])
  return (
    <Dynamic
      component={local.as ?? `button`}
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

/** Radix `DialogClose`: `type="button"`, a click closes. */
export function DialogClosePart(props: AsProps): JSX.Element {
  const ctx = useDialogContext(`DialogClose`)
  const [local, rest] = splitProps(props, [`as`, `ref`, `onClick`])
  return <Dynamic component={local.as ?? `button`} type="button" {...rest} ref={local.ref} onClick={composeHandlers(local.onClick, () => ctx.setOpen(false))} />
}

/** Radix `Portal` as the primitives use it: the children appended to
 *  `container`, no wrapper. Without a container Radix mounts into
 *  `document.body` one commit LATER (its `mounted` state): a portal nested
 *  in another portal's content lands after it, once the outer content's
 *  effects (`hideOthers`, the focus guards) ran. */
export function DialogPortalPart(props: { container?: HTMLElement | null; children?: JSX.Element }): JSX.Element {
  const [mounted, setMounted] = createSignal(false)
  onMount(() => setMounted(true))
  return <LayerPortal mount={props.container !== undefined ? props.container : mounted() ? document.body : null}>{props.children}</LayerPortal>
}

/** Radix `DialogOverlay` (modal dialogs only): `data-state`, the scroll
 *  lock, `pointer-events: auto`; present while open (+ its exit animation). */
export function DialogOverlayPart(props: { style?: unknown; ref?: unknown; [key: string]: unknown }): JSX.Element {
  const ctx = useDialogContext(`DialogOverlay`)
  const presence = createPresence(ctx.open)
  const [local, rest] = splitProps(props, [`style`, `ref`])
  const Overlay = () => {
    const el = (<div data-state={stateOf(ctx.open())} {...rest} />) as HTMLDivElement
    presence.ref(el)
    bindStyle(el, () => ({ "pointer-events": `auto`, ...styleObject(local.style) }))
    callRef(local.ref, el)
    onMount(() => useScrollLock())
    return el
  }
  return (
    <Show when={ctx.modal && presence.mounted()}>
      <Overlay />
    </Show>
  )
}

export interface DialogContentPartProps {
  role?: string
  style?: unknown
  ref?: unknown
  onOpenAutoFocus?: (e: Event) => void
  onCloseAutoFocus?: (e: Event) => void
  onEscapeKeyDown?: (e: KeyboardEvent) => void
  onPointerDownOutside?: (e: OutsideEvent) => void
  onFocusOutside?: (e: OutsideEvent) => void
  onInteractOutside?: (e: OutsideEvent) => void
  onKeyDown?: unknown
  children?: JSX.Element
  [key: string]: unknown
}

/** Radix `DialogContent` (modal or not, per the Root): `role="dialog"`,
 *  `id`, `aria-describedby`, `aria-labelledby`, `data-state`,
 *  `tabindex="-1"`, the layer's `pointer-events`; present while open. */
export function DialogContentPart(props: DialogContentPartProps): JSX.Element {
  const ctx = useDialogContext(`DialogContent`)
  const presence = createPresence(ctx.open)
  const [local, rest] = splitProps(props, [`role`, `style`, `ref`, `children`, `onOpenAutoFocus`, `onCloseAutoFocus`, `onEscapeKeyDown`, `onPointerDownOutside`, `onFocusOutside`, `onInteractOutside`, `onKeyDown`])
  const Content = () => {
    const modal = ctx.modal
    let interactedOutside = false
    let pointerDownOutside = false
    const layer = createLayer({
      disableOutsidePointerEvents: () => modal,
      onEscapeKeyDown: (e) => local.onEscapeKeyDown?.(e),
      onPointerDownOutside: (e) => {
        local.onPointerDownOutside?.(e)
        if (!modal || e.defaultPrevented) return
        const original = e.detail.originalEvent as PointerEvent
        const ctrlLeftClick = original.button === 0 && original.ctrlKey === true
        if (original.button === 2 || ctrlLeftClick) e.preventDefault()
      },
      onFocusOutside: (e) => {
        local.onFocusOutside?.(e)
        if (modal && !e.defaultPrevented) e.preventDefault()
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
          ctx.trigger()?.focus()
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
    const el = (
      <div
        role={(local.role ?? `dialog`) as `dialog`}
        id={ctx.contentId}
        aria-describedby={ctx.descriptionId}
        aria-labelledby={ctx.titleId}
        data-state={stateOf(ctx.open())}
        tabindex="-1"
        {...rest}
        onKeyDown={(e: KeyboardEvent) => {
          callHandler(local.onKeyDown, e)
          scope.onKeyDown(e)
        }}
      >
        {local.children}
      </div>
    ) as HTMLDivElement
    presence.ref(el)
    bindStyle(el, () => ({ "pointer-events": layerPointerEvents(layer, registered(), lag), ...styleObject(local.style) }))
    callRef(local.ref, el)
    onMount(() => {
      layer.attach(el)
      setRegistered(true)
      noteLayerMount()
      scope.attach(el)
      useFocusGuards()
      if (modal) useModalEffects(el, false)
    })
    return el
  }
  return (
    <Show when={presence.mounted()}>
      <Content />
    </Show>
  )
}

/** Radix `DialogTitle`: `<h2 id=titleId>`. */
export function DialogTitlePart(props: { [key: string]: unknown; children?: JSX.Element }): JSX.Element {
  const ctx = useDialogContext(`DialogTitle`)
  return <h2 id={ctx.titleId} {...props} />
}

/** Radix `DialogDescription`: `<p id=descriptionId>`. */
export function DialogDescriptionPart(props: { [key: string]: unknown; children?: JSX.Element }): JSX.Element {
  const ctx = useDialogContext(`DialogDescription`)
  return <p id={ctx.descriptionId} {...props} />
}
