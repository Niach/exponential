// FEED-128 (React's natives/overlays.tsx): the OVERLAYS — Dialog, Drawer,
// Popover, Tooltip, Menu (press | contextmenu) and the Toast. Every portal
// mounts into the surface's OVERLAY LAYER inside the `xui` container (so a
// breakpoint still matches inside a Dialog, audit bug C); toasts mount into
// the TOAST layer above it.
//
// React paints these on Radix; the Solid painter paints Radix's DOM itself
// (Kobalte's parts cannot produce it: their wrappers, attribute names,
// portal containers and positioning differ) on plain elements, with the
// SAME machinery underneath: `@floating-ui/dom` with Radix Popper's exact
// middleware (offset → shift(limitShift) → flip → size → transformOrigin,
// `strategy: fixed`, `OVERLAY_OFFSET`/`OVERLAY_PADDING`, so `placeOverlay`
// and fixtures/overlay-geometry.json hold), `aria-hidden`'s `hideOthers`,
// the dismissable-layer stack (Escape, pointer-down and focus outside, the
// body's pointer-events while a modal layer is up), the focus-scope stack
// (auto focus, trap, loop, return), the focus guards and the scroll lock,
// each writing what Radix writes in the order Radix writes it (the
// web-parity check diffs the `style` attribute strings).

import { For, Show, createContext, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount, untrack, useContext, type Accessor, type Component, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import type { VirtualElement } from "@floating-ui/dom"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential_app/ui"
import { absolutePath, isBinding } from "@exponential_app/ui/web"
import { useSurfaceContext } from "../context"
import { CloseOnSubmitContext } from "../form"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, num, objects, parts, str, type Attrs, type PartFn } from "./shared"
import { SearchBody, type SearchItem } from "./search"
import { bindStyle, createDragToDismiss, createFocusScope, createLayer, createMenuRow, createPresence, createSelectable, graceArea, inPolygon, LayerContext, LayerPortal, MenuContent, Popper, popperContentStyle, searchMatches, useFocusGuards, useKeyboardTracking, useModalEffects, useScrollLock, VisuallyHidden, type Point, type Side } from "../behaviors"

// The machinery (Radix's primitives on plain elements: the layer stack, the
// focus scopes, the popper, the menu content) lives in `../behaviors`; this
// file paints the overlays' parts on it.

// ---------------------------------------------------------------------------
// Shared pieces of the overlays
// ---------------------------------------------------------------------------

/** Radix's triggers write `type="button"` on the slot wrapper span too. */
const TYPE_BUTTON: Attrs = { type: `button` }

/** An overlay's `open`: local mirror, bound path written, `change {open}`. */
function createOpenState(props: NativeProps): [Accessor<boolean>, (next: boolean) => void] {
  const [open, setOpen] = createBoundState(
    () => props.node,
    () => props.scope,
    `open`,
    () => bool(props.props.open)
  )
  const change = (next: boolean) => {
    setOpen(next)
    void props.emit(`change`, { open: next }, { open: next })
  }
  return [open, change]
}

/** Round 2 §7: an overlay is LAYOUT-TRANSPARENT — its frame is its
 *  trigger's; without a trigger it lays out nothing. */
function overlayRootProps(rootProps: Record<string, unknown>, trigger: boolean, triggerNode?: { component: string } | null): Record<string, unknown> {
  if (!trigger) return { ...rootProps, "data-xui-overlay-root": `none` }
  return { ...rootProps, "data-xui-overlay-root": triggerNode?.component === `Link` ? `stretch` : `` }
}

// ---------------------------------------------------------------------------
// Dialog + Drawer
// ---------------------------------------------------------------------------

export function DialogNative(props: NativeProps): JSX.Element {
  return <ModalNative {...props} kind="Dialog" />
}
export function DrawerNative(props: NativeProps): JSX.Element {
  return <ModalNative {...props} kind="Drawer" />
}

