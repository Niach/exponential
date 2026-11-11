// VAPP-87 + round 1: the OVERLAYS — Dialog, Drawer, Popover, Tooltip,
// Menu (press | contextmenu) on Radix portals, and the Toast. Every portal
// mounts into the surface's OVERLAY LAYER inside the `xui` container (so a
// breakpoint still matches inside a Dialog, audit bug C); toasts mount into
// the TOAST layer above it. The placement contract the core mirrors: side +
// centre alignment, `OVERLAY_OFFSET` from the anchor, flip when the
// preferred side lacks room, shift to stay `OVERLAY_PADDING` inside the
// viewport (`placeOverlay`, fixtures/overlay-geometry.json). Enter/exit
// animate on the theme's motion tokens (base-css.ts; zero under reduced
// motion). A trigger slot is wrapped in an inline-flex span so Radix
// measures the slot's own box and the slot's painter keeps its element.

import { Fragment, forwardRef, type ComponentType, useCallback, useContext, useEffect, useId, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type HTMLAttributes, type PointerEvent as ReactPointerEvent, type ReactNode } from "react"
import { createPortal } from "react-dom"
import { ContextMenu as ContextPrimitive, Dialog as DialogPrimitive, DropdownMenu as MenuPrimitive, Popover as PopoverPrimitive, Slot, Tooltip as TooltipPrimitive } from "radix-ui"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential_app/eui"
import { useSurfaceContext } from "../context"
import { findAnchor, isFieldAnchorElement, hostedPopupOf, mountHostedPopup, holdDialogBottom, isBinding, linkAnchor, markAnchor, overlayOpenFocus, tooltipFocusHost, tooltipPhysicalSide, virtualAnchor, watchAnchor, withinAnchor, withinOtherOverlay, type AnchorLink, type HostedPopup, type VirtualAnchor } from "@exponential_app/eui/web"
import { absolutePath, isPickerPopover, propColor, popoverPanelWidth } from "@exponential_app/eui/web"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { ClosePickerContext, CloseOnSubmitContext } from "../form"
import { useBoundState } from "./bound"
import { bool, BuiltinIcon, num, str, useParts, type PartFn, objects } from "./shared"
import { SearchBody, searchMatches } from "./search"
import { MenuPreviewContext, MenuPreviewPanel, anyPreview, hasPreview, insidePreview, menuModalityProps, useMenuPreview, useMenuPreviewRow } from "./menu-preview"

/** A Radix Portal into the overlay layer, or (EXP-1673, `inlineOverlays`)
 *  no portal: the content paints in place. */
function OverlayPortal({ as: Portal, children, hosted = null }: { as: ComponentType<{ container?: HTMLElement; children?: ReactNode }>; children: ReactNode; hosted?: HostedContainer }) {
  const ctx = useSurfaceContext()
  // VAPP-281: the host's container (the content lays out in it statically),
  // nothing while the host is being asked.
  if (hosted === `pending`) return null
  if (hosted) return <Portal container={hosted}>{children}</Portal>
  if (ctx.inlineOverlays) return <Fragment>{children}</Fragment>
  return <Portal container={ctx.portal ?? undefined}>{children}</Portal>
}

/** An overlay's `open`: local mirror, bound path written, `change {open}`. */
function useOpenState(node: NativeProps[`node`], scope: string, external: boolean, emit: NativeProps[`emit`]) {
  const [open, setOpen] = useBoundState(node, scope, `open`, external)
  const change = useCallback(
    (next: boolean) => {
      setOpen(next)
      void emit(`change`, { open: next }, { open: next })
    },
    [setOpen, emit]
  )
  return [open, change] as const
}

/** The slot wrapper Radix anchors to and clicks through (forwardRef so the
 *  popper can measure it; the rest of the props are Radix's). The toggle runs
 *  on the way down: a Button in the slot stops its click (so it never presses
 *  a Row behind it), which must not stop the overlay it opens. */
const Trigger = forwardRef<HTMLSpanElement, HTMLAttributes<HTMLSpanElement> & { children?: ReactNode }>(function Trigger({ children, onClick, ...rest }, ref) {
  return (
    <span ref={ref} className="xui-trigger" onClickCapture={onClick} {...rest}>
      {children}
    </span>
  )
})

/** VAPP-281: where an open popup's content goes: the host's element, null
 *  (the in-surface overlay), or `pending` while the host is asked. */
type HostedContainer = HTMLElement | null | `pending`

/** VAPP-281: with `host.popupHost`, ask it for a container when the popup
 *  opens (its anchor's box in surface coordinates, side, align); tell it
 *  when it closes. */
function useHostedPopup(owner: string, kind: HostedPopup[`kind`], open: boolean, anchorEl: Element | null | undefined, side: HostedPopup[`side`], align: HostedPopup[`align`]): HostedContainer {
  const ctx = useSurfaceContext()
  const host = ctx.host.popupHost
  const [asked, setAsked] = useState<{ el: HTMLElement | null } | null>(null)
  // Asked again when the anchor's element changes (an `anchor` node is found
  // after the first commit).
  useLayoutEffect(() => {
    if (!host || !open) return
    const popup = hostedPopupOf(owner, kind, anchorEl, side, align, ctx.direction)
    setAsked({ el: mountHostedPopup(host, popup, anchorEl) })
    return () => {
      host.unmount?.(popup)
      setAsked(null)
    }
  }, [host, open, owner, kind, anchorEl, side, align, ctx.direction])
  if (!host || !open) return null
  return asked ? asked.el : `pending`
}

