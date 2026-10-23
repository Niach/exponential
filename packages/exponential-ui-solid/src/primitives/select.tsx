// FEED-131: shadcn's Select on Radix Select 2.2.6's DOM, by hand. The
// closed content renders into a detached DocumentFragment (as Radix does) so
// the items register for the trigger's typeahead and the selected
// ItemText's children portal into the SelectValue span; open, the content
// mounts on <body> in either position mode: `item-aligned` (Radix's fixed
// wrapper sized by `position()`, its inline writes in Radix's order, the
// zIndex last) or `popper` (the shared Popper). Inside a <form> Radix's
// visually-hidden native <select> follows the trigger.

import { createContext, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount, Show, splitProps, untrack, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"
import { Check, ChevronDown, ChevronUp } from "lucide-solid"
import { SELECT_CLASSES, SELECT_INLINE, SELECT_VIEWPORT_ATTR, SELECT_VIEWPORT_CSS, selectItemAlignedContentStyle, selectPopperContentStyle } from "@exponential_app/ui/web/primitives"
import { bindStyle, createFocusScope, createLayer, findNextItem, LayerContext, mountIn, NativeSelect, OPEN_KEYS, Popper, radixTypeahead, SELECTION_KEYS, styleWriter, useFocusGuards, useModalEffects, type Align, type OutsideEvent, type Side, type StyleObject } from "../behaviors"
import { insert } from "solid-js/web"
import { PaintIcon } from "../icons"
import { cn } from "./cn"

const CONTENT_MARGIN = 10

interface CollectionItem {
  value: string
  disabled: boolean
  textValue: Accessor<string>
  label: Accessor<string>
  el: HTMLElement
  text: () => HTMLElement | undefined
}

interface SelectContextValue {
  value: Accessor<string | undefined>
  setValue: (v: string) => void
  open: Accessor<boolean>
  setOpen: (o: boolean) => void
  dir: Accessor<`ltr` | `rtl`>
  disabled: Accessor<boolean | undefined>
  required: Accessor<boolean | undefined>
  contentId: string
  trigger: Accessor<HTMLButtonElement | undefined>
  setTrigger: (el: HTMLButtonElement) => void
  valueNode: Accessor<HTMLElement | undefined>
  setValueNode: (el: HTMLElement) => void
  valueNodeHasChildren: Accessor<boolean>
  setValueNodeHasChildren: (v: boolean) => void
  triggerPointerDownPos: { current: { x: number; y: number } | null }
  register: (item: CollectionItem) => () => void
  /** Radix's `getItems()`: the mounted items in document order. */
  getItems: () => CollectionItem[]
  items: Accessor<CollectionItem[]>
}

const SelectContext = createContext<SelectContextValue>()
const useSelect = () => useContext(SelectContext)!

interface ContentContextValue {
  content: Accessor<HTMLElement | undefined>
  viewport: Accessor<HTMLElement | undefined>
  setViewport: (el: HTMLElement) => void
  selectedItem: () => HTMLElement | undefined
  onItemLeave?: () => void
  focusSelectedItem?: () => void
  isPositioned: Accessor<boolean>
  searchRef: { readonly current: string }
  position: `item-aligned` | `popper`
}
const ContentContext = createContext<ContentContextValue>()
const NO_CONTENT: ContentContextValue = { content: () => undefined, viewport: () => undefined, setViewport: () => {}, selectedItem: () => undefined, isPositioned: () => false, searchRef: { current: `` }, position: `item-aligned` }

interface ViewportContextValue {
  contentWrapper?: () => HTMLElement | undefined
  shouldExpandOnScroll?: { current: boolean }
  onScrollButtonChange?: (node: HTMLElement) => void
}
const ViewportContext = createContext<ViewportContextValue>({})

interface ItemContextValue {
  value: string
  disabled: boolean
  textId: string
  isSelected: Accessor<boolean>
  onItemTextChange: (node: HTMLElement) => void
}
const ItemContext = createContext<ItemContextValue>()
const GroupContext = createContext<string>()

/** Radix's `useControllableState`. */
function createControllable<T>(prop: () => T | undefined, initial: T | undefined, onChange?: (v: T) => void): [Accessor<T | undefined>, (v: T) => void] {
  const [inner, setInner] = createSignal<T | undefined>(initial)
  const controlled = () => prop() !== undefined
  const value = () => (controlled() ? prop() : inner())
  const set = (v: T) => {
    if (controlled()) {
      if (v !== prop()) onChange?.(v)
    } else if (v !== untrack(inner)) {
      setInner(() => v)
      onChange?.(v)
    }
  }
  return [value, set]
}

/** A JSX expression's DOM node (a component's result may be an accessor). */
const node = <T extends Node>(value: unknown): T => {
  let v = value
  while (typeof v === `function`) v = (v as () => unknown)()
  return v as T
}

/** A second dynamic child: it puts Solid's insert into its marker mode, so
 *  swapping the placeholder never clears the selected item's portalled text. */
const nothing = (): JSX.Element => undefined

const shouldShowPlaceholder = (value: string | undefined) => value === `` || value === undefined

const callHandler = <E extends Event>(handler: unknown, e: E) => {
  if (typeof handler === `function`) (handler as (e: E) => void)(e)
  else if (Array.isArray(handler)) (handler[0] as (d: unknown, e: E) => void)(handler[1], e)
}

export interface SelectProps {
  value?: string
  defaultValue?: string
  onValueChange?: (value: string) => void
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  dir?: `ltr` | `rtl`
  name?: string
  autoComplete?: string
  disabled?: boolean
  required?: boolean
  form?: string
  children?: JSX.Element
}

function Select(props: SelectProps) {
  const [value, setValue] = createControllable(() => props.value, untrack(() => props.defaultValue), (v) => props.onValueChange?.(v))
  const [open, setOpen] = createControllable(() => props.open, untrack(() => props.defaultOpen) ?? false, (v) => props.onOpenChange?.(v))
  const [trigger, setTrigger] = createSignal<HTMLButtonElement>()
  const [valueNode, setValueNode] = createSignal<HTMLElement>()
  const [valueNodeHasChildren, setValueNodeHasChildren] = createSignal(false)
  const [items, setItems] = createSignal<CollectionItem[]>([])
  const register = (item: CollectionItem) => {
    setItems((list) => [...list, item])
    return () => setItems((list) => list.filter((i) => i !== item))
  }
  const getItems = () => untrack(items).slice().sort((a, b) => (a.el.compareDocumentPosition(b.el) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1))
  const ctx: SelectContextValue = {
    value,
    setValue,
    open: () => Boolean(open()),
    setOpen,
    dir: () => props.dir ?? `ltr`,
    disabled: () => props.disabled,
    required: () => props.required,
    contentId: createUniqueId(),
    trigger,
    setTrigger,
    valueNode,
    setValueNode,
    valueNodeHasChildren,
    setValueNodeHasChildren,
    triggerPointerDownPos: { current: null },
    register,
    getItems,
    items,
  }
  // Radix's `isFormControl`: decided once the trigger is mounted.
  const [inForm, setInForm] = createSignal(false)
  onMount(() => setInForm(Boolean(props.form) || Boolean(trigger()?.closest(`form`))))
  const options = createMemo(() => {
    void items()
    return getItems().map((i) => ({ value: i.value, label: i.label(), disabled: i.disabled }))
  })
  const optionKey = createMemo(() => options().map((o) => o.value).join(`;`))
  return (
    <SelectContext.Provider value={ctx}>
      {props.children}
      <Show when={inForm() ? `${optionKey()}#` : false} keyed>
        {(_key) => <NativeSelect name={props.name as string} value={value()} disabled={Boolean(props.disabled)} options={options()} onChange={(v) => setValue(v)} />}
      </Show>
    </SelectContext.Provider>
  )
}

function SelectGroup(props: ComponentProps<`div`>) {
  const id = createUniqueId()
  return (
    <GroupContext.Provider value={id}>
      <div role="group" aria-labelledby={id} data-slot="select-group" {...props} />
    </GroupContext.Provider>
  )
}

function SelectValue(props: { placeholder?: JSX.Element; children?: JSX.Element; class?: string; style?: JSX.CSSProperties | string } & Omit<ComponentProps<`span`>, `children` | `class` | `style`>) {
  const ctx = useSelect()
  const [local, rest] = splitProps(props, [`placeholder`, `children`, `class`, `style`])
  const hasChildren = `children` in props && props.children !== undefined
  onMount(() => ctx.setValueNodeHasChildren(hasChildren))
  return (
    <span data-slot="select-value" {...rest} ref={(el) => ctx.setValueNode(el)} style={SELECT_INLINE.value}>
      {shouldShowPlaceholder(ctx.value()) ? (local.placeholder ?? ``) : local.children}
      {nothing()}
    </span>
  )
}

function SelectTrigger(props: ComponentProps<`button`> & { size?: `sm` | `default` }) {
  const ctx = useSelect()
  const [local, rest] = splitProps(props, [`class`, `size`, `children`, `disabled`, `onClick`, `onPointerDown`, `onKeyDown`])
  const isDisabled = () => Boolean(ctx.disabled() || local.disabled)
  let pointerType = `touch`
  const typeahead = radixTypeahead((search) => {
    const list = ctx.getItems().filter((i) => !i.disabled).map((i) => ({ label: i.textValue(), i }))
    const next = findNextItem(list, search, list.find((x) => x.i.value === ctx.value()))
    if (next) ctx.setValue(next.i.value)
  })
  const handleOpen = (e?: PointerEvent | MouseEvent) => {
    if (!isDisabled()) {
      ctx.setOpen(true)
      typeahead.reset()
    }
    if (e) ctx.triggerPointerDownPos.current = { x: Math.round(e.pageX), y: Math.round(e.pageY) }
  }
  return (
    <button
      type="button"
      role="combobox"
      aria-controls={ctx.contentId}
      aria-expanded={ctx.open()}
      aria-required={ctx.required()}
      aria-autocomplete="none"
      dir={ctx.dir()}
      data-state={ctx.open() ? `open` : `closed`}
      disabled={isDisabled()}
      data-disabled={isDisabled() ? `` : undefined}
      data-placeholder={shouldShowPlaceholder(ctx.value()) ? `` : undefined}
      data-slot="select-trigger"
      data-size={local.size ?? `default`}
      class={cn(SELECT_CLASSES.trigger, local.class)}
      {...rest}
      ref={(el) => ctx.setTrigger(el)}
      onClick={(e) => {
        callHandler(local.onClick, e)
        if (e.defaultPrevented) return
        e.currentTarget.focus()
        if (pointerType !== `mouse`) handleOpen(e)
      }}
      onPointerDown={(e) => {
        callHandler(local.onPointerDown, e)
        if (e.defaultPrevented) return
        pointerType = e.pointerType
        const target = e.target as HTMLElement
        if (target.hasPointerCapture?.(e.pointerId)) target.releasePointerCapture(e.pointerId)
        if (e.button === 0 && e.ctrlKey === false && e.pointerType === `mouse`) {
          handleOpen(e)
          e.preventDefault()
        }
      }}
      onKeyDown={(e) => {
        callHandler(local.onKeyDown, e)
        if (e.defaultPrevented) return
        const typing = typeahead.search !== ``
        const modifier = e.ctrlKey || e.altKey || e.metaKey
        if (!modifier && e.key.length === 1) typeahead.type(e.key)
        if (typing && e.key === ` `) return
        if (OPEN_KEYS.includes(e.key)) {
          handleOpen()
          e.preventDefault()
        }
      }}
    >
      {local.children}
      <PaintIcon icon={ChevronDown} aria-hidden="true" class={SELECT_CLASSES.triggerIcon} />
    </button>
  )
}

export interface SelectContentProps extends Omit<ComponentProps<`div`>, `onKeyDown`> {
  position?: `item-aligned` | `popper`
  side?: Side
  sideOffset?: number
  align?: Align
  alignOffset?: number
  collisionPadding?: number
  onCloseAutoFocus?: (e: Event) => void
  onEscapeKeyDown?: (e: KeyboardEvent) => void
  onPointerDownOutside?: (e: OutsideEvent) => void
  onKeyDown?: (e: KeyboardEvent) => void
}

function SelectContent(props: SelectContentProps) {
  const ctx = useSelect()
  const position = () => props.position ?? `item-aligned`
  const content = () => (
    <>
      <SelectScrollUpButton />
      <Viewport class={cn(SELECT_CLASSES.viewport, position() === `popper` && SELECT_CLASSES.viewportPopper)}>{props.children}</Viewport>
      <SelectScrollDownButton />
    </>
  )
  return (
    <Show when={ctx.open()} fallback={<ClosedContent>{content()}</ClosedContent>}>
      <ContentImpl data-slot="select-content" {...props} position={position()} align={props.align ?? `center`} class={cn(SELECT_CLASSES.content, position() === `popper` && SELECT_CLASSES.contentPopper, props.class)}>
        {content()}
      </ContentImpl>
    </Show>
  )
}

/** Radix's closed SelectContent: the children in a detached fragment. */
function ClosedContent(props: { children: JSX.Element }) {
  const frag = document.createDocumentFragment()
  const div = node<HTMLElement>(
    <ContentContext.Provider value={NO_CONTENT}>
      <div>{props.children}</div>
    </ContentContext.Provider>
  )
  frag.appendChild(div)
  return null
}

function ContentImpl(props: SelectContentProps & { position: `item-aligned` | `popper` }) {
  const ctx = useSelect()
  const [local, rest] = splitProps(props, [`position`, `side`, `sideOffset`, `align`, `alignOffset`, `collisionPadding`, `onCloseAutoFocus`, `onEscapeKeyDown`, `onPointerDownOutside`, `onKeyDown`, `children`, `style`])
  const [content, setContent] = createSignal<HTMLElement>()
  const [viewport, setViewport] = createSignal<HTMLElement>()
  const [isPositioned, setPositioned] = createSignal(false)
  const [registered, setRegistered] = createSignal(false)
  const selectedItem = () => {
    const items = ctx.getItems()
    const v = ctx.value()
    return (items.find((i) => v !== undefined && i.value === v) ?? items.find((i) => !i.disabled))?.el
  }
  const selectedItemText = () => {
    const el = selectedItem()
    return ctx.getItems().find((i) => i.el === el)?.text()
  }
  const focusFirst = (candidates: (HTMLElement | undefined)[]) => {
    const [first, ...others] = ctx.getItems().map((i) => i.el)
    const [last] = others.slice(-1)
    const previous = document.activeElement
    const vp = viewport()
    for (const c of candidates) {
      if (c === previous) return
      c?.scrollIntoView({ block: `nearest` })
      if (c === first && vp) vp.scrollTop = 0
      if (c === last && vp) vp.scrollTop = vp.scrollHeight
      c?.focus()
      if (document.activeElement !== previous) return
    }
  }
  const focusSelectedItem = () => focusFirst([selectedItem(), content()])
  // Radix focuses in a later commit, once the focus scope listens: a focus
  // the trap never saw would be lost to the trigger's own click.
  createEffect(() => {
    if (isPositioned()) queueMicrotask(() => queueMicrotask(() => untrack(focusSelectedItem)))
  })
  const close = () => ctx.setOpen(false)
  onMount(() => {
    window.addEventListener(`blur`, close)
    window.addEventListener(`resize`, close)
    const el = content()
    if (el && ctx.triggerPointerDownPos.current !== null) {
      let delta = { x: 0, y: 0 }
      const move = (e: PointerEvent) => {
        const at = ctx.triggerPointerDownPos.current
        delta = { x: Math.abs(Math.round(e.pageX) - (at?.x ?? 0)), y: Math.abs(Math.round(e.pageY) - (at?.y ?? 0)) }
      }
      const up = (e: PointerEvent) => {
        if (delta.x <= 10 && delta.y <= 10) e.preventDefault()
        else if (!el.contains(e.target as Node)) close()
        document.removeEventListener(`pointermove`, move)
        ctx.triggerPointerDownPos.current = null
      }
      document.addEventListener(`pointermove`, move)
      document.addEventListener(`pointerup`, up, { capture: true, once: true })
      onCleanup(() => {
        document.removeEventListener(`pointermove`, move)
        document.removeEventListener(`pointerup`, up, { capture: true })
      })
    }
    onCleanup(() => {
      window.removeEventListener(`blur`, close)
      window.removeEventListener(`resize`, close)
    })
  })
  const searchRef = { current: `` }
  const typeahead = radixTypeahead((search) => {
    const enabled = ctx.getItems().filter((i) => !i.disabled)
    const list = enabled.map((i) => ({ label: i.textValue(), i }))
    const next = findNextItem(list, search, list.find((x) => x.i.el === document.activeElement))
    if (next) setTimeout(() => next.i.el.focus())
  })
  Object.defineProperty(searchRef, `current`, { get: () => typeahead.search })
  const layer = createLayer({
    disableOutsidePointerEvents: () => true,
    onEscapeKeyDown: (e) => local.onEscapeKeyDown?.(e),
    onPointerDownOutside: (e) => local.onPointerDownOutside?.(e),
    onFocusOutside: (e) => e.preventDefault(),
    onDismiss: close,
  })
  const scope = createFocusScope({
    trapped: () => ctx.open(),
    loop: false,
    onMountAutoFocus: (e) => e.preventDefault(),
    onUnmountAutoFocus: (e) => {
      local.onCloseAutoFocus?.(e)
      if (e.defaultPrevented) return
      ctx.trigger()?.focus({ preventScroll: true })
      e.preventDefault()
    },
  })
  const contentCtx: ContentContextValue = { content, viewport, setViewport, selectedItem, onItemLeave: () => content()?.focus(), focusSelectedItem, isPositioned, searchRef, position: local.position }
  const onKeyDown = (e: KeyboardEvent) => {
    local.onKeyDown?.(e)
    if (!e.defaultPrevented) {
      const modifier = e.ctrlKey || e.altKey || e.metaKey
      if (e.key === `Tab`) e.preventDefault()
      if (!modifier && e.key.length === 1) typeahead.type(e.key)
      if ([`ArrowUp`, `ArrowDown`, `Home`, `End`].includes(e.key)) {
        let candidates = ctx.getItems().filter((i) => !i.disabled).map((i) => i.el)
        if ([`ArrowUp`, `End`].includes(e.key)) candidates = candidates.slice().reverse()
        if ([`ArrowUp`, `ArrowDown`].includes(e.key)) candidates = candidates.slice(candidates.indexOf(e.target as HTMLElement) + 1)
        setTimeout(() => focusFirst(candidates))
        e.preventDefault()
      }
    }
    scope.onKeyDown(e)
  }
  const pointerEvents = () => layer.pointerEvents(registered())
  const build = (extra: Record<string, unknown>) =>
    node<HTMLDivElement>(
      <div role="listbox" id={ctx.contentId} data-state={ctx.open() ? `open` : `closed`} dir={ctx.dir()} {...rest} {...extra} tabindex="-1" onContextMenu={(e) => e.preventDefault()} onKeyDown={onKeyDown}>
        <LayerContext.Provider value={layer.layer}>
          <ContentContext.Provider value={contentCtx}>{local.children}</ContentContext.Provider>
        </LayerContext.Provider>
      </div>
    )
  const activate = (el: HTMLElement) => {
    onMount(() => {
      layer.attach(el)
      setRegistered(true)
      scope.attach(el)
      useFocusGuards()
      useModalEffects(el, true)
    })
  }
  if (local.position === `popper`) {
    const wrapper = node<HTMLElement>(
      <Popper anchor={ctx.trigger()} side={local.side ?? `bottom`} align={local.align ?? `start`} sideOffset={local.sideOffset ?? 0} alignOffset={local.alignOffset ?? 0} collisionPadding={local.collisionPadding ?? CONTENT_MARGIN} dir={ctx.dir()}>
        {(popper) => {
          const el = build({ "data-side": popper.placedSide(), "data-align": popper.placedAlign() })
          setContent(el)
          popper.setContent(el)
          bindStyle(el, (): StyleObject => selectPopperContentStyle(pointerEvents(), popper.isPositioned()))
          createEffect(() => {
            if (popper.isPositioned()) setPositioned(true)
          })
          activate(el)
          return el
        }}
      </Popper>
    )
    mountIn(document.body, wrapper)
    return null
  }
  return <ItemAligned build={build} setContent={setContent} contentCtx={contentCtx} pointerEvents={pointerEvents} onPlaced={() => setPositioned(true)} activate={activate} selectedItemText={selectedItemText} />
}

function ItemAligned(props: { build: (extra: Record<string, unknown>) => HTMLDivElement; setContent: (el: HTMLElement) => void; contentCtx: ContentContextValue; pointerEvents: () => string | undefined; onPlaced: () => void; activate: (el: HTMLElement) => void; selectedItemText: () => HTMLElement | undefined }) {
  const ctx = useSelect()
  const c = props.contentCtx
  let wrapper!: HTMLDivElement
  let shouldReposition = true
  const shouldExpandOnScroll = { current: false }
  const viewportCtx: ViewportContextValue = {
    contentWrapper: () => wrapper,
    shouldExpandOnScroll,
    onScrollButtonChange: (node) => {
      if (node && shouldReposition) {
        // React's isPositioned effect has focused (and scrolled to) the
        // selected item before a scroll button mounts and re-positions.
        c.focusSelectedItem?.()
        position()
        c.focusSelectedItem?.()
        shouldReposition = false
      }
    },
  }
  const el = node<HTMLDivElement>(<ViewportContext.Provider value={viewportCtx}>{props.build({})}</ViewportContext.Provider>)
  props.setContent(el)
  bindStyle(el, (): StyleObject => selectItemAlignedContentStyle(props.pointerEvents()))
  wrapper = (<div>{el}</div>) as HTMLDivElement
  styleWriter(wrapper)({ ...SELECT_INLINE.wrapper })
  const position = () => {
    const trigger = ctx.trigger()
    const valueNode = ctx.valueNode()
    const content = el
    const viewport = c.viewport()
    const selectedItem = c.selectedItem()
    const selectedItemText = props.selectedItemText()
    if (!(trigger && valueNode && wrapper && content && viewport && selectedItem && selectedItemText)) return
    const triggerRect = trigger.getBoundingClientRect()
    const contentRect = content.getBoundingClientRect()
    const valueNodeRect = valueNode.getBoundingClientRect()
    const itemTextRect = selectedItemText.getBoundingClientRect()
    const clamp = (v: number, [min, max]: [number, number]) => Math.min(max, Math.max(min, v))
    if (ctx.dir() !== `rtl`) {
      const itemTextOffset = itemTextRect.left - contentRect.left
      const left = valueNodeRect.left - itemTextOffset
      const leftDelta = triggerRect.left - left
      const minContentWidth = triggerRect.width + leftDelta
      const contentWidth = Math.max(minContentWidth, contentRect.width)
      const rightEdge = window.innerWidth - CONTENT_MARGIN
      const clampedLeft = clamp(left, [CONTENT_MARGIN, Math.max(CONTENT_MARGIN, rightEdge - contentWidth)])
      wrapper.style.minWidth = minContentWidth + `px`
      wrapper.style.left = clampedLeft + `px`
    } else {
      const itemTextOffset = contentRect.right - itemTextRect.right
      const right = window.innerWidth - valueNodeRect.right - itemTextOffset
      const rightDelta = window.innerWidth - triggerRect.right - right
      const minContentWidth = triggerRect.width + rightDelta
      const contentWidth = Math.max(minContentWidth, contentRect.width)
      const leftEdge = window.innerWidth - CONTENT_MARGIN
      const clampedRight = clamp(right, [CONTENT_MARGIN, Math.max(CONTENT_MARGIN, leftEdge - contentWidth)])
      wrapper.style.minWidth = minContentWidth + `px`
      wrapper.style.right = clampedRight + `px`
    }
    const items = ctx.getItems()
    const availableHeight = window.innerHeight - CONTENT_MARGIN * 2
    const itemsHeight = viewport.scrollHeight
    const contentStyles = window.getComputedStyle(content)
    const contentBorderTopWidth = parseInt(contentStyles.borderTopWidth, 10)
    const contentPaddingTop = parseInt(contentStyles.paddingTop, 10)
    const contentBorderBottomWidth = parseInt(contentStyles.borderBottomWidth, 10)
    const contentPaddingBottom = parseInt(contentStyles.paddingBottom, 10)
    const fullContentHeight = contentBorderTopWidth + contentPaddingTop + itemsHeight + contentPaddingBottom + contentBorderBottomWidth
    const minContentHeight = Math.min(selectedItem.offsetHeight * 5, fullContentHeight)
    const viewportStyles = window.getComputedStyle(viewport)
    const viewportPaddingTop = parseInt(viewportStyles.paddingTop, 10)
    const viewportPaddingBottom = parseInt(viewportStyles.paddingBottom, 10)
    const topEdgeToTriggerMiddle = triggerRect.top + triggerRect.height / 2 - CONTENT_MARGIN
    const triggerMiddleToBottomEdge = availableHeight - topEdgeToTriggerMiddle
    const selectedItemHalfHeight = selectedItem.offsetHeight / 2
    const itemOffsetMiddle = selectedItem.offsetTop + selectedItemHalfHeight
    const contentTopToItemMiddle = contentBorderTopWidth + contentPaddingTop + itemOffsetMiddle
    const itemMiddleToContentBottom = fullContentHeight - contentTopToItemMiddle
    const willAlignWithoutTopOverflow = contentTopToItemMiddle <= topEdgeToTriggerMiddle
    if (willAlignWithoutTopOverflow) {
      const isLastItem = items.length > 0 && selectedItem === items[items.length - 1]!.el
      wrapper.style.bottom = `0px`
      const viewportOffsetBottom = content.clientHeight - viewport.offsetTop - viewport.offsetHeight
      const clampedTriggerMiddleToBottomEdge = Math.max(triggerMiddleToBottomEdge, selectedItemHalfHeight + (isLastItem ? viewportPaddingBottom : 0) + viewportOffsetBottom + contentBorderBottomWidth)
      wrapper.style.height = contentTopToItemMiddle + clampedTriggerMiddleToBottomEdge + `px`
    } else {
      const isFirstItem = items.length > 0 && selectedItem === items[0]!.el
      wrapper.style.top = `0px`
      const clampedTopEdgeToTriggerMiddle = Math.max(topEdgeToTriggerMiddle, contentBorderTopWidth + viewport.offsetTop + (isFirstItem ? viewportPaddingTop : 0) + selectedItemHalfHeight)
      wrapper.style.height = clampedTopEdgeToTriggerMiddle + itemMiddleToContentBottom + `px`
      viewport.scrollTop = contentTopToItemMiddle - topEdgeToTriggerMiddle + viewport.offsetTop
    }
    wrapper.style.margin = `${CONTENT_MARGIN}px 0`
    wrapper.style.minHeight = minContentHeight + `px`
    wrapper.style.maxHeight = availableHeight + `px`
    props.onPlaced()
    requestAnimationFrame(() => (shouldExpandOnScroll.current = true))
  }
  onMount(() => {
    position()
    // React writes the content's zIndex on the next render, after `position()`.
    wrapper.style.zIndex = window.getComputedStyle(el).zIndex
  })
  // The selected item can change while open (Radix re-runs `position` on it).
  createEffect(on(() => [ctx.value(), c.viewport()], () => position(), { defer: true }))
  props.activate(el)
  mountIn(document.body, wrapper)
  return null
}

function Viewport(props: { class?: string; children?: JSX.Element }) {
  const c = useContext(ContentContext) ?? NO_CONTENT
  const vc = useContext(ViewportContext)
  let prevScrollTop = 0
  return (
    <>
      <style>{SELECT_VIEWPORT_CSS}</style>
      <div
        data-radix-select-viewport=""
        role="presentation"
        class={props.class}
        ref={(el) => {
          c.setViewport(el)
          styleWriter(el)({ ...SELECT_INLINE.viewport })
        }}
        onScroll={(e) => {
          const viewport = e.currentTarget
          const wrapper = vc.contentWrapper?.()
          if (vc.shouldExpandOnScroll?.current && wrapper) {
            const scrolledBy = Math.abs(prevScrollTop - viewport.scrollTop)
            if (scrolledBy > 0) {
              const availableHeight = window.innerHeight - CONTENT_MARGIN * 2
              const prevHeight = Math.max(parseFloat(wrapper.style.minHeight), parseFloat(wrapper.style.height))
              if (prevHeight < availableHeight) {
                const nextHeight = prevHeight + scrolledBy
                const clampedNextHeight = Math.min(availableHeight, nextHeight)
                const heightDiff = nextHeight - clampedNextHeight
                wrapper.style.height = clampedNextHeight + `px`
                if (wrapper.style.bottom === `0px`) {
                  viewport.scrollTop = heightDiff > 0 ? heightDiff : 0
                  wrapper.style.justifyContent = SELECT_INLINE.expandEnd
                }
              }
            }
          }
          prevScrollTop = viewport.scrollTop
        }}
      >
        {props.children}
      </div>
    </>
  )
}

function SelectLabel(props: ComponentProps<`div`>) {
  const id = useContext(GroupContext)
  const [local, rest] = splitProps(props, [`class`])
  return <div id={id} data-slot="select-label" class={cn(SELECT_CLASSES.label, local.class)} {...rest} />
}

function SelectItem(props: ComponentProps<`div`> & { value: string; disabled?: boolean; textValue?: string }) {
  const ctx = useSelect()
  const c = useContext(ContentContext) ?? NO_CONTENT
  const [local, rest] = splitProps(props, [`class`, `children`, `value`, `disabled`, `textValue`, `onFocus`, `onBlur`, `onClick`, `onPointerUp`, `onPointerDown`, `onPointerMove`, `onPointerLeave`, `onKeyDown`])
  if (local.value === ``) throw new Error(`A <Select.Item /> must have a value prop that is not an empty string. This is because the Select value can be set to an empty string to clear the selection and show the placeholder.`)
  const value = untrack(() => local.value)
  const disabled = untrack(() => Boolean(local.disabled))
  const isSelected = () => ctx.value() === value
  const [textValue, setTextValue] = createSignal(untrack(() => local.textValue) ?? ``)
  const [label, setLabel] = createSignal(``)
  const [focused, setFocused] = createSignal(false)
  const textId = createUniqueId()
  let pointerType = `touch`
  let el!: HTMLDivElement
  let textEl: HTMLElement | undefined
  const handleSelect = () => {
    if (!disabled) {
      ctx.setValue(value)
      ctx.setOpen(false)
    }
  }
  onMount(() => onCleanup(ctx.register({ value, disabled, textValue, label, el, text: () => textEl })))
  const itemCtx: ItemContextValue = {
    value,
    disabled,
    textId,
    isSelected,
    onItemTextChange: (node) => {
      textEl = node
      setTextValue((prev) => prev || (node.textContent ?? ``).trim())
      setLabel(node.textContent ?? ``)
    },
  }
  return (
    <ItemContext.Provider value={itemCtx}>
      <div
        role="option"
        aria-labelledby={textId}
        data-highlighted={focused() ? `` : undefined}
        aria-selected={isSelected() && focused()}
        data-state={isSelected() ? `checked` : `unchecked`}
        aria-disabled={disabled || undefined}
        data-disabled={disabled ? `` : undefined}
        tabindex={disabled ? undefined : -1}
        data-slot="select-item"
        class={cn(SELECT_CLASSES.item, local.class)}
        {...rest}
        data-radix-collection-item=""
        ref={el}
        onFocus={(e) => {
          callHandler(local.onFocus, e)
          if (!e.defaultPrevented) setFocused(true)
        }}
        onBlur={(e) => {
          callHandler(local.onBlur, e)
          if (!e.defaultPrevented) setFocused(false)
        }}
        onClick={(e) => {
          callHandler(local.onClick, e)
          if (!e.defaultPrevented && pointerType !== `mouse`) handleSelect()
        }}
        onPointerUp={(e) => {
          callHandler(local.onPointerUp, e)
          if (!e.defaultPrevented && pointerType === `mouse`) handleSelect()
        }}
        onPointerDown={(e) => {
          callHandler(local.onPointerDown, e)
          if (!e.defaultPrevented) pointerType = e.pointerType
        }}
        onPointerMove={(e) => {
          callHandler(local.onPointerMove, e)
          if (e.defaultPrevented) return
          pointerType = e.pointerType
          if (disabled) c.onItemLeave?.()
          else if (pointerType === `mouse`) e.currentTarget.focus({ preventScroll: true })
        }}
        onPointerLeave={(e) => {
          callHandler(local.onPointerLeave, e)
          if (!e.defaultPrevented && e.currentTarget === document.activeElement) c.onItemLeave?.()
        }}
        onKeyDown={(e) => {
          callHandler(local.onKeyDown, e)
          if (e.defaultPrevented) return
          if (c.searchRef.current !== `` && e.key === ` `) return
          if (SELECTION_KEYS.includes(e.key)) handleSelect()
          if (e.key === ` `) e.preventDefault()
        }}
      >
        <span data-slot="select-item-indicator" class={SELECT_CLASSES.itemIndicator}>
          <Show when={isSelected()}>
            <span aria-hidden="true">
              <PaintIcon icon={Check} class={SELECT_CLASSES.itemCheck} />
            </span>
          </Show>
        </span>
        <ItemText>{local.children}</ItemText>
      </div>
    </ItemContext.Provider>
  )
}

/** Radix `SelectItemText`: the text span, and the selected one's children
 *  portalled into the trigger's value span. */
function ItemText(props: { children?: JSX.Element }) {
  const ctx = useSelect()
  const item = useContext(ItemContext)!
  let span!: HTMLSpanElement
  onMount(() => item.onItemTextChange(span))
  const portalTarget = () => (item.isSelected() && !ctx.valueNodeHasChildren() ? (ctx.valueNode() ?? null) : null)
  return (
    <>
      <span id={item.textId} ref={span}>
        {props.children}
      </span>
      <Show when={portalTarget()}>
        {(target) => {
          // Radix's `createPortal(children, valueNode)`: a second copy of the
          // children appended to the value span, removed whole on unmount.
          const el = target()
          const start = document.createTextNode(``)
          const end = document.createTextNode(``)
          el.append(start, end)
          insert(el, () => props.children, end)
          onCleanup(() => {
            while (start.nextSibling && start.nextSibling !== end) start.nextSibling.remove()
            start.remove()
            end.remove()
          })
          return null
        }}
      </Show>
    </>
  )
}

function SelectSeparator(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div aria-hidden="true" data-slot="select-separator" class={cn(SELECT_CLASSES.separator, local.class)} {...rest} />
}

function ScrollButton(props: ComponentProps<`div`> & { direction: `up` | `down`; slot: string }) {
  const c = useContext(ContentContext) ?? NO_CONTENT
  const [can, setCan] = createSignal(false)
  const [local, rest] = splitProps(props, [`class`, `direction`, `slot`, `children`])
  createEffect(() => {
    const viewport = c.viewport()
    if (!viewport || !c.isPositioned()) return
    const handle = () => {
      if (local.direction === `up`) setCan(viewport.scrollTop > 0)
      else setCan(Math.ceil(viewport.scrollTop) < viewport.scrollHeight - viewport.clientHeight)
    }
    handle()
    viewport.addEventListener(`scroll`, handle)
    onCleanup(() => viewport.removeEventListener(`scroll`, handle))
  })
  return (
    <Show when={can()}>
      <ScrollButtonImpl direction={local.direction} slot={local.slot} class={local.class} rest={rest} />
    </Show>
  )
}

function ScrollButtonImpl(props: { direction: `up` | `down`; slot: string; class?: string; rest: Record<string, unknown> }) {
  const c = useContext(ContentContext) ?? NO_CONTENT
  const vc = useContext(ViewportContext)
  let timer: number | null = null
  const clear = () => {
    if (timer !== null) {
      window.clearInterval(timer)
      timer = null
    }
  }
  onCleanup(clear)
  const onAutoScroll = () => {
    const viewport = c.viewport()
    const selected = c.selectedItem()
    if (viewport && selected) viewport.scrollTop = viewport.scrollTop + (props.direction === `up` ? -1 : 1) * selected.offsetHeight
  }
  const start = () => {
    if (timer === null) timer = window.setInterval(onAutoScroll, 50)
  }
  onMount(() => {
    const active = document.activeElement
    if (active instanceof HTMLElement && active.closest(`[${SELECT_VIEWPORT_ATTR}]`)) active.scrollIntoView({ block: `nearest` })
  })
  return (
    <div
      aria-hidden="true"
      data-slot={props.slot}
      class={cn(SELECT_CLASSES.scrollButton, props.class)}
      {...props.rest}
      ref={(el) => {
        styleWriter(el)({ ...SELECT_INLINE.scrollButton })
        queueMicrotask(() => vc.onScrollButtonChange?.(el))
      }}
      onPointerDown={start}
      onPointerMove={() => {
        c.onItemLeave?.()
        start()
      }}
      onPointerLeave={clear}
    >
      <PaintIcon icon={props.direction === `up` ? ChevronUp : ChevronDown} class={SELECT_CLASSES.scrollIcon} />
    </div>
  )
}

function SelectScrollUpButton(props: ComponentProps<`div`>) {
  return <ScrollButton direction="up" slot="select-scroll-up-button" {...props} />
}

function SelectScrollDownButton(props: ComponentProps<`div`>) {
  return <ScrollButton direction="down" slot="select-scroll-down-button" {...props} />
}

export { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectScrollDownButton, SelectScrollUpButton, SelectSeparator, SelectTrigger, SelectValue }