function ModalNative(props: NativeProps & { kind: `Dialog` | `Drawer` }): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const [open, setOpen] = createOpenState(props)
  const dismissible = () => props.props.dismissible !== false
  const side = () => str(props.props.side, `bottom`)
  const title = () => str(props.props.title)
  const description = () => str(props.props.description)
  const contentId = createUniqueId()
  const titleId = createUniqueId()
  const descriptionId = createUniqueId()
  const onOpenChange = (next: boolean) => (next || untrack(dismissible) ? setOpen(next) : undefined)
  const close = () => setOpen(false)
  const drag = createDragToDismiss(side, () => props.kind === `Drawer` && props.props.dragToDismiss !== false && dismissible(), () => setOpen(false), () => ctx.reducedMotion)
  const hasTrigger = () => Boolean(props.node.slots?.trigger)
  let triggerEl: HTMLSpanElement | undefined
  const state = () => (open() ? `open` : `closed`)
  const overlayPresence = createPresence(open)
  const contentPresence = createPresence(open)

  const Content = (): JSX.Element => {
    const layer = createLayer({
      disableOutsidePointerEvents: () => true,
      onEscapeKeyDown: (e) => {
        if (!untrack(dismissible)) e.preventDefault()
      },
      onPointerDownOutside: (e) => {
        const original = e.detail.originalEvent as PointerEvent
        const ctrlLeftClick = original.button === 0 && original.ctrlKey === true
        if (original.button === 2 || ctrlLeftClick) e.preventDefault()
        if (!untrack(dismissible)) e.preventDefault()
      },
      onFocusOutside: (e) => e.preventDefault(),
      onInteractOutside: (e) => {
        if (!untrack(dismissible)) e.preventDefault()
      },
      onDismiss: () => onOpenChange(false),
    })
    const scope = createFocusScope({
      trapped: () => true,
      loop: true,
      onMountAutoFocus: (e) => {
        // A part marked `accessibility.autoFocus` (contract §6: the
        // AlertDialog's cancel) takes the focus, else the cancel action.
        const content = e.currentTarget as HTMLElement | null
        const marked = content?.querySelector<HTMLElement>(`[data-xui-autofocus]`) ?? content?.querySelector<HTMLElement>(`[data-xui-part="AlertDialog/cancel"]`)
        const target = marked && (marked.matches(`button,input,textarea,select,a[href],[tabindex]`) ? marked : marked.querySelector<HTMLElement>(`button,input,textarea,select,a[href],[tabindex]`))
        if (target) {
          e.preventDefault()
          target.focus()
        }
      },
      onUnmountAutoFocus: (e) => {
        e.preventDefault()
        triggerEl?.focus()
      },
    })
    const [registered, setRegistered] = createSignal(false)
    const el = (
      <div
        {...part(`content`, open() && `open`)}
        data-side={props.kind === `Drawer` ? side() : undefined}
        data-xui-overlay={props.kind}
        dir={ctx.direction}
        role={props.kind === `Dialog` && !dismissible() ? `alertdialog` : `dialog`}
        id={contentId}
        aria-describedby={description() ? descriptionId : undefined}
        aria-labelledby={titleId}
        data-state={state()}
        tabindex="-1"
        onKeyDown={scope.onKeyDown}
      >
        {props.kind === `Drawer` && (side() === `bottom` || side() === `top`) ? <div {...part(`handle`)} aria-hidden="true" onPointerDown={drag.onPointerDown} onPointerMove={drag.onPointerMove} onPointerUp={drag.onPointerUp} on:pointercancel={drag.onPointerCancel} /> : null}
        {title() || description() ? (
          props.kind === `Drawer` ? (
            <div class="xui-overlay-head" onPointerDown={drag.onPointerDown} onPointerMove={drag.onPointerMove} onPointerUp={drag.onPointerUp} on:pointercancel={drag.onPointerCancel}>
              <ModalHead part={part} title={title()} description={description()} titleId={titleId} descriptionId={descriptionId} fallback={ctx.t(`dialog`)} />
            </div>
          ) : (
            <div class="xui-overlay-head">
              <ModalHead part={part} title={title()} description={description()} titleId={titleId} descriptionId={descriptionId} fallback={ctx.t(`dialog`)} />
            </div>
          )
        ) : (
          <h2 class="xui-sr-only" id={titleId}>
            {ctx.t(`dialog`)}
          </h2>
        )}
        {/* Round 4: a valid submit closes THIS overlay when it is the
            nearest one around the form and dismissible (an AlertDialog,
            dismissible false, never auto-closes). */}
        <CloseOnSubmitContext.Provider value={() => (untrack(dismissible) ? close() : undefined)}>
          <LayerContext.Provider value={layer.layer}>
            <div class="xui-overlay-body">{props.children}</div>
            <Show when={props.node.slots?.footer}>
              <div {...part(`footer`)}>{props.slots.footer}</div>
            </Show>
          </LayerContext.Provider>
        </CloseOnSubmitContext.Provider>
        {props.kind === `Dialog` && dismissible() ? (
          <button type="button" {...part(`close`)} aria-label={ctx.t(`close`)} onClick={() => onOpenChange(false)}>
            <BuiltinIcon slot="Dialog.close" />
          </button>
        ) : null}
      </div>
    ) as HTMLDivElement
    contentPresence.ref(el)
    drag.setPanel(el)
    bindStyle(el, () => ({ "pointer-events": layer.pointerEvents(registered()) }))
    onMount(() => {
      layer.attach(el)
      setRegistered(true)
      scope.attach(el)
      useFocusGuards()
      useModalEffects(el, false)
    })
    return el
  }

  const Overlay = (): JSX.Element => {
    const el = (<div {...part(`overlay`, open() && `open`)} data-xui-overlay-backdrop={props.kind} data-state={state()} />) as HTMLDivElement
    overlayPresence.ref(el)
    bindStyle(el, () => ({ "pointer-events": `auto` }))
    // react-remove-scroll sits on the Dialog's overlay.
    onMount(() => useScrollLock())
    return el
  }

  return (
    <>
      <div {...overlayRootProps(props.rootProps, hasTrigger(), props.node.slots?.trigger)}>
        <Show when={hasTrigger()}>
          <span ref={triggerEl} class="xui-trigger" {...TYPE_BUTTON} aria-haspopup="dialog" aria-expanded={open()} aria-controls={contentId} data-state={state()} onClick={() => onOpenChange(!untrack(open))}>
            {props.slots.trigger}
          </span>
        </Show>
      </div>
      {/* Radix: one portal per part (the overlay's, then the content's). */}
      <LayerPortal mount={ctx.portal}>
        <Show when={overlayPresence.mounted()}>
          <Overlay />
        </Show>
      </LayerPortal>
      <LayerPortal mount={ctx.portal}>
        <Show when={contentPresence.mounted()}>
          <Content />
        </Show>
      </LayerPortal>
    </>
  )
}

function ModalHead(props: { part: PartFn; title: string; description: string; titleId: string; descriptionId: string; fallback: string }): JSX.Element {
  return (
    <>
      {props.title ? (
        <h2 {...props.part(`title`)} id={props.titleId}>
          {props.title}
        </h2>
      ) : (
        <h2 class="xui-sr-only" id={props.titleId}>
          {props.fallback}
        </h2>
      )}
      {props.description ? (
        <p {...props.part(`description`)} id={props.descriptionId}>
          {props.description}
        </p>
      ) : null}
    </>
  )
}

// ---------------------------------------------------------------------------
// Popover
// ---------------------------------------------------------------------------

const HOVER_OPEN_MS = 200
const HOVER_CLOSE_MS = 150