/** VAPP-276: the element of the overlay's `anchor` node (found after every
 *  render, so a node painted later in the tree is picked up), wired as the
 *  overlay's trigger while it is mounted (`linkAnchor`), its open state
 *  mirrored (`markAnchor`). `null` without an `anchor` prop. */
function useAnchor(id: string, from: { current: Element | null }, link: AnchorLink, open: boolean, contentId: string) {
  const ctx = useSurfaceContext()
  const [el, setEl] = useState<HTMLElement | null>(null)
  const linkRef = useRef(link)
  linkRef.current = link
  useLayoutEffect(() => {
    const found = id ? findAnchor(from.current ?? ctx.portal, ctx.surfaceId, id) : null
    if (found !== el) setEl(found)
  })
  useEffect(() => {
    if (!el) return
    return linkAnchor(el, {
      haspopup: link.haspopup,
      toggle: () => linkRef.current.toggle(),
      open: () => linkRef.current.open?.(),
      hover: link.hover ? { active: () => linkRef.current.hover?.active() ?? false, enter: () => linkRef.current.hover?.enter(), leave: () => linkRef.current.hover?.leave() } : undefined,
    })
  }, [el, link.haspopup, !!link.hover])
  useEffect(() => {
    if (el) markAnchor(el, open, contentId)
  }, [el, open, contentId])
  const elRef = useRef<HTMLElement | null>(null)
  elRef.current = el
  const virtual = useMemo(() => virtualAnchor(() => elRef.current), [])
  // Radix's `virtualRef` is a ref object.
  const virtualRef = useMemo(() => ({ current: virtual }), [virtual])
  return { el, virtual, virtualRef }
}

/** #EXP-1739 (overlayAnchor): while `open`, the overlay closes once its
 *  anchor `el` scrolls, moves or goes (watched from the element it resolved
 *  to; a newly resolved element starts a new watch). */
function useCloseOnAnchorLost(open: boolean, el: Element | null | undefined, close: () => void) {
  const ctx = useSurfaceContext()
  const closeRef = useRef(close)
  closeRef.current = close
  useEffect(() => {
    if (!open || ctx.inlineOverlays || !el) return
    return watchAnchor(() => el, () => closeRef.current())
  }, [open, el, ctx.inlineOverlays])
}

/** VAPP-276: the popper reference a Radix DropdownMenu Trigger measures and
 *  focuses: a forwarded VIRTUAL element following the anchor (the trigger
 *  paints nothing of its own). */
const AnchorProxy = forwardRef<unknown, { anchor: VirtualAnchor }>(function AnchorProxy({ anchor }, ref) {
  useImperativeHandle(ref, () => anchor, [anchor])
  return null
})

/** Round 2 §7: an overlay is LAYOUT-TRANSPARENT — its frame is its
 *  trigger's (the trigger is the flex item: a Link stretches in a stretch
 *  row, a Button keeps its control height; the overlay never fills a column
 *  or stretches a row on its own); without a trigger it lays out nothing. */
function overlayRootProps(rootProps: Record<string, unknown>, trigger: NativeProps[`slots`][string] | undefined | null, triggerNode?: { component: string } | null): Record<string, unknown> {
  if (!trigger) return { ...rootProps, "data-xui-overlay-root": `none` }
  return { ...rootProps, "data-xui-overlay-root": triggerNode?.component === `Link` ? `stretch` : `` }
}

export function DialogNative(p: NativeProps) {
  return <ModalNative {...p} kind="Dialog" />
}
export function DrawerNative(p: NativeProps) {
  return <ModalNative {...p} kind="Drawer" />
}

/** Drawer drag-to-dismiss: a drag toward the drawer's edge past 80 px (or a
 *  flick) closes it; anything less springs back. */
function useDragToDismiss(side: string, enabled: boolean, onDismiss: () => void, reducedMotion: boolean) {
  const panel = useRef<HTMLDivElement | null>(null)
  const drag = useRef<{ id: number; x: number; y: number; at: number } | null>(null)
  const sign = side === `bottom` || side === `right` ? 1 : -1
  const axis = side === `left` || side === `right` ? `x` : `y`
  const distance = (e: ReactPointerEvent) => {
    const d = drag.current
    if (!d) return 0
    return Math.max(0, ((axis === `x` ? e.clientX - d.x : e.clientY - d.y) * sign))
  }
  const paint = (px: number, animate: boolean) => {
    const el = panel.current
    if (!el) return
    el.style.transition = animate && !reducedMotion ? `transform var(--xui-motion-standard) var(--xui-ease-decelerate)` : `none`
    el.style.transform = px ? `translate${axis === `x` ? `X` : `Y`}(${px * sign}px)` : ``
  }
  const handlers = enabled
    ? {
        onPointerDown: (e: ReactPointerEvent) => {
          if (e.button !== 0) return
          drag.current = { id: e.pointerId, x: e.clientX, y: e.clientY, at: performance.now() }
          ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
        },
        onPointerMove: (e: ReactPointerEvent) => {
          if (drag.current?.id === e.pointerId) paint(distance(e), false)
        },
        onPointerUp: (e: ReactPointerEvent) => {
          if (drag.current?.id !== e.pointerId) return
          const px = distance(e)
          const dt = performance.now() - drag.current.at
          drag.current = null
          if (px > 80 || (px > 24 && dt > 0 && px / dt > 0.6)) {
            paint(0, false)
            onDismiss()
          } else paint(0, true)
        },
        onPointerCancel: () => {
          drag.current = null
          paint(0, true)
        },
      }
    : {}
  return { panel, handlers }
}

