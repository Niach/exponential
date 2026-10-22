import { Show, createContext, createEffect, createSignal, on, onCleanup, splitProps, untrack, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { Check, ChevronRight, Circle } from "lucide-solid"
import { DROPDOWN_MENU_CLASSES } from "@exponential-at/ui/web/primitives"

import { MenuContent, createMenuRow, createPresence, createSelectable, useKeyboardTracking, type Align, type OutsideEvent, type Side } from "../behaviors"
import { DialogPortalPart, layerMountedThisTask, noteLayerMount, callRef, composeHandlers, createId, createOpenState, mergeRefs, stateOf, type AsProps } from "../behaviors/dialog"
import { PaintIcon } from "../icons"
import { cn } from "./cn"

// Radix DropdownMenu on plain elements, on the Menu machinery of
// ../behaviors/menu (the modal root content, roving focus, typeahead, the
// submenus' pointer grace): every part shadcn's dropdown-menu.tsx exports.

interface RootValue {
  open: Accessor<boolean>
  setOpen: (open: boolean) => void
  triggerId: string
  contentId: string
  trigger: Accessor<HTMLElement | undefined>
  setTrigger: (el: HTMLElement | undefined) => void
  dir: () => `ltr` | `rtl`
}
const RootContext = createContext<RootValue>()
const useRoot = (part: string) => {
  const ctx = useContext(RootContext)
  if (!ctx) throw new Error(`\`${part}\` must be used within \`DropdownMenu\``)
  return ctx
}
/** The open state of the enclosing menu level (the root or a submenu). */
const LevelOpenContext = createContext<Accessor<boolean>>(() => false)

export interface DropdownMenuProps {
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  /** Radix's `modal` (the root content is always modal here). */
  modal?: boolean
  dir?: `ltr` | `rtl`
  children?: JSX.Element
}

function DropdownMenu(props: DropdownMenuProps) {
  const [open, setOpen] = createOpenState({ open: () => props.open, defaultOpen: props.defaultOpen, onOpenChange: () => props.onOpenChange })
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  useKeyboardTracking()
  const value: RootValue = { open, setOpen, triggerId: `radix-${createId()}`, contentId: `radix-${createId()}`, trigger, setTrigger, dir: () => props.dir ?? `ltr` }
  return (
    <RootContext.Provider value={value}>
      <LevelOpenContext.Provider value={open}>{props.children}</LevelOpenContext.Provider>
    </RootContext.Provider>
  )
}

function DropdownMenuPortal(props: { container?: HTMLElement | null; children?: JSX.Element }) {
  return <DialogPortalPart {...props} />
}

function DropdownMenuTrigger(props: AsProps & { disabled?: boolean }) {
  const ctx = useRoot(`DropdownMenuTrigger`)
  const [local, rest] = splitProps(props, [`as`, `ref`, `disabled`, `onPointerDown`, `onKeyDown`])
  const disabled = () => local.disabled ?? false
  return (
    <Dynamic
      component={local.as ?? `button`}
      data-slot="dropdown-menu-trigger"
      type="button"
      id={ctx.triggerId}
      aria-haspopup="menu"
      aria-expanded={ctx.open() ? `true` : `false`}
      aria-controls={ctx.open() ? ctx.contentId : undefined}
      data-state={stateOf(ctx.open())}
      data-disabled={disabled() ? `` : undefined}
      disabled={disabled()}
      {...rest}
      ref={mergeRefs(local.ref, ctx.setTrigger)}
      onPointerDown={composeHandlers(local.onPointerDown, (e: PointerEvent) => {
        if (!untrack(disabled) && e.button === 0 && e.ctrlKey === false) {
          const was = untrack(ctx.open)
          ctx.setOpen(!was)
          if (!was) e.preventDefault()
        }
      })}
      onKeyDown={composeHandlers(local.onKeyDown, (e: KeyboardEvent) => {
        if (untrack(disabled)) return
        if ([`Enter`, ` `].includes(e.key)) ctx.setOpen(!untrack(ctx.open))
        if (e.key === `ArrowDown`) ctx.setOpen(true)
        if ([`Enter`, ` `, `ArrowDown`].includes(e.key)) e.preventDefault()
      })}
    />
  )
}

type ContentProps = Omit<ComponentProps<`div`>, `style` | `onKeyDown`> & {
  side?: Side
  align?: Align
  sideOffset?: number
  alignOffset?: number
  collisionPadding?: number
  container?: HTMLElement | null
  onKeyDown?: (e: KeyboardEvent) => void
  onCloseAutoFocus?: (e: Event) => void
  onEscapeKeyDown?: (e: KeyboardEvent) => void
  onFocusOutside?: (e: OutsideEvent) => void
  onInteractOutside?: (e: OutsideEvent) => void
}

const CONTENT_KEYS = [`class`, `side`, `align`, `sideOffset`, `alignOffset`, `collisionPadding`, `container`, `children`, `ref`, `onKeyDown`, `onCloseAutoFocus`, `onEscapeKeyDown`, `onFocusOutside`, `onInteractOutside`] as const

function DropdownMenuContent(props: ContentProps) {
  const ctx = useRoot(`DropdownMenuContent`)
  const [local, rest] = splitProps(props, CONTENT_KEYS)
  const presence = createPresence(ctx.open)
  let interactedOutside = false
  return (
    <DropdownMenuPortal container={local.container}>
      <Show when={presence.mounted()}>
        {(_) => {
          const lag = layerMountedThisTask()
          return (
        <MenuContent
          anchor={ctx.trigger()}
          side={local.side ?? `bottom`}
          align={local.align ?? `center`}
          sideOffset={local.sideOffset ?? 4}
          alignOffset={local.alignOffset}
          collisionPadding={local.collisionPadding ?? 0}
          dir={ctx.dir()}
          kind="dropdown-menu"
          lagParentLayer={lag}
          root
          id={ctx.contentId}
          labelledBy={ctx.triggerId}
          state={() => stateOf(ctx.open())}
          attrs={() => ({ "data-slot": `dropdown-menu-content`, class: cn(DROPDOWN_MENU_CLASSES.content, local.class), ...rest })}
          setContent={(el) => {
            if (el) {
              presence.ref(el)
              noteLayerMount()
              callRef(local.ref, el)
            }
          }}
          onDismiss={() => ctx.setOpen(false)}
          onKeyDown={local.onKeyDown}
          onEscapeKeyDown={local.onEscapeKeyDown}
          onFocusOutside={local.onFocusOutside}
          onInteractOutside={(e) => {
            local.onInteractOutside?.(e)
            if (e.defaultPrevented) return
            const original = e.detail.originalEvent as PointerEvent
            const ctrlLeftClick = original.button === 0 && original.ctrlKey === true
            if (original.button === 2 || ctrlLeftClick) interactedOutside = true
          }}
          onCloseAutoFocus={(e) => {
            local.onCloseAutoFocus?.(e)
            if (e.defaultPrevented) return
            if (!interactedOutside) ctx.trigger()?.focus()
            interactedOutside = false
            e.preventDefault()
          }}
        >
          {local.children}
        </MenuContent>
          )
        }}
      </Show>
    </DropdownMenuPortal>
  )
}

function DropdownMenuGroup(props: ComponentProps<`div`>) {
  return <div role="group" data-slot="dropdown-menu-group" {...props} />
}

const insetAttr = (inset: boolean | undefined) => (inset === undefined ? undefined : String(inset))

type ItemProps = Omit<ComponentProps<`div`>, `onSelect`> & {
  disabled?: boolean
  onSelect?: (e: Event) => void
}

/** Radix `MenuItem`'s row: the roving attributes, the selection, the
 *  caller's handlers first. */
function createItem(local: { disabled?: boolean; onSelect?: (e: Event) => void; ref?: unknown; onClick?: unknown; onPointerDown?: unknown; onPointerUp?: unknown; onPointerMove?: unknown; onPointerLeave?: unknown; onKeyDown?: unknown; onFocus?: unknown; onBlur?: unknown }, onSelect: (e: Event) => void) {
  const root = useRoot(`DropdownMenuItem`)
  const row = createSelectable(
    () => local.disabled ?? false,
    (e) => onSelect(e),
    () => root.setOpen(false)
  )
  const h = row.handlers
  return {
    attrs: row.attrs,
    ref: mergeRefs(row.ref, local.ref),
    handlers: {
      onClick: composeHandlers(local.onClick, h.onClick),
      onPointerDown: composeHandlers(local.onPointerDown, h.onPointerDown, false),
      onPointerUp: composeHandlers(local.onPointerUp, h.onPointerUp),
      onPointerMove: composeHandlers(local.onPointerMove, h.onPointerMove),
      onPointerLeave: composeHandlers(local.onPointerLeave, h.onPointerLeave),
      onKeyDown: composeHandlers(local.onKeyDown, h.onKeyDown),
      onFocus: composeHandlers(local.onFocus, h.onFocus),
      onBlur: composeHandlers(local.onBlur, h.onBlur),
    },
  }
}

const ITEM_KEYS = [`disabled`, `onSelect`, `ref`, `onClick`, `onPointerDown`, `onPointerUp`, `onPointerMove`, `onPointerLeave`, `onKeyDown`, `onFocus`, `onBlur`] as const

function DropdownMenuItem(props: ItemProps & { inset?: boolean; variant?: `default` | `destructive` }) {
  const [local, handlers, rest] = splitProps(props, [`class`, `inset`, `variant`], ITEM_KEYS)
  const item = createItem(handlers, (e) => handlers.onSelect?.(e))
  return (
    <div
      ref={item.ref}
      role="menuitem"
      {...item.attrs()}
      data-slot="dropdown-menu-item"
      data-inset={insetAttr(local.inset)}
      data-variant={local.variant ?? `default`}
      class={cn(DROPDOWN_MENU_CLASSES.item, local.class)}
      {...rest}
      {...(item.handlers as JSX.HTMLAttributes<HTMLDivElement>)}
    />
  )
}

type CheckedState = boolean | `indeterminate`
const checkedState = (c: CheckedState) => (c === `indeterminate` ? `indeterminate` : c ? `checked` : `unchecked`)
const IndicatorContext = createContext<Accessor<CheckedState>>(() => false)

function ItemIndicator(props: { children?: JSX.Element }) {
  const checked = useContext(IndicatorContext)
  return (
    <Show when={checked() === true || checked() === `indeterminate`}>
      <span data-state={checkedState(checked())}>{props.children}</span>
    </Show>
  )
}

function DropdownMenuCheckboxItem(props: ItemProps & { checked?: CheckedState; onCheckedChange?: (checked: boolean) => void }) {
  const [local, handlers, rest] = splitProps(props, [`class`, `children`, `checked`, `onCheckedChange`], ITEM_KEYS)
  const checked = () => local.checked ?? false
  const item = createItem(handlers, (e) => {
    handlers.onSelect?.(e)
    const c = untrack(checked)
    local.onCheckedChange?.(c === `indeterminate` ? true : !c)
  })
  return (
    <IndicatorContext.Provider value={checked}>
      <div
        ref={item.ref}
        role="menuitemcheckbox"
        aria-checked={checked() === `indeterminate` ? `mixed` : checked() ? `true` : `false`}
        {...item.attrs()}
        data-slot="dropdown-menu-checkbox-item"
        class={cn(DROPDOWN_MENU_CLASSES.checkItem, local.class)}
        {...rest}
        data-state={checkedState(checked())}
        {...(item.handlers as JSX.HTMLAttributes<HTMLDivElement>)}
      >
        <span class={DROPDOWN_MENU_CLASSES.itemIndicator}>
          <ItemIndicator>
            <PaintIcon icon={Check} class={DROPDOWN_MENU_CLASSES.checkIcon} />
          </ItemIndicator>
        </span>
        {local.children}
      </div>
    </IndicatorContext.Provider>
  )
}

const RadioGroupContext = createContext<{ value: () => string | undefined; onValueChange: (value: string) => void }>({ value: () => undefined, onValueChange: () => {} })

function DropdownMenuRadioGroup(props: Omit<ComponentProps<`div`>, `onChange`> & { value?: string; onValueChange?: (value: string) => void }) {
  const [local, rest] = splitProps(props, [`value`, `onValueChange`])
  return (
    <RadioGroupContext.Provider value={{ value: () => local.value, onValueChange: (v) => local.onValueChange?.(v) }}>
      <div role="group" data-slot="dropdown-menu-radio-group" {...rest} />
    </RadioGroupContext.Provider>
  )
}

function DropdownMenuRadioItem(props: ItemProps & { value: string }) {
  const [local, handlers, rest] = splitProps(props, [`class`, `children`, `value`], ITEM_KEYS)
  const group = useContext(RadioGroupContext)
  const checked = () => local.value === group.value()
  const item = createItem(handlers, (e) => {
    handlers.onSelect?.(e)
    group.onValueChange(local.value)
  })
  return (
    <IndicatorContext.Provider value={checked}>
      <div
        ref={item.ref}
        role="menuitemradio"
        aria-checked={checked() ? `true` : `false`}
        {...item.attrs()}
        data-slot="dropdown-menu-radio-item"
        class={cn(DROPDOWN_MENU_CLASSES.checkItem, local.class)}
        {...rest}
        data-state={checkedState(checked())}
        {...(item.handlers as JSX.HTMLAttributes<HTMLDivElement>)}
      >
        <span class={DROPDOWN_MENU_CLASSES.itemIndicator}>
          <ItemIndicator>
            <PaintIcon icon={Circle} class={DROPDOWN_MENU_CLASSES.radioIcon} />
          </ItemIndicator>
        </span>
        {local.children}
      </div>
    </IndicatorContext.Provider>
  )
}

function DropdownMenuLabel(props: ComponentProps<`div`> & { inset?: boolean }) {
  const [local, rest] = splitProps(props, [`class`, `inset`])
  return <div data-slot="dropdown-menu-label" data-inset={insetAttr(local.inset)} class={cn(DROPDOWN_MENU_CLASSES.label, local.class)} {...rest} />
}

function DropdownMenuSeparator(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div role="separator" aria-orientation="horizontal" data-slot="dropdown-menu-separator" class={cn(DROPDOWN_MENU_CLASSES.separator, local.class)} {...rest} />
}

function DropdownMenuShortcut(props: ComponentProps<`span`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <span data-slot="dropdown-menu-shortcut" class={cn(DROPDOWN_MENU_CLASSES.shortcut, local.class)} {...rest} />
}

interface SubValue {
  open: Accessor<boolean>
  setOpen: (open: boolean) => void
  triggerId: string
  contentId: string
  trigger: Accessor<HTMLElement | undefined>
  setTrigger: (el: HTMLElement | undefined) => void
  content: Accessor<HTMLElement | undefined>
  setContent: (el: HTMLElement | undefined) => void
}
const SubContext = createContext<SubValue>()
const useSub = (part: string) => {
  const ctx = useContext(SubContext)
  if (!ctx) throw new Error(`\`${part}\` must be used within \`DropdownMenuSub\``)
  return ctx
}

function DropdownMenuSub(props: { open?: boolean; defaultOpen?: boolean; onOpenChange?: (open: boolean) => void; children?: JSX.Element }) {
  const parentOpen = useContext(LevelOpenContext)
  const [open, setOpen] = createOpenState({ open: () => props.open, defaultOpen: props.defaultOpen, onOpenChange: () => props.onOpenChange })
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const [content, setContent] = createSignal<HTMLElement>()
  // A submenu closes with the menu it sits in.
  createEffect(on(parentOpen, (o) => o === false && setOpen(false)))
  onCleanup(() => setOpen(false))
  const value: SubValue = { open, setOpen, triggerId: `radix-${createId()}`, contentId: `radix-${createId()}`, trigger, setTrigger, content, setContent }
  return (
    <SubContext.Provider value={value}>
      <LevelOpenContext.Provider value={open}>{props.children}</LevelOpenContext.Provider>
    </SubContext.Provider>
  )
}

function DropdownMenuSubTrigger(props: ComponentProps<`div`> & { inset?: boolean; variant?: `default` | `destructive`; disabled?: boolean }) {
  const root = useRoot(`DropdownMenuSubTrigger`)
  const sub = useSub(`DropdownMenuSubTrigger`)
  const [local, rest] = splitProps(props, [`class`, `inset`, `variant`, `children`, `disabled`, `ref`, `onClick`, `onPointerMove`, `onPointerLeave`, `onKeyDown`, `onFocus`, `onBlur`])
  const disabled = () => local.disabled ?? false
  const row = createMenuRow(disabled)
  let openTimer: ReturnType<typeof setTimeout> | undefined
  let graceTimer: ReturnType<typeof setTimeout> | undefined
  const clearOpenTimer = () => {
    clearTimeout(openTimer)
    openTimer = undefined
  }
  onCleanup(() => {
    clearOpenTimer()
    clearTimeout(graceTimer)
    row.level.setGrace(null)
  })
  const openKey = () => (root.dir() === `rtl` ? `ArrowLeft` : `ArrowRight`)
  return (
    <div
      ref={mergeRefs(row.ref, sub.setTrigger, local.ref)}
      role="menuitem"
      id={sub.triggerId}
      aria-haspopup="menu"
      aria-expanded={sub.open() ? `true` : `false`}
      aria-controls={sub.contentId}
      data-state={stateOf(sub.open())}
      {...row.attrs()}
      data-slot="dropdown-menu-sub-trigger"
      data-inset={insetAttr(local.inset)}
      data-variant={local.variant ?? `default`}
      class={cn(DROPDOWN_MENU_CLASSES.subTrigger, local.class)}
      {...rest}
      onClick={composeHandlers(local.onClick, (e: MouseEvent) => {
        if (untrack(disabled)) return
        ;(e.currentTarget as HTMLElement).focus()
        if (!untrack(sub.open)) sub.setOpen(true)
      })}
      onPointerMove={composeHandlers(local.onPointerMove, (e: PointerEvent) => {
        if (e.pointerType !== `mouse`) return
        // Moving toward the open submenu: neither re-open nor re-focus.
        if (row.level.onItemEnter(e)) return
        if (!untrack(disabled) && !untrack(sub.open) && !openTimer) {
          row.level.setGrace(null)
          openTimer = setTimeout(() => {
            sub.setOpen(true)
            clearOpenTimer()
          }, 100)
        }
        row.handlers.onPointerMove(e)
      })}
      onPointerLeave={composeHandlers(local.onPointerLeave, (e: PointerEvent) => {
        if (e.pointerType !== `mouse`) return
        clearOpenTimer()
        const content = sub.content()
        const rect = content?.getBoundingClientRect()
        if (content && rect) {
          const side = content.dataset.side as Side
          const right = side === `right`
          const bleed = right ? -5 : 5
          const near = rect[right ? `left` : `right`]
          const far = rect[right ? `right` : `left`]
          row.level.setGrace({
            area: [
              { x: e.clientX + bleed, y: e.clientY },
              { x: near, y: rect.top },
              { x: far, y: rect.top },
              { x: far, y: rect.bottom },
              { x: near, y: rect.bottom },
            ],
            side,
          })
          clearTimeout(graceTimer)
          graceTimer = setTimeout(() => row.level.setGrace(null), 300)
        } else {
          if (row.level.onTriggerLeave(e)) return
          row.level.setGrace(null)
          row.handlers.onPointerLeave(e)
        }
      })}
      onKeyDown={composeHandlers(local.onKeyDown, (e: KeyboardEvent) => {
        const typingAhead = row.level.search() !== ``
        if (untrack(disabled) || (typingAhead && e.key === ` `)) return
        if ([`Enter`, ` `, openKey()].includes(e.key)) {
          sub.setOpen(true)
          sub.content()?.focus()
          e.preventDefault()
          return
        }
        row.handlers.onKeyDown(e)
      })}
      onFocus={composeHandlers(local.onFocus, row.handlers.onFocus)}
      onBlur={composeHandlers(local.onBlur, row.handlers.onBlur)}
    >
      {local.children}
      <PaintIcon icon={ChevronRight} class={DROPDOWN_MENU_CLASSES.subChevron} />
    </div>
  )
}

function DropdownMenuSubContent(props: ContentProps) {
  const root = useRoot(`DropdownMenuSubContent`)
  const sub = useSub(`DropdownMenuSubContent`)
  const [local, rest] = splitProps(props, CONTENT_KEYS)
  const presence = createPresence(sub.open)
  const closeKey = () => (root.dir() === `rtl` ? `ArrowRight` : `ArrowLeft`)
  return (
    <DropdownMenuPortal container={local.container}>
      <Show when={presence.mounted()}>
        {(_) => {
          const lag = layerMountedThisTask()
          return (
        <MenuContent
          anchor={sub.trigger()}
          side={root.dir() === `rtl` ? `left` : `right`}
          align="start"
          sideOffset={local.sideOffset ?? 0}
          alignOffset={local.alignOffset}
          collisionPadding={local.collisionPadding ?? 0}
          dir={root.dir()}
          kind="dropdown-menu"
          lagParentLayer={lag}
          root={false}
          id={sub.contentId}
          labelledBy={sub.triggerId}
          state={() => stateOf(sub.open())}
          attrs={() => ({ "data-slot": `dropdown-menu-sub-content`, class: cn(DROPDOWN_MENU_CLASSES.subContent, local.class), ...rest })}
          setContent={(el) => {
            sub.setContent(el)
            if (el) {
              presence.ref(el)
              noteLayerMount()
              callRef(local.ref, el)
            }
          }}
          // Radix's sub content has no dismiss of its own: the root's layer
          // answers outside pointers, Escape closes the whole menu.
          onDismiss={() => {}}
          onEscapeKeyDown={(e) => {
            local.onEscapeKeyDown?.(e)
            if (e.defaultPrevented) return
            root.setOpen(false)
            e.preventDefault()
          }}
          onFocusOutside={(e) => {
            local.onFocusOutside?.(e)
            if (e.defaultPrevented) return
            if (e.target !== sub.trigger()) sub.setOpen(false)
          }}
          onInteractOutside={local.onInteractOutside}
          onCloseAutoFocus={(e) => e.preventDefault()}
          onKeyDown={(e) => {
            local.onKeyDown?.(e)
            if (e.defaultPrevented) return
            if (sub.content()?.contains(e.target as Node) && e.key === closeKey()) {
              sub.setOpen(false)
              sub.trigger()?.focus()
              e.preventDefault()
            }
          }}
        >
          {local.children}
        </MenuContent>
          )
        }}
      </Show>
    </DropdownMenuPortal>
  )
}

export {
  DropdownMenu,
  DropdownMenuPortal,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuItem,
  DropdownMenuCheckboxItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
}