export function PopoverNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const [open, setOpen] = createOpenState(props)
  const side = () => str(props.props.side, `bottom`) as Side
  // `openOn: hover` (a hover card): opens on pointer hover AND keyboard focus
  // of the trigger, stays while the pointer is over the content; touch
  // (no hover pointer) keeps press.
  const hoverMode = () => props.props.openOn === `hover` && ctx.hover
  const contentId = createUniqueId()
  const hasTrigger = () => Boolean(props.node.slots?.trigger)
  let timer: ReturnType<typeof setTimeout> | undefined
  // THIS popover's content (focus moving from the trigger into it keeps it open).
  let contentEl: HTMLElement | undefined
  const schedule = (next: boolean) => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(
      () => {
        if (untrack(open) !== next) setOpen(next)
      },
      next ? HOVER_OPEN_MS : HOVER_CLOSE_MS
    )
  }
  onCleanup(() => timer && clearTimeout(timer))
  const [anchor, setAnchor] = createSignal<HTMLElement>()
  const state = () => (open() ? `open` : `closed`)
  const presence = createPresence(open)

  const Content = (): JSX.Element => {
    let hasInteractedOutside = false
    let hasPointerDownOutside = false
    const layer = createLayer({
      onInteractOutside: (e) => {
        if (!e.defaultPrevented) {
          hasInteractedOutside = true
          if (e.detail.originalEvent.type === `pointerdown`) hasPointerDownOutside = true
        }
        const target = e.target as Node | null
        if (target && anchor()?.contains(target) && hasTrigger()) e.preventDefault()
        if (e.detail.originalEvent.type === `focusin` && hasPointerDownOutside) e.preventDefault()
      },
      onDismiss: () => setOpen(false),
    })
    const scope = createFocusScope({
      trapped: () => false,
      loop: true,
      onMountAutoFocus: (e) => {
        if (untrack(hoverMode)) e.preventDefault()
      },
      onUnmountAutoFocus: (e) => {
        if (!hasInteractedOutside && hasTrigger()) anchor()?.focus()
        e.preventDefault()
        hasInteractedOutside = false
        hasPointerDownOutside = false
      },
    })
    const [registered, setRegistered] = createSignal(false)
    return (
      <Popper anchor={anchor()} side={side()} align="center" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} dir={ctx.direction}>
        {(popper) => {
          const el = (
            <div
              {...part(`content`, open() && `open`)}
              data-side={popper.placedSide()}
              data-align={popper.placedAlign()}
              data-state={state()}
              role="dialog"
              id={contentId}
              data-xui-overlay="Popover"
              data-open-on={hoverMode() ? `hover` : undefined}
              dir={ctx.direction}
              tabindex="-1"
              onKeyDown={scope.onKeyDown}
              onPointerEnter={() => hoverMode() && schedule(true)}
              onPointerLeave={() => hoverMode() && schedule(false)}
            >
              {/* The nearest overlay decides: a Form in a Popover closes nothing,
                  not even a Dialog around the Popover. */}
              <CloseOnSubmitContext.Provider value={null}>
                <LayerContext.Provider value={layer.layer}>{props.children}</LayerContext.Provider>
              </CloseOnSubmitContext.Provider>
            </div>
          ) as HTMLDivElement
          contentEl = el
          presence.ref(el)
          popper.setContent(el)
          bindStyle(el, () => popperContentStyle(`popover`, `trigger`, layer.pointerEvents(registered()), popper.isPositioned()))
          onMount(() => {
            layer.attach(el)
            setRegistered(true)
            scope.attach(el)
            useFocusGuards()
          })
          return el
        }}
      </Popper>
    )
  }

  return (
    <>
      <div {...overlayRootProps(props.rootProps, hasTrigger(), props.node.slots?.trigger)}>
        <Show when={hasTrigger()} fallback={<span ref={setAnchor} style={css({ display: `inline-flex` })} />}>
          <span
            ref={setAnchor}
            class="xui-trigger"
            {...TYPE_BUTTON}
            aria-haspopup="dialog"
            aria-expanded={open()}
            aria-controls={contentId}
            data-state={state()}
            onClick={() => setOpen(!untrack(open))}
            onPointerEnter={(e) => hoverMode() && e.pointerType !== `touch` && schedule(true)}
            onPointerLeave={(e) => hoverMode() && e.pointerType !== `touch` && schedule(false)}
            onFocusIn={() => hoverMode() && schedule(true)}
            onFocusOut={(e) => {
              if (!hoverMode()) return
              const to = e.relatedTarget as Node | null
              if (!to || !contentEl?.contains(to)) schedule(false)
            }}
          >
            {props.slots.trigger}
          </span>
        </Show>
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <Content />
        </Show>
      </LayerPortal>
    </>
  )
}

// ---------------------------------------------------------------------------
// Tooltip
// ---------------------------------------------------------------------------

const TOOLTIP_OPEN = `tooltip.open`
const TOOLTIP_DELAY_MS = 300
const TOOLTIP_SKIP_DELAY_MS = 300