function ModalNative({ node, props, rootProps, emit, children, slots, scope, kind }: NativeProps & { kind: `Dialog` | `Drawer` }) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const [open, setOpen] = useOpenState(node, scope, bool(props.open), emit)
  const dismissible = props.dismissible !== false
  const side = str(props.side, `bottom`)
  const title = str(props.title)
  const description = str(props.description)
  const drag = useDragToDismiss(side, kind === `Drawer` && props.dragToDismiss !== false && dismissible, () => setOpen(false), ctx.reducedMotion)
  const close = useCallback(() => setOpen(false), [setOpen])
  // EXP-1676: an open Dialog holds its bottom edge (`holdDialogBottom`, the
  // core's rule); an inline (static) render stays in flow.
  const unhold = useRef<(() => void) | null>(null)
  const holds = kind === `Dialog` && !ctx.inlineOverlays
  const panelRef = useCallback(
    (el: HTMLDivElement | null) => {
      drag.panel.current = el
      unhold.current?.()
      unhold.current = el && holds ? holdDialogBottom(el) : null
    },
    [drag.panel, holds]
  )
  return (
    <DialogPrimitive.Root open={open} onOpenChange={(next) => (next || dismissible ? setOpen(next) : undefined)}>
      <div {...overlayRootProps(rootProps, slots.trigger, node.slots?.trigger)}>
        {slots.trigger ? (
          <DialogPrimitive.Trigger asChild>
            <Trigger>{slots.trigger}</Trigger>
          </DialogPrimitive.Trigger>
        ) : null}
      </div>
      <OverlayPortal as={DialogPrimitive.Portal}>
        <DialogPrimitive.Overlay {...(part(`overlay`, open && `open`) as Record<string, string>)} data-xui-overlay-backdrop={kind} />
        <DialogPrimitive.Content
          ref={panelRef}
          {...(part(`content`, open && `open`) as Record<string, string>)}
          data-side={kind === `Drawer` ? side : undefined}
          data-xui-overlay={kind}
          dir={ctx.direction}
          role={kind === `Dialog` && !dismissible ? `alertdialog` : `dialog`}
          onEscapeKeyDown={dismissible ? undefined : (e) => e.preventDefault()}
          onPointerDownOutside={dismissible ? undefined : (e) => e.preventDefault()}
          onInteractOutside={dismissible ? undefined : (e) => e.preventDefault()}
          onOpenAutoFocus={overlayOpenFocus}
          {...(description ? {} : { "aria-describedby": undefined })}
        >
          {kind === `Drawer` && (side === `bottom` || side === `top`) ? <div {...(part(`handle`) as Record<string, string>)} aria-hidden="true" {...drag.handlers} /> : null}
          {title || description ? (
            <div className="xui-overlay-head" {...(kind === `Drawer` ? drag.handlers : {})}>
              {title ? <DialogPrimitive.Title {...(part(`title`) as Record<string, string>)}>{title}</DialogPrimitive.Title> : <DialogPrimitive.Title className="xui-sr-only">{ctx.t(`dialog`)}</DialogPrimitive.Title>}
              {description ? <DialogPrimitive.Description {...(part(`description`) as Record<string, string>)}>{description}</DialogPrimitive.Description> : null}
            </div>
          ) : (
            <DialogPrimitive.Title className="xui-sr-only">{ctx.t(`dialog`)}</DialogPrimitive.Title>
          )}
          {/* Round 4: a valid submit closes THIS overlay when it is the
              nearest one around the form and dismissible (an AlertDialog,
              dismissible false, never auto-closes). */}
          <CloseOnSubmitContext.Provider value={dismissible ? close : null}>
            <div className="xui-overlay-body">{children}</div>
            {slots.footer ? <div {...(part(`footer`) as Record<string, string>)}>{slots.footer}</div> : null}
          </CloseOnSubmitContext.Provider>
          {kind === `Dialog` && dismissible ? (
            <DialogPrimitive.Close {...(part(`close`) as Record<string, string>)} aria-label={ctx.t(`close`)}>
              <BuiltinIcon slot="Dialog.close" />
            </DialogPrimitive.Close>
          ) : null}
        </DialogPrimitive.Content>
      </OverlayPortal>
    </DialogPrimitive.Root>
  )
}

const HOVER_OPEN_MS = 200
const HOVER_CLOSE_MS = 150