/** Tooltip: shows on hover AND keyboard focus (a11y.json), Escape hides. */
export function TooltipNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const side = () => str(props.props.side, `top`) as Side
  const content = () => str(props.props.content) || str(props.props.text)
  const contentId = createUniqueId()
  const [open, setOpenState] = createSignal(false)
  const [delayed, setDelayed] = createSignal(false)
  let openDelayed = true
  let openTimer: ReturnType<typeof setTimeout> | undefined
  let skipTimer: ReturnType<typeof setTimeout> | undefined
  let pointerMoveOpened = false
  let pointerDown = false
  let anchorEl: HTMLDivElement | undefined
  let contentEl: HTMLElement | undefined
  const setOpen = (next: boolean) => {
    if (next) {
      clearTimeout(skipTimer)
      openDelayed = false
      document.dispatchEvent(new CustomEvent(TOOLTIP_OPEN))
    } else {
      clearTimeout(skipTimer)
      skipTimer = setTimeout(() => (openDelayed = true), TOOLTIP_SKIP_DELAY_MS)
    }
    setOpenState(next)
  }
  const handleOpen = () => {
    clearTimeout(openTimer)
    setDelayed(false)
    setOpen(true)
  }
  const handleClose = () => {
    clearTimeout(openTimer)
    if (untrack(open)) setOpen(false)
  }
  const handleDelayedOpen = () => {
    clearTimeout(openTimer)
    openTimer = setTimeout(() => {
      setDelayed(true)
      setOpen(true)
    }, TOOLTIP_DELAY_MS)
  }
  onCleanup(() => {
    clearTimeout(openTimer)
    clearTimeout(skipTimer)
  })
  const stateAttribute = () => (open() ? (delayed() ? `delayed-open` : `instant-open`) : `closed`)
  // Hoverable content: leaving the trigger toward the content keeps it open.
  let grace: Point[] | null = null
  const trackGrace = (e: PointerEvent) => {
    if (!grace) return
    const target = e.target as Node | null
    const point = { x: e.clientX, y: e.clientY }
    const overTrigger = anchorEl?.contains(target) ?? false
    const overContent = contentEl?.contains(target) ?? false
    if (overTrigger || overContent) grace = null
    else if (!inPolygon(point, grace)) {
      grace = null
      handleClose()
    }
  }
  // Radix tracks the grace area from an effect after the leave: the
  // pointer move that caused the leave itself is never judged.
  const leave = (e: PointerEvent, from: HTMLElement | undefined, to: HTMLElement | undefined) => {
    if (!from || !to || !untrack(open)) return
    const area = graceArea({ x: e.clientX, y: e.clientY }, from.getBoundingClientRect(), to.getBoundingClientRect())
    setTimeout(() => (grace = area), 0)
  }
  document.addEventListener(`pointermove`, trackGrace)
  onCleanup(() => document.removeEventListener(`pointermove`, trackGrace))
  const presence = createPresence(open)

  const Content = (): JSX.Element => {
    const layer = createLayer({ onFocusOutside: (e) => e.preventDefault(), onDismiss: handleClose })
    const [registered, setRegistered] = createSignal(false)
    const onOtherOpen = () => handleClose()
    const onScroll = (e: Event) => {
      const target = e.target as Node | null
      if (target && anchorEl && (target as Node).contains?.(anchorEl)) handleClose()
    }
    return (
      <Popper anchor={anchorEl} side={side()} align="center" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING}>
        {(popper) => {
          const el = (
            <div {...part(`content`)} data-side={popper.placedSide()} data-align={popper.placedAlign()} data-state={stateAttribute()} data-xui-overlay="Tooltip" onPointerLeave={(e) => leave(e, contentEl, anchorEl)}>
              {content()}
              <VisuallyHidden id={contentId}>{content()}</VisuallyHidden>
            </div>
          ) as HTMLDivElement
          contentEl = el
          presence.ref(el)
          popper.setContent(el)
          bindStyle(el, () => popperContentStyle(`tooltip`, `trigger`, layer.pointerEvents(registered()), popper.isPositioned()))
          onMount(() => {
            layer.attach(el)
            setRegistered(true)
            // Another tooltip opening closes this one; so does scrolling the trigger away.
            setTimeout(() => document.addEventListener(TOOLTIP_OPEN, onOtherOpen), 0)
            window.addEventListener(`scroll`, onScroll, { capture: true })
            onCleanup(() => {
              document.removeEventListener(TOOLTIP_OPEN, onOtherOpen)
              window.removeEventListener(`scroll`, onScroll, { capture: true })
            })
          })
          return el
        }}
      </Popper>
    )
  }

  // The tooltip wraps its children in the ANCHOR (`<id>.anchor`), the frame
  // it shows against; the root is layout-transparent around it.
  return (
    <>
      <div {...props.rootProps} data-xui-overlay-root="">
        <div
          ref={anchorEl}
          {...part(`anchor`)}
          tabindex="0"
          aria-describedby={open() ? contentId : undefined}
          data-state={stateAttribute()}
          onPointerMove={(e) => {
            if (e.pointerType === `touch`) return
            if (!pointerMoveOpened && !grace) {
              if (openDelayed) handleDelayedOpen()
              else handleOpen()
              pointerMoveOpened = true
            }
          }}
          onPointerLeave={(e) => {
            clearTimeout(openTimer)
            pointerMoveOpened = false
            leave(e, anchorEl, contentEl)
          }}
          onPointerDown={() => {
            if (untrack(open)) handleClose()
            pointerDown = true
            document.addEventListener(`pointerup`, () => setTimeout(() => (pointerDown = false), 0), { once: true })
          }}
          onFocusIn={() => {
            if (!pointerDown) handleOpen()
          }}
          onFocusOut={handleClose}
          onClick={handleClose}
        >
          {props.children}
        </div>
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <Content />
        </Show>
      </LayerPortal>
    </>
  )
}

// ---------------------------------------------------------------------------
// Menu (round 3: ONE native for press and context menus)
// ---------------------------------------------------------------------------

interface MenuItem {
  label?: string
  value?: string
  kind?: `item` | `checkbox` | `separator` | `label` | `submenu`
  icon?: string
  shortcut?: string
  checked?: boolean
  destructive?: boolean
  disabled?: boolean
  separator?: boolean
  items?: MenuItem[]
  /** EXP-1259: the submenu is a search list of its entries. */
  searchable?: boolean
  placeholder?: string
}

interface MenuRoot {
  close: () => void
  dir: () => `ltr` | `rtl`
  overlay: string
  openOn: string
  part: PartFn
  emit: NativeProps[`emit`]
  scope: () => string
}
const MenuRootContext = createContext<MenuRoot>()
/** The menu content's `--radix-<kind>-content-*` prefix. */
const menuKind = (root: MenuRoot) => (root.openOn === `contextmenu` ? `context-menu` : `dropdown-menu`)

function MenuEntry(props: { item: MenuItem; raw: Record<string, unknown> | undefined; path: string }): JSX.Element {
  const root = useContext(MenuRootContext)!
  const kind = () => (props.item.separator ? `separator` : (props.item.kind ?? `item`))
  return (
    <Dynamic
      component={(() => {
        const k = kind()
        if (k === `separator`) return MenuSeparator
        if (k === `label`) return MenuLabel
        if (k === `submenu`) return MenuSub
        if (k === `checkbox`) return MenuCheckbox
        return MenuPlainItem
      })()}
      item={props.item}
      raw={props.raw}
      path={props.path}
      root={root}
    />
  )
}

type EntryProps = { item: MenuItem; raw: Record<string, unknown> | undefined; path: string; root: MenuRoot }

function MenuSeparator(props: EntryProps): JSX.Element {
  return <div role="separator" aria-orientation="horizontal" {...props.root.part(`separator`)} />
}
function MenuLabel(props: EntryProps): JSX.Element {
  return <div {...props.root.part(`label`)}>{str(props.item.label)}</div>
}

function ItemIcon(props: { name?: string }): JSX.Element {
  const ctx = useSurfaceContext()
  return <Show when={props.name}>{(name) => <IconGlyph icons={ctx.host.icons} name={name()} />}</Show>
}
function Shortcut(props: { text?: string; part: PartFn }): JSX.Element {
  return <Show when={props.text}>{(t) => <span {...props.part(`shortcut`)}>{t()}</span>}</Show>
}

function MenuPlainItem(props: EntryProps): JSX.Element {
  const disabled = () => bool(props.item.disabled)
  const row = createSelectable(disabled, () => void props.root.emit(`select`, { value: props.item.value ?? props.item.label }), props.root.close)
  return (
    <div
      ref={row.ref}
      role="menuitem"
      {...props.root.part(`item`, disabled() && `disabled`)}
      data-destructive={props.item.destructive ? `true` : undefined}
      {...row.attrs()}
      onClick={row.handlers.onClick}
      onPointerDown={row.handlers.onPointerDown}
      onPointerUp={row.handlers.onPointerUp}
      onPointerMove={row.handlers.onPointerMove}
      onPointerLeave={row.handlers.onPointerLeave}
      onKeyDown={row.handlers.onKeyDown}
      onFocus={row.handlers.onFocus}
      onBlur={row.handlers.onBlur}
    >
      <ItemIcon name={props.item.icon} />
      <span class="xui-menu-label">{str(props.item.label)}</span>
      <Shortcut text={props.item.shortcut} part={props.root.part} />
    </div>
  )
}

function MenuCheckbox(props: EntryProps): JSX.Element {
  const ctx = useSurfaceContext()
  const disabled = () => bool(props.item.disabled)
  const checked = () => bool(props.item.checked)
  const row = createSelectable(disabled, (e) => {
    const next = !untrack(checked)
    const raw = props.raw?.checked
    if (isBinding(raw)) ctx.setData(absolutePath(raw.path, props.root.scope()), next)
    void props.root.emit(`select`, { value: props.item.value ?? props.item.label, checked: next })
    e.preventDefault()
  }, props.root.close)
  return (
    <div
      ref={row.ref}
      role="menuitemcheckbox"
      aria-checked={checked() ? `true` : `false`}
      {...props.root.part(`item`, disabled() && `disabled`, checked() && `checked`)}
      data-state={checked() ? `checked` : `unchecked`}
      {...row.attrs()}
      onClick={row.handlers.onClick}
      onPointerDown={row.handlers.onPointerDown}
      onPointerUp={row.handlers.onPointerUp}
      onPointerMove={row.handlers.onPointerMove}
      onPointerLeave={row.handlers.onPointerLeave}
      onKeyDown={row.handlers.onKeyDown}
      onFocus={row.handlers.onFocus}
      onBlur={row.handlers.onBlur}
    >
      <span {...props.root.part(`check`)}>
        <Show when={checked()}>
          <span data-state="checked">
            <BuiltinIcon slot="Menu.check" />
          </span>
        </Show>
      </span>
      <span class="xui-menu-label">{str(props.item.label)}</span>
      <Shortcut text={props.item.shortcut} part={props.root.part} />
    </div>
  )
}