export function PopoverNative({ node, props, rootProps, emit, children, slots, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const [open, setOpen] = useOpenState(node, scope, bool(props.open), emit)
  const side = str(props.side, `bottom`) as `top` | `right` | `bottom` | `left`
  // #EXP-1760: a pick in an IconPicker / ColorPicker grid closes it.
  const closePicker = useCallback(() => setOpen(false), [setOpen])
  // `openOn: hover` (a hover card): opens on pointer hover AND keyboard focus
  // of the trigger, stays while the pointer is over the content; touch
  // (no hover pointer) keeps press.
  const hoverMode = props.openOn === `hover` && ctx.hover
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const openRef = useRef(open)
  openRef.current = open
  // THIS popover's content (focus moving from the trigger into it keeps it
  // open; another hover card's content on the page does not count).
  const contentRef = useRef<HTMLDivElement | null>(null)
  const schedule = (next: boolean) => {
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => {
      if (openRef.current !== next) setOpen(next)
    }, next ? HOVER_OPEN_MS : HOVER_CLOSE_MS)
  }
  useEffect(() => () => void (timer.current && clearTimeout(timer.current)), [])
  const hoverProps = hoverMode
    ? {
        onPointerEnter: (e: ReactPointerEvent) => e.pointerType !== `touch` && schedule(true),
        onPointerLeave: (e: ReactPointerEvent) => e.pointerType !== `touch` && schedule(false),
        onFocus: () => schedule(true),
        onBlur: (e: React.FocusEvent) => {
          const to = e.relatedTarget as Node | null
          if (!to || !contentRef.current?.contains(to)) schedule(false)
        },
      }
    : {}
  // VAPP-276: `anchor` = a node elsewhere opens it and places it; `align`
  // along the side.
  const align = str(props.align, `center`) as `start` | `center` | `end`
  const anchorId = str(props.anchor)
  const contentId = useId()
  const wrapperRef = useRef<HTMLDivElement | null>(null)
  const outside = useRef(false)
  const anchor = useAnchor(anchorId, wrapperRef, { haspopup: `dialog`, toggle: () => setOpen(!openRef.current), hover: { active: () => hoverModeRef.current, enter: () => schedule(true), leave: () => schedule(false) } }, open, contentId)
  const anchored = !!anchorId
  // #EDE-65 typeaheadFocus: anchored on a text field = a typeahead: opening
  // it never takes the focus from the field, closing it returns it nowhere.
  const typeahead = anchored && isFieldAnchorElement(anchor.el)
  const [triggerEl, setTriggerEl] = useState<Element | null>(null)
  useLayoutEffect(() => setTriggerEl(wrapperRef.current?.querySelector(`.xui-trigger`) ?? wrapperRef.current?.firstElementChild ?? null), [])
  const hosted = useHostedPopup(node.id, `Popover`, open, anchored ? anchor.el : triggerEl, side, align)
  useCloseOnAnchorLost(open, anchored && anchor.el ? anchor.el : triggerEl, () => setOpen(false))
  const hoverModeRef = useRef(hoverMode)
  hoverModeRef.current = hoverMode
  const panelWidth = popoverPanelWidth(props, `--radix-popover-trigger-width`)
  return (
    <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
      <div ref={wrapperRef} {...overlayRootProps(rootProps, slots.trigger, node.slots?.trigger)}>
        {/* An id that names no painted node: the wrapper anchors it (the core's rule). */}
        {anchored && anchor.el ? <PopoverPrimitive.Anchor virtualRef={anchor.virtualRef} /> : null}
        {slots.trigger ? (
          <PopoverPrimitive.Trigger asChild>
            <Trigger {...hoverProps}>{slots.trigger}</Trigger>
          </PopoverPrimitive.Trigger>
        ) : anchored && anchor.el ? null : (
          <PopoverPrimitive.Anchor asChild>
            <span style={{ display: `inline-flex` }} />
          </PopoverPrimitive.Anchor>
        )}
      </div>
      <OverlayPortal as={PopoverPrimitive.Portal} hosted={hosted}>
        <PopoverPrimitive.Content
          ref={contentRef}
          {...(part(`content`, open && `open`) as Record<string, string>)}
          {...(anchored ? { id: contentId } : {})}
          side={side}
          align={align}
          sideOffset={OVERLAY_OFFSET}
          collisionPadding={OVERLAY_PADDING}
          data-xui-overlay="Popover"
          data-open-on={hoverMode ? `hover` : undefined}
          dir={ctx.direction}
          // #EXP-1760: a picker opens with the focus on its panel (Tab walks
          // the grid), not a focus ring on its first cell (the IDE's look).
          onOpenAutoFocus={hoverMode || typeahead ? (e) => e.preventDefault() : isPickerPopover(node.recipe) ? (e) => { e.preventDefault(); contentRef.current?.focus() } : undefined}
          // VAPP-276: a press on the anchor is its toggle, not an outside
          // press; a press or focus in ANOTHER open overlay (a second panel
          // applet) dismisses nothing (the core's rule); a close that did not
          // come from outside returns focus to the anchor.
          onInteractOutside={(e) => {
            if (withinOtherOverlay(contentRef.current, e.target) || (anchored && withinAnchor(anchor.el, e.target))) e.preventDefault()
            else outside.current = true
          }}
          onCloseAutoFocus={
            anchored
              ? (e) => {
                  e.preventDefault()
                  if (!outside.current && !hoverMode && !typeahead) anchor.el?.focus()
                  outside.current = false
                }
              : undefined
          }
          onPointerEnter={hoverMode ? () => schedule(true) : undefined}
          onPointerLeave={hoverMode ? () => schedule(false) : undefined}
          // `matchAnchorWidth`: exactly the anchor's (else the trigger's)
          // width, Radix's anchor-width variable (re-measured on a resize).
          style={panelWidth ? { width: panelWidth } : undefined}
        >
          {/* The nearest overlay decides: a Form in a Popover closes nothing,
              not even a Dialog around the Popover. */}
          <CloseOnSubmitContext.Provider value={null}>
            <ClosePickerContext.Provider value={isPickerPopover(node.recipe) ? closePicker : null}>{children}</ClosePickerContext.Provider>
          </CloseOnSubmitContext.Provider>
        </PopoverPrimitive.Content>
      </OverlayPortal>
    </PopoverPrimitive.Root>
  )
}