function MenuSub(props: EntryProps): JSX.Element {
  const ctx = useSurfaceContext()
  const disabled = () => bool(props.item.disabled)
  const row = createMenuRow(disabled)
  const [open, setOpen] = createSignal(false)
  const triggerId = createUniqueId()
  const contentId = createUniqueId()
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const [content, setContent] = createSignal<HTMLElement>()
  let openTimer: ReturnType<typeof setTimeout> | undefined
  let graceTimer: ReturnType<typeof setTimeout> | undefined
  onCleanup(() => {
    clearTimeout(openTimer)
    clearTimeout(graceTimer)
  })
  const rtl = () => props.root.dir() === `rtl`
  const openKeys = () => [`Enter`, ` `, rtl() ? `ArrowLeft` : `ArrowRight`]
  const closeKey = () => (rtl() ? `ArrowRight` : `ArrowLeft`)
  const presence = createPresence(open)
  return (
    <>
      <div
        ref={(el) => {
          row.ref(el)
          setTrigger(el)
        }}
        role="menuitem"
        id={triggerId}
        aria-haspopup="menu"
        aria-expanded={open() ? `true` : `false`}
        aria-controls={contentId}
        data-state={open() ? `open` : `closed`}
        {...props.root.part(`item`, disabled() && `disabled`)}
        {...row.attrs()}
        onClick={() => {
          if (!untrack(disabled) && !untrack(open)) setOpen(true)
        }}
        onPointerMove={(e: PointerEvent) => {
          row.handlers.onPointerMove(e)
          if (e.pointerType !== `mouse`) return
          if (!untrack(disabled) && !untrack(open) && !openTimer) {
            row.level.setGrace(null)
            openTimer = setTimeout(() => {
              setOpen(true)
              openTimer = undefined
            }, 100)
          }
        }}
        onPointerLeave={(e: PointerEvent) => {
          if (e.pointerType !== `mouse`) return
          clearTimeout(openTimer)
          openTimer = undefined
          const rect = content()?.getBoundingClientRect()
          if (rect) {
            const s = content()?.dataset.side as Side
            const right = s === `right`
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
              side: s,
            })
            clearTimeout(graceTimer)
            graceTimer = setTimeout(() => row.level.setGrace(null), 300)
          } else {
            if (row.level.onTriggerLeave(e)) return
            row.level.setGrace(null)
            row.handlers.onPointerLeave(e)
          }
        }}
        onKeyDown={(e: KeyboardEvent) => {
          const typingAhead = row.level.search() !== ``
          if (untrack(disabled) || (typingAhead && e.key === ` `)) return
          if (openKeys().includes(e.key)) {
            if (untrack(open)) content()?.focus({ preventScroll: true })
            else setOpen(true)
            e.preventDefault()
            return
          }
          row.handlers.onKeyDown(e)
        }}
        onFocus={row.handlers.onFocus}
        onBlur={row.handlers.onBlur}
      >
        <ItemIcon name={props.item.icon} />
        <span class="xui-menu-label">{str(props.item.label)}</span>
        <span {...props.root.part(`submenuIndicator`)}>
          <BuiltinIcon slot="Menu.submenuIndicator" />
        </span>
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <MenuContent
            anchor={trigger()}
            side={rtl() ? `left` : `right`}
            align="start"
            sideOffset={2}
            dir={props.root.dir()}
            kind={menuKind(props.root)}
            root={false}
            id={contentId}
            labelledBy={triggerId}
            state={() => (open() ? `open` : `closed`)}
            attrs={() => ({ ...props.root.part(`content`), "data-xui-overlay": `${props.root.overlay}.sub`, "data-searchable": props.item.searchable ? `true` : undefined })}
            setContent={(el) => {
              setContent(el)
              if (el) presence.ref(el)
            }}
            onDismiss={() => setOpen(false)}
            onEscapeKeyDown={(e) => {
              props.root.close()
              e.preventDefault()
            }}
            onFocusOutside={(e) => {
              if (e.target !== trigger()) setOpen(false)
            }}
            onCloseAutoFocus={(e) => e.preventDefault()}
            onKeyDown={(e) => {
              const el = content()
              if (el?.contains(e.target as Node) && e.key === closeKey()) {
                setOpen(false)
                trigger()?.focus()
                e.preventDefault()
              }
            }}
          >
            <Show when={props.item.searchable} fallback={<MenuEntries items={objects<MenuItem>(props.item.items)} raws={objects<Record<string, unknown>>(props.raw?.items)} path={`${props.path}.`} />}>
              <MenuSearch item={props.item} i={props.path} />
            </Show>
          </MenuContent>
        </Show>
      </LayerPortal>
    </>
  )
}

/** EXP-1259: a searchable submenu = the search list (head, the entries that
 *  match as menu rows, the highlighted one `selected`, the fade): its field
 *  takes the focus when it opens (and when the pointer enters), ↑/↓ move,
 *  Enter or Tab pick (the menu closes), ← at the caret's start returns to
 *  the parent row. The list itself is the SearchList's body (search.tsx). */
function MenuSearch(props: { item: MenuItem; i: string }): JSX.Element {
  const ctx = useSurfaceContext()
  const root = useContext(MenuRootContext)!
  const [query, setQuery] = createSignal(``)
  let box: HTMLDivElement | undefined
  // The field takes the focus once the opened content has it.
  onMount(() => {
    const frame = requestAnimationFrame(() => box?.querySelector<HTMLInputElement>(`input`)?.focus({ preventScroll: true }))
    onCleanup(() => cancelAnimationFrame(frame))
  })
  const rows = createMemo(() =>
    objects<MenuItem>(props.item.items)
      .map((entry, index) => ({ index, entry }))
      .filter(({ entry }) => entry.kind !== `separator` && entry.kind !== `label` && !entry.separator && searchMatches({ label: str(entry.label) }, query()))
      .map(({ index, entry }) => ({ index, item: { label: str(entry.label), value: str(entry.value ?? entry.label), icon: entry.icon, disabled: bool(entry.disabled) } as SearchItem }))
  )
  return (
    <div
      ref={box}
      class="xui-menu-search"
      onPointerEnter={(e) => {
        const input = e.currentTarget.querySelector<HTMLInputElement>(`input`)
        if (input && !e.currentTarget.contains(document.activeElement)) input.focus({ preventScroll: true })
      }}
    >
      <SearchBody
        part={root.part}
        owner="Menu"
        suffix={`.${props.i}`}
        rows={rows()}
        query={query()}
        onQuery={setQuery}
        placeholder={str(props.item.placeholder) || ctx.t(`search`)}
        emptyText={ctx.t(`noResults`)}
        maxRows={8}
        showCount
        autoFocus
        leaveBubbles
        onPick={(picked) => {
          void root.emit(`select`, { value: picked.value })
          root.close()
        }}
        rowPart={(_, __, active) => root.part(`item`, active && `selected`)}
        renderRow={(row) => (
          <>
            {row.icon ? <IconGlyph icons={ctx.host.icons} name={row.icon} /> : null}
            <span class="xui-menu-label">{row.label}</span>
          </>
        )}
      />
    </div>
  )
}