/** Tooltip: shows on hover AND keyboard focus (a11y.json), Escape hides.
 *  #VAPP-277: `side` may be logical (`start`/`end`, mirrored under RTL);
 *  inside a focusable host (a pressable rail Row) the anchor adds no tab
 *  stop and the host's keyboard focus opens it. */
export function TooltipNative({ node, props, rootProps, children }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const side = tooltipPhysicalSide(str(props.side, `top`), ctx.direction)
  const content = str(props.content) || str(props.text)
  const anchorRef = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState(false)
  useCloseOnAnchorLost(open, open ? anchorRef.current : null, () => setOpen(false))
  const [hosted, setHosted] = useState(false)
  useEffect(() => {
    const anchor = anchorRef.current
    const host = anchor ? tooltipFocusHost(anchor) : null
    setHosted(Boolean(host))
    if (!host) return
    let pointer = false
    const onDown = () => (pointer = true)
    const onFocus = () => {
      if (!pointer) setOpen(true)
      pointer = false
    }
    const onBlur = () => setOpen(false)
    host.addEventListener(`pointerdown`, onDown)
    host.addEventListener(`focus`, onFocus)
    host.addEventListener(`blur`, onBlur)
    return () => {
      host.removeEventListener(`pointerdown`, onDown)
      host.removeEventListener(`focus`, onFocus)
      host.removeEventListener(`blur`, onBlur)
    }
  }, [])
  // The tooltip wraps its children in the ANCHOR (`<id>.anchor`), the frame
  // it shows against; the root is layout-transparent around it.
  return (
    <TooltipPrimitive.Provider delayDuration={300}>
      <TooltipPrimitive.Root open={ctx.inlineOverlays || open} onOpenChange={setOpen}>
        <div {...(rootProps as Record<string, unknown>)} data-xui-overlay-root="">
          <TooltipPrimitive.Trigger asChild>
            <div ref={anchorRef} {...(part(`anchor`) as Record<string, string>)} tabIndex={hosted ? undefined : 0}>
              {children}
            </div>
          </TooltipPrimitive.Trigger>
        </div>
        <OverlayPortal as={TooltipPrimitive.Portal}>
          <TooltipPrimitive.Content {...(part(`content`) as Record<string, string>)} side={side} sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Tooltip">
            {content}
          </TooltipPrimitive.Content>
        </OverlayPortal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  )
}

// ---------------------------------------------------------------------------
// Menu (round 3: ONE native for press and context menus)
// ---------------------------------------------------------------------------

export interface MenuItem {
  label?: string
  value?: string
  kind?: `item` | `checkbox` | `separator` | `label` | `submenu`
  icon?: string
  /** #EXP-1629: the glyph's tint (`$color.<name>`, or the host's colour). */
  iconColor?: string
  shortcut?: string
  checked?: boolean
  destructive?: boolean
  disabled?: boolean
  separator?: boolean
  items?: MenuItem[]
  /** EXP-1259: the submenu is a search list of its entries. */
  searchable?: boolean
  placeholder?: string
  /** #EXP-1629: a mono id before the label, muted text after it, a nested
   *  surface beside the row while hovered or highlighted. */
  identifier?: string
  detail?: string
  preview?: unknown
}

/** EXP-1259: a searchable submenu = the search list (head, the entries that
 *  match as menu rows, the highlighted one `selected`, the fade): its field
 *  takes the focus when it opens (and when the pointer enters), ↑/↓ move,
 *  Enter or Tab pick (the menu closes), ← at the caret's start returns to
 *  the parent row. */
function MenuSearch({ item, i, part, emit, close }: { item: MenuItem; i: string; part: PartFn; emit: NativeProps[`emit`]; close: () => void }) {
  const ctx = useSurfaceContext()
  const [query, setQuery] = useState(``)
  const box = useRef<HTMLDivElement | null>(null)
  // The field takes the focus once Radix has focused the opened content.
  useEffect(() => {
    const frame = requestAnimationFrame(() => box.current?.querySelector<HTMLInputElement>(`input`)?.focus({ preventScroll: true }))
    return () => cancelAnimationFrame(frame)
  }, [])
  const rows = objects<MenuItem>(item.items)
    .map((entry, index) => ({ index, entry }))
    .filter(({ entry }) => entry.kind !== `separator` && entry.kind !== `label` && !entry.separator && searchMatches({ label: str(entry.label), identifier: entry.identifier }, query))
    .map(({ index, entry }) => ({ index, item: { label: str(entry.label), value: str(entry.value ?? entry.label), icon: entry.icon, iconColor: entry.iconColor, identifier: entry.identifier, detail: entry.detail, disabled: bool(entry.disabled) } }))
  return (
    <div
      ref={box}
      className="xui-menu-search"
      onPointerEnter={(e) => {
        const input = e.currentTarget.querySelector<HTMLInputElement>(`input`)
        if (input && !e.currentTarget.contains(document.activeElement)) input.focus({ preventScroll: true })
      }}
    >
      <SearchBody
        part={part}
        owner="Menu"
        suffix={`.${i}`}
        rows={rows}
        query={query}
        onQuery={setQuery}
        placeholder={str(item.placeholder) || ctx.t(`search`)}
        emptyText={ctx.t(`noResults`)}
        maxRows={8}
        showCount
        autoFocus
        leaveBubbles
        onPick={(picked) => {
          void emit(`select`, { value: picked.value })
          close()
        }}
        rowPart={(_, __, active) => part(`item`, active && `selected`) as Record<string, string>}
        renderRow={(row) => (
          <>
            {row.icon ? <IconGlyph icons={ctx.host.icons} name={row.icon} style={propColor(row.iconColor) ? { color: propColor(row.iconColor) } : undefined} /> : null}
            {typeof row.identifier === `string` && row.identifier ? <span {...(part(`identifier`) as Record<string, string>)}>{row.identifier}</span> : null}
            <span className="xui-menu-label">{row.label}</span>
            {typeof row.detail === `string` && row.detail ? <span {...(part(`detail`) as Record<string, string>)}>{row.detail}</span> : null}
          </>
        )}
      />
    </div>
  )
}

type MenuNs = typeof MenuPrimitive | typeof ContextPrimitive

/** #EXP-1629: a row's preview tracking (pointer, keyboard highlight) handed
 *  to its ONE Radix item child (Slot merges the handlers and the ref). */
function PreviewRow({ at, has, children }: { at: string; has: boolean; children: ReactNode }) {
  const row = useMenuPreviewRow(at, has)
  return <Slot.Root {...row}>{children}</Slot.Root>
}

/** The entries, shared by both openings. A submenu's `items` may be a
 *  binding (`{path}` to rows a host source feeds): the bind pass resolved it
 *  already, so it renders like a literal one. */
export function MenuItems({ ns, items, rawItems, part, overlay, emit, scope, path, close }: { ns: MenuNs; items: MenuItem[]; rawItems: unknown; part: PartFn; overlay: string; emit: NativeProps[`emit`]; scope: string; path: string; close: () => void }) {
  const ctx = useSurfaceContext()
  const raws = objects<Record<string, unknown>>(rawItems)
  const N = ns as typeof MenuPrimitive
  const previewApi = useContext(MenuPreviewContext)
  const menuModality = previewApi ? menuModalityProps(previewApi) : {}
  return (
    <>
      {items.map((item, i) => {
        const kind = item.separator ? `separator` : (item.kind ?? `item`)
        const key = `${path}${i}`
        const icon = item.icon ? <IconGlyph icons={ctx.host.icons} name={item.icon} style={propColor(item.iconColor) ? { color: propColor(item.iconColor) } : undefined} /> : null
        const shortcut = item.shortcut ? <span {...(part(`shortcut`) as Record<string, string>)}>{item.shortcut}</span> : null
        // #EXP-1629: the mono id between the icon and the label, the muted
        // detail after the label (before the shortcut / the indicator).
        const identifier = typeof item.identifier === `string` && item.identifier ? <span {...(part(`identifier`) as Record<string, string>)}>{item.identifier}</span> : null
        const detail = typeof item.detail === `string` && item.detail ? <span {...(part(`detail`) as Record<string, string>)}>{item.detail}</span> : null
        if (kind === `separator`) return <N.Separator key={key} {...(part(`separator`) as Record<string, string>)} />
        if (kind === `label`) return <N.Label key={key} {...(part(`label`) as Record<string, string>)}>{str(item.label)}</N.Label>
        if (kind === `submenu`) {
          return (
            <N.Sub key={key}>
              <N.SubTrigger {...(part(`item`, bool(item.disabled) && `disabled`) as Record<string, string>)} disabled={bool(item.disabled)}>
                {icon}
                {identifier}
                <span className="xui-menu-label">{str(item.label)}</span>
                {detail}
                <span {...(part(`submenuIndicator`) as Record<string, string>)}>
                  <BuiltinIcon slot="Menu.submenuIndicator" />
                </span>
              </N.SubTrigger>
              <OverlayPortal as={N.Portal}>
                <N.SubContent
                  {...(part(`content`) as Record<string, string>)}
                  {...menuModality}
                  onPointerDownOutside={insidePreview}
                  onFocusOutside={insidePreview}
                  sideOffset={2}
                  collisionPadding={OVERLAY_PADDING}
                  data-xui-overlay={`${overlay}.sub`}
                  data-searchable={item.searchable ? `true` : undefined}
                >
                  {item.searchable ? (
                    <MenuSearch item={item} i={key} part={part} emit={emit} close={close} />
                  ) : (
                    <MenuItems ns={ns} items={objects<MenuItem>(item.items)} rawItems={raws[i]?.items} part={part} overlay={overlay} emit={emit} scope={scope} path={`${key}.`} close={close} />
                  )}
                </N.SubContent>
              </OverlayPortal>
            </N.Sub>
          )
        }
        if (kind === `checkbox`) {
          const raw = raws[i]?.checked
          return (
            <PreviewRow key={key} at={key} has={hasPreview(item)}>
            <N.CheckboxItem
              {...(part(`item`, bool(item.disabled) && `disabled`, bool(item.checked) && `checked`) as Record<string, string>)}
              checked={bool(item.checked)}
              disabled={bool(item.disabled)}
              onCheckedChange={(next) => {
                const checked = next === true
                if (isBinding(raw)) ctx.setData(absolutePath(raw[`@path`], scope), checked)
                void emit(`select`, { value: item.value ?? item.label, checked })
              }}
              onSelect={(e) => e.preventDefault()}
            >
              <span {...(part(`check`) as Record<string, string>)}>
                <N.ItemIndicator>
                  <BuiltinIcon slot="Menu.check" />
                </N.ItemIndicator>
              </span>
              {icon}
              {identifier}
              <span className="xui-menu-label">{str(item.label)}</span>
              {detail}
              {shortcut}
            </N.CheckboxItem>
            </PreviewRow>
          )
        }
        return (
          <PreviewRow key={key} at={key} has={hasPreview(item)}>
          <N.Item {...(part(`item`, bool(item.disabled) && `disabled`) as Record<string, string>)} disabled={bool(item.disabled)} data-destructive={item.destructive ? `true` : undefined} onSelect={() => void emit(`select`, { value: item.value ?? item.label })}>
            {icon}
            {identifier}
            <span className="xui-menu-label">{str(item.label)}</span>
            {detail}
            {shortcut}
          </N.Item>
          </PreviewRow>
        )
      })}
    </>
  )
}