/** The entries of one level. A submenu's `items` may be a binding (`{path}`
 *  to rows a host source feeds): the bind pass resolved it already. */
function MenuEntries(props: { items: MenuItem[]; raws: Record<string, unknown>[]; path: string }): JSX.Element {
  const indexes = createMemo(() => props.items.map((_, i) => i), undefined, { equals: (a, b) => a.length === b.length })
  return <For each={indexes()}>{(i) => <MenuEntry item={props.items[i]} raw={props.raws[i]} path={`${props.path}${i}`} />}</For>
}

/** Menu: `openOn: press` (default) = a dropdown menu, its ONE child the
 *  trigger (no child = the default outline Button from label/icon, no
 *  chevron), anchored bottom-start; `openOn: contextmenu` = a context menu
 *  over the child (right-click / long-press / Shift+F10 / the Menu key),
 *  opened at the pointer (the child's corner from the keyboard). */
export function MenuNative(props: NativeProps): JSX.Element {
  return (
    <Show when={props.props.openOn === `contextmenu`} fallback={<PressMenuOpening {...props} />}>
      <ContextMenuOpening {...props} />
    </Show>
  )
}

function createMenuRoot(props: NativeProps, part: PartFn, close: () => void, openOn: string): MenuRoot {
  const ctx = useSurfaceContext()
  useKeyboardTracking()
  return { close, dir: () => ctx.direction, overlay: `Menu`, openOn, part, emit: props.emit, scope: () => props.scope }
}

function PressMenuOpening(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const items = () => objects<MenuItem>(props.props.items)
  const label = () => str(props.props.label)
  const icon = () => str(props.props.icon)
  const [open, setOpen] = createSignal(false)
  const hasChild = () => props.node.children.length > 0
  const triggerId = createUniqueId()
  const contentId = createUniqueId()
  const [trigger, setTrigger] = createSignal<HTMLElement>()
  const root = createMenuRoot(props, part, () => setOpen(false), `press`)
  const presence = createPresence(open)
  let interactedOutside = false
  const triggerAttrs = (): Attrs => ({
    type: `button`,
    id: triggerId,
    "aria-haspopup": `menu`,
    "aria-expanded": open() ? `true` : `false`,
    "aria-controls": open() ? contentId : undefined,
    "data-state": open() ? `open` : `closed`,
  })
  const onPointerDown = (e: PointerEvent) => {
    if (e.button === 0 && e.ctrlKey === false) {
      const was = untrack(open)
      setOpen(!was)
      if (!was) e.preventDefault()
    }
  }
  const onKeyDown = (e: KeyboardEvent) => {
    if ([`Enter`, ` `].includes(e.key)) setOpen(!untrack(open))
    if (e.key === `ArrowDown`) setOpen(true)
    if ([`Enter`, ` `, `ArrowDown`].includes(e.key)) e.preventDefault()
  }
  return (
    <MenuRootContext.Provider value={root}>
      <div {...overlayRootProps(props.rootProps, true, props.node.children[0] ?? null)}>
        <Show
          when={hasChild()}
          fallback={
            // Round 2 §7: the default trigger = an outline button with the
            // icon (when set) and the label (`$string.menu` without one and
            // without an icon), no chevron.
            <button ref={setTrigger} {...triggerAttrs()} {...part(`trigger`, open() && `open`)} aria-label={label() || ctx.t(`menu`)} onPointerDown={onPointerDown} onKeyDown={onKeyDown}>
              {icon() ? <IconGlyph icons={ctx.host.icons} name={icon()} class="xui-icon" width={16} height={16} /> : null}
              {label() || !icon() ? <span>{label() || ctx.t(`menu`)}</span> : null}
            </button>
          }
        >
          <span ref={setTrigger} class="xui-trigger" {...triggerAttrs()} onPointerDown={onPointerDown} onKeyDown={onKeyDown}>
            {props.children}
          </span>
        </Show>
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <MenuContent
            anchor={trigger()}
            side="bottom"
            align="start"
            sideOffset={OVERLAY_OFFSET}
            dir={root.dir()}
            kind={menuKind(root)}
            root
            id={contentId}
            labelledBy={triggerId}
            state={() => (open() ? `open` : `closed`)}
            attrs={() => ({ ...part(`content`, open() && `open`), "data-xui-overlay": `Menu`, "data-open-on": `press` })}
            setContent={(el) => el && presence.ref(el)}
            onDismiss={() => setOpen(false)}
            onInteractOutside={(e) => {
              const original = e.detail.originalEvent as PointerEvent
              const ctrlLeftClick = original.button === 0 && original.ctrlKey === true
              if (original.button === 2 || ctrlLeftClick) interactedOutside = true
            }}
            onCloseAutoFocus={(e) => {
              if (!interactedOutside) trigger()?.focus()
              interactedOutside = false
              e.preventDefault()
            }}
          >
            <MenuEntries items={items()} raws={objects<Record<string, unknown>>(props.node.props.items)} path="" />
          </MenuContent>
        </Show>
      </LayerPortal>
    </MenuRootContext.Provider>
  )
}