/** Menu: `openOn: press` (default) = a Radix DropdownMenu, its ONE child the
 *  trigger (no child = the default outline Button from label/icon, no
 *  chevron), anchored bottom-start; `openOn: contextmenu` = a Radix
 *  ContextMenu over the child (right-click / long-press / Shift+F10 / the
 *  Menu key), opened at the pointer (the child's corner from the keyboard). */
export function MenuNative(p: NativeProps) {
  return p.props.openOn === `contextmenu` ? <ContextMenuOpening {...p} /> : <PressMenuOpening {...p} />
}

function PressMenuOpening({ node, props, rootProps, emit, children, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const items = objects<MenuItem>(props.items)
  const label = str(props.label)
  const icon = str(props.icon)
  const [open, setOpen] = useState(ctx.inlineOverlays)
  const child = node.children[0] ?? null
  // VAPP-276: `side` + `align`; `anchor` = a node elsewhere is the trigger
  // the popper measures (its child, when given, still toggles it).
  const side = str(props.side, `bottom`) as `top` | `right` | `bottom` | `left`
  const align = str(props.align, `start`) as `start` | `center` | `end`
  const anchorId = str(props.anchor)
  const anchored = !!anchorId
  const contentId = useId()
  const wrapperRef = useRef<HTMLDivElement | null>(null)
  const openRef = useRef(open)
  openRef.current = open
  const anchor = useAnchor(anchorId, wrapperRef, { haspopup: `menu`, toggle: () => setOpen(!openRef.current), open: () => setOpen(true) }, open, contentId)
  const [triggerEl, setTriggerEl] = useState<Element | null>(null)
  useLayoutEffect(() => setTriggerEl(wrapperRef.current?.firstElementChild ?? null), [])
  const hosted = useHostedPopup(node.id, `Menu`, open, anchored ? anchor.el : triggerEl, side, align)
  useCloseOnAnchorLost(open, anchored && anchor.el ? anchor.el : triggerEl, () => setOpen(false))
  const preview = useMenuPreview(open)
  return (
    <MenuPreviewContext.Provider value={anyPreview(items) ? preview : null}>
    <MenuPrimitive.Root open={open} onOpenChange={setOpen} dir={ctx.direction}>
      <div ref={wrapperRef} {...overlayRootProps(rootProps, anchored && !child ? null : true, child)}>
        {anchored ? (
          <>
            <MenuPrimitive.Trigger asChild>
              <AnchorProxy anchor={anchor.virtual} />
            </MenuPrimitive.Trigger>
            {child ? <Trigger onClick={() => setOpen(!openRef.current)}>{children}</Trigger> : null}
          </>
        ) : (
        <MenuPrimitive.Trigger asChild>
          {child ? (
            <Trigger>{children}</Trigger>
          ) : (
            // Round 2 §7: the default trigger = an outline button with the
            // icon (when set) and the label (`$string.menu` without one and
            // without an icon), no chevron.
            <button type="button" {...(part(`trigger`, open && `open`) as Record<string, string>)} aria-label={label || ctx.t(`menu`)}>
              {icon ? <IconGlyph icons={ctx.host.icons} name={icon} className="xui-icon" width={16} height={16} /> : null}
              {label || !icon ? <span>{label || ctx.t(`menu`)}</span> : null}
            </button>
          )}
        </MenuPrimitive.Trigger>
        )}
      </div>
      <OverlayPortal as={MenuPrimitive.Portal} hosted={hosted}>
        <MenuPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} {...(anchored ? { id: contentId } : {})} {...menuModalityProps(preview)} onPointerDownOutside={insidePreview} onFocusOutside={insidePreview} side={side} align={align} sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Menu" data-open-on="press">
          <MenuItems ns={MenuPrimitive} items={items} rawItems={node.props.items} part={part} overlay="Menu" emit={emit} scope={scope} path="" close={() => setOpen(false)} />
        </MenuPrimitive.Content>
      </OverlayPortal>
      {open ? <MenuPreviewPanel api={preview} items={items} part={part} emit={emit} /> : null}
    </MenuPrimitive.Root>
    </MenuPreviewContext.Provider>
  )
}

/** A context menu is uncontrolled (Radix): close it the way Escape does. */
function closeContextMenu() {
  document.dispatchEvent(new KeyboardEvent(`keydown`, { key: `Escape`, bubbles: true }))
}

function ContextMenuOpening({ node, props, rootProps, emit, children, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const items = objects<MenuItem>(props.items)
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLDivElement | null>(null)
  useCloseOnAnchorLost(open, open ? triggerRef.current : null, closeContextMenu)
  const preview = useMenuPreview(open)
  return (
    <MenuPreviewContext.Provider value={anyPreview(items) ? preview : null}>
    <ContextPrimitive.Root onOpenChange={setOpen} dir={ctx.direction}>
      <ContextPrimitive.Trigger asChild>
        <div
          ref={triggerRef}
          {...(rootProps as Record<string, unknown>)}
          data-state={open ? `open` : `closed`}
          onKeyDown={(e) => {
            if ((e.key === `F10` && e.shiftKey) || e.key === `ContextMenu`) {
              e.preventDefault()
              const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
              e.currentTarget.dispatchEvent(new MouseEvent(`contextmenu`, { bubbles: true, cancelable: true, clientX: r.left + 8, clientY: r.top + 8 }))
            }
          }}
        >
          {children}
        </div>
      </ContextPrimitive.Trigger>
      <OverlayPortal as={ContextPrimitive.Portal}>
        <ContextPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} {...menuModalityProps(preview)} onPointerDownOutside={insidePreview} onFocusOutside={insidePreview} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Menu" data-open-on="contextmenu">
          <MenuItems ns={ContextPrimitive} items={items} rawItems={node.props.items} part={part} overlay="Menu" emit={emit} scope={scope} path="" close={closeContextMenu} />
        </ContextPrimitive.Content>
      </OverlayPortal>
      {open ? <MenuPreviewPanel api={preview} items={items} part={part} emit={emit} /> : null}
    </ContextPrimitive.Root>
    </MenuPreviewContext.Provider>
  )
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

/** Toast (contract §3): in the TOAST layer, newest nearest the edge, at most
 *  3 visible (base CSS). The renderer runs `duration` (0 = sticky), PAUSED
 *  while hovered or focused, then writes `open: false` and fires `dismiss`
 *  + `change`. `error` is assertive, the rest polite; never takes focus. */
export function ToastNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const [open, setOpenState] = useBoundState(node, scope, `open`, props.open === undefined ? true : bool(props.open))
  const [leaving, setLeaving] = useState(false)
  const [paused, setPaused] = useState(false)
  const duration = props.duration === undefined ? 5000 : Math.max(0, num(props.duration, 5000))
  const type = str(props.type, `info`)
  const remaining = useRef(duration)
  const startedAt = useRef(0)
  const close = useCallback(() => {
    setOpenState(false)
    void emit(`dismiss`, undefined, { open: false })
    void emit(`change`, { open: false }, { open: false })
  }, [setOpenState, emit])
  // Exit: keep the element for the motion duration with data-state=closed.
  const [shown, setShown] = useState(open)
  useEffect(() => {
    if (open) {
      setShown(true)
      setLeaving(false)
      remaining.current = duration
      return
    }
    if (!shown) return
    setLeaving(true)
    const ms = ctx.reducedMotion ? 0 : (ctx.theme.tokens.motion?.standard ?? 180)
    const t = setTimeout(() => {
      setShown(false)
      setLeaving(false)
    }, ms)
    return () => clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])
  useEffect(() => {
    if (!open || duration === 0 || paused || ctx.inlineOverlays) return
    startedAt.current = Date.now()
    const t = setTimeout(close, remaining.current)
    return () => {
      clearTimeout(t)
      remaining.current = Math.max(0, remaining.current - (Date.now() - startedAt.current))
    }
  }, [open, duration, paused, close])
  if (!shown || !(ctx.toastLayer || ctx.inlineOverlays)) return null
  const title = str(props.title)
  const description = str(props.description)
  const actionLabel = str(props.actionLabel)
  const assertive = type === `error`
  const toast = (
    <div
      {...(rootProps as Record<string, unknown>)}
      role={assertive ? `alert` : `status`}
      aria-live={assertive ? `assertive` : `polite`}
      aria-atomic="true"
      data-state={leaving ? `closed` : `open`}
      data-xui-overlay="Toast"
      onPointerEnter={() => setPaused(true)}
      onPointerLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
      onKeyDown={(e) => {
        if (e.key === `Escape` && props.dismissible !== false) {
          e.stopPropagation()
          close()
        }
      }}
    >
      <span {...(part(`icon`) as Record<string, string>)}>
        <BuiltinIcon slot={`Toast.icon.${type}`} />
      </span>
      <div className="xui-toast-text">
        <span {...(part(`title`) as Record<string, string>)}>{title}</span>
        {description ? <span {...(part(`description`) as Record<string, string>)}>{description}</span> : null}
      </div>
      {actionLabel ? (
        <button type="button" {...(part(`action`) as Record<string, string>)} onClick={() => void emit(`action`)}>
          {actionLabel}
        </button>
      ) : null}
      {props.dismissible !== false ? (
        <button type="button" {...(part(`close`) as Record<string, string>)} aria-label={ctx.t(`dismiss`)} onClick={close}>
          <BuiltinIcon slot="Toast.close" />
        </button>
      ) : null}
    </div>
  )
  return ctx.inlineOverlays ? toast : createPortal(toast, ctx.toastLayer!)
}