function ContextMenuOpening(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const items = () => objects<MenuItem>(props.props.items)
  const [open, setOpen] = createSignal(false)
  const [point, setPoint] = createSignal({ x: 0, y: 0 })
  const root = createMenuRoot(props, part, () => setOpen(false), `contextmenu`)
  const presence = createPresence(open)
  let longPress: ReturnType<typeof setTimeout> | undefined
  const clearLongPress = () => clearTimeout(longPress)
  onCleanup(clearLongPress)
  const openAt = (e: { clientX: number; clientY: number }) => {
    setPoint({ x: e.clientX, y: e.clientY })
    setOpen(true)
  }
  const anchor: VirtualElement = { getBoundingClientRect: () => DOMRect.fromRect({ width: 0, height: 0, ...point() }) }
  return (
    <MenuRootContext.Provider value={root}>
      <div
        {...props.rootProps}
        data-state={open() ? `open` : `closed`}
        on:contextmenu={(e: MouseEvent) => {
          clearLongPress()
          openAt(e)
          e.preventDefault()
        }}
        onPointerDown={(e: PointerEvent) => {
          if (e.pointerType === `mouse`) return
          clearLongPress()
          longPress = setTimeout(() => openAt(e), 700)
        }}
        onPointerMove={(e: PointerEvent) => e.pointerType !== `mouse` && clearLongPress()}
        on:pointercancel={(e: PointerEvent) => e.pointerType !== `mouse` && clearLongPress()}
        onPointerUp={(e: PointerEvent) => e.pointerType !== `mouse` && clearLongPress()}
        onKeyDown={(e: KeyboardEvent) => {
          if ((e.key === `F10` && e.shiftKey) || e.key === `ContextMenu`) {
            e.preventDefault()
            const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
            e.currentTarget!.dispatchEvent(new MouseEvent(`contextmenu`, { bubbles: true, cancelable: true, clientX: r.left + 8, clientY: r.top + 8 }))
          }
        }}
      >
        {props.children}
      </div>
      <LayerPortal mount={ctx.portal}>
        <Show when={presence.mounted()}>
          <MenuContent
            anchor={anchor}
            side="right"
            align="start"
            sideOffset={2}
            dir={root.dir()}
            kind={menuKind(root)}
            root
            state={() => (open() ? `open` : `closed`)}
            attrs={() => ({ ...part(`content`, open() && `open`), "data-xui-overlay": `Menu`, "data-open-on": `contextmenu` })}
            setContent={(el) => el && presence.ref(el)}
            onDismiss={() => setOpen(false)}
            onCloseAutoFocus={() => undefined}
          >
            <MenuEntries items={items()} raws={objects<Record<string, unknown>>(props.node.props.items)} path="" />
          </MenuContent>
        </Show>
      </LayerPortal>
    </MenuRootContext.Provider>
  )
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

/** Toast (contract §3): in the TOAST layer, newest nearest the edge, at most
 *  3 visible (base CSS). The renderer runs `duration` (0 = sticky), PAUSED
 *  while hovered or focused, then writes `open: false` and fires `dismiss`
 *  + `change`. `error` is assertive, the rest polite; never takes focus. */
export function ToastNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const [open, setOpenState] = createBoundState(
    () => props.node,
    () => props.scope,
    `open`,
    () => (props.props.open === undefined ? true : bool(props.props.open))
  )
  const [leaving, setLeaving] = createSignal(false)
  const [paused, setPaused] = createSignal(false)
  const duration = () => (props.props.duration === undefined ? 5000 : Math.max(0, num(props.props.duration, 5000)))
  const type = () => str(props.props.type, `info`)
  let remaining = untrack(duration)
  let startedAt = 0
  const close = () => {
    setOpenState(false)
    void props.emit(`dismiss`, undefined, { open: false })
    void props.emit(`change`, { open: false }, { open: false })
  }
  // Exit: keep the element for the motion duration with data-state=closed.
  const [shown, setShown] = createSignal(untrack(open))
  createEffect(
    on(open, (o) => {
      if (o) {
        setShown(true)
        setLeaving(false)
        remaining = untrack(duration)
        return
      }
      if (!untrack(shown)) return
      setLeaving(true)
      const ms = ctx.reducedMotion ? 0 : (ctx.theme.tokens.motion?.standard ?? 180)
      const t = setTimeout(() => {
        setShown(false)
        setLeaving(false)
      }, ms)
      onCleanup(() => clearTimeout(t))
    }, { defer: true })
  )
  createEffect(() => {
    if (!open() || duration() === 0 || paused()) return
    startedAt = Date.now()
    const t = setTimeout(close, remaining)
    onCleanup(() => {
      clearTimeout(t)
      remaining = Math.max(0, remaining - (Date.now() - startedAt))
    })
  })
  const assertive = () => type() === `error`
  return (
    <LayerPortal mount={shown() ? ctx.toastLayer : null}>
      <div
        {...props.rootProps}
        role={assertive() ? `alert` : `status`}
        aria-live={assertive() ? `assertive` : `polite`}
        aria-atomic="true"
        data-state={leaving() ? `closed` : `open`}
        data-xui-overlay="Toast"
        onPointerEnter={() => setPaused(true)}
        onPointerLeave={() => setPaused(false)}
        onFocusIn={() => setPaused(true)}
        onFocusOut={() => setPaused(false)}
        onKeyDown={(e: KeyboardEvent) => {
          if (e.key === `Escape` && props.props.dismissible !== false) {
            e.stopPropagation()
            close()
          }
        }}
      >
        <span {...part(`icon`)}>
          <BuiltinIcon slot={`Toast.icon.${type()}`} />
        </span>
        <div class="xui-toast-text">
          <span {...part(`title`)}>{str(props.props.title)}</span>
          {str(props.props.description) ? <span {...part(`description`)}>{str(props.props.description)}</span> : null}
        </div>
        {str(props.props.actionLabel) ? (
          <button type="button" {...part(`action`)} onClick={() => void props.emit(`action`)}>
            {str(props.props.actionLabel)}
          </button>
        ) : null}
        {props.props.dismissible !== false ? (
          <button type="button" {...part(`close`)} aria-label={ctx.t(`dismiss`)} onClick={close}>
            <BuiltinIcon slot="Toast.close" />
          </button>
        ) : null}
      </div>
    </LayerPortal>
  )
}

export const NATIVES_OVERLAYS: Record<string, Component<NativeProps>> = {
  Dialog: DialogNative,
  Drawer: DrawerNative,
  Popover: PopoverNative,
  Tooltip: TooltipNative,
  Menu: MenuNative,
  Toast: ToastNative,
}
