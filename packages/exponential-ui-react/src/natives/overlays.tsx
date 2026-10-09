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

import { forwardRef, useCallback, useEffect, useRef, useState, type HTMLAttributes, type PointerEvent as ReactPointerEvent, type ReactNode } from "react"
import { createPortal } from "react-dom"
import { ContextMenu as ContextPrimitive, Dialog as DialogPrimitive, DropdownMenu as MenuPrimitive, Popover as PopoverPrimitive, Tooltip as TooltipPrimitive } from "radix-ui"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { isBinding } from "../data"
import { absolutePath } from "../data"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { CloseOnSubmitContext } from "../form"
import { useBoundState } from "./bound"
import { bool, BuiltinIcon, num, str, useParts, type PartFn, objects } from "./shared"

/** An overlay's `open`: local mirror, bound path written, `change {open}`. */
function useOpenState(node: NativeProps[`node`], scope: string, external: boolean, emit: NativeProps[`emit`]) {
  const [open, setOpen] = useBoundState(node, scope, `open`, external)
  const change = useCallback(
    (next: boolean) => {
      setOpen(next)
      void emit(`change`, { open: next })
    },
    [setOpen, emit]
  )
  return [open, change] as const
}

/** The slot wrapper Radix anchors to and clicks through (forwardRef so the
 *  popper can measure it; the rest of the props are Radix's). */
const Trigger = forwardRef<HTMLSpanElement, HTMLAttributes<HTMLSpanElement> & { children?: ReactNode }>(function Trigger({ children, ...rest }, ref) {
  return (
    <span ref={ref} className="xui-trigger" {...rest}>
      {children}
    </span>
  )
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
  return (
    <DialogPrimitive.Root open={open} onOpenChange={(next) => (next || dismissible ? setOpen(next) : undefined)}>
      <div {...overlayRootProps(rootProps, slots.trigger, node.slots?.trigger)}>
        {slots.trigger ? (
          <DialogPrimitive.Trigger asChild>
            <Trigger>{slots.trigger}</Trigger>
          </DialogPrimitive.Trigger>
        ) : null}
      </div>
      <DialogPrimitive.Portal container={ctx.portal ?? undefined}>
        <DialogPrimitive.Overlay {...(part(`overlay`, open && `open`) as Record<string, string>)} data-xui-overlay-backdrop={kind} />
        <DialogPrimitive.Content
          ref={drag.panel}
          {...(part(`content`, open && `open`) as Record<string, string>)}
          data-side={kind === `Drawer` ? side : undefined}
          data-xui-overlay={kind}
          dir={ctx.direction}
          role={kind === `Dialog` && !dismissible ? `alertdialog` : `dialog`}
          onEscapeKeyDown={dismissible ? undefined : (e) => e.preventDefault()}
          onPointerDownOutside={dismissible ? undefined : (e) => e.preventDefault()}
          onInteractOutside={dismissible ? undefined : (e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            // A part marked `accessibility.autoFocus` (contract §6: the
            // AlertDialog's cancel) takes the focus, else the cancel action.
            const content = e.currentTarget as HTMLElement | null
            const marked = content?.querySelector<HTMLElement>(`[data-xui-autofocus]`) ?? content?.querySelector<HTMLElement>(`[data-xui-part="AlertDialog/cancel"]`)
            const target = marked && (marked.matches(`button,input,textarea,select,a[href],[tabindex]`) ? marked : marked.querySelector<HTMLElement>(`button,input,textarea,select,a[href],[tabindex]`))
            if (target) {
              e.preventDefault()
              target.focus()
            }
          }}
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
          <CloseOnSubmitContext.Provider value={close}>
            <div className="xui-overlay-body">{children}</div>
            {slots.footer ? <div {...(part(`footer`) as Record<string, string>)}>{slots.footer}</div> : null}
          </CloseOnSubmitContext.Provider>
          {kind === `Dialog` && dismissible ? (
            <DialogPrimitive.Close {...(part(`close`) as Record<string, string>)} aria-label={ctx.t(`close`)}>
              <BuiltinIcon slot="Dialog.close" />
            </DialogPrimitive.Close>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
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
  // `openOn: hover` (HoverCard): opens on pointer hover AND keyboard focus
  // of the trigger, stays while the pointer is over the content; touch
  // (no hover pointer) keeps press.
  const hoverMode = props.openOn === `hover` && ctx.hover
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const openRef = useRef(open)
  openRef.current = open
  // THIS popover's content (focus moving from the trigger into it keeps it
  // open; another HoverCard's content on the page does not count).
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
  return (
    <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
      <div {...overlayRootProps(rootProps, slots.trigger, node.slots?.trigger)}>
        {slots.trigger ? (
          <PopoverPrimitive.Trigger asChild>
            <Trigger {...hoverProps}>{slots.trigger}</Trigger>
          </PopoverPrimitive.Trigger>
        ) : (
          <PopoverPrimitive.Anchor asChild>
            <span style={{ display: `inline-flex` }} />
          </PopoverPrimitive.Anchor>
        )}
      </div>
      <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
        <PopoverPrimitive.Content
          ref={contentRef}
          {...(part(`content`, open && `open`) as Record<string, string>)}
          side={side}
          align="center"
          sideOffset={OVERLAY_OFFSET}
          collisionPadding={OVERLAY_PADDING}
          data-xui-overlay="Popover"
          data-open-on={hoverMode ? `hover` : undefined}
          dir={ctx.direction}
          onOpenAutoFocus={hoverMode ? (e) => e.preventDefault() : undefined}
          onPointerEnter={hoverMode ? () => schedule(true) : undefined}
          onPointerLeave={hoverMode ? () => schedule(false) : undefined}
        >
          {children}
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  )
}

/** Tooltip: shows on hover AND keyboard focus (a11y.json), Escape hides. */
export function TooltipNative({ node, props, rootProps, children }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const side = str(props.side, `top`) as `top` | `right` | `bottom` | `left`
  const content = str(props.content) || str(props.text)
  // The tooltip wraps its children in the ANCHOR (`<id>.anchor`), the frame
  // it shows against; the root is layout-transparent around it.
  return (
    <TooltipPrimitive.Provider delayDuration={300}>
      <TooltipPrimitive.Root>
        <div {...(rootProps as Record<string, unknown>)} data-xui-overlay-root="">
          <TooltipPrimitive.Trigger asChild>
            <div {...(part(`anchor`) as Record<string, string>)} tabIndex={0}>
              {children}
            </div>
          </TooltipPrimitive.Trigger>
        </div>
        <TooltipPrimitive.Portal container={ctx.portal ?? undefined}>
          <TooltipPrimitive.Content {...(part(`content`) as Record<string, string>)} side={side} sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Tooltip">
            {content}
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  )
}

// ---------------------------------------------------------------------------
// Menu (round 3: ONE native for the old DropdownMenu + ContextMenu)
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
}

type MenuNs = typeof MenuPrimitive | typeof ContextPrimitive

/** The entries, shared by both openings. A submenu's `items` may be a
 *  binding (`{path}` to rows a host source feeds): the bind pass resolved it
 *  already, so it renders like a literal one. */
function MenuItems({ ns, items, rawItems, part, overlay, emit, scope, path }: { ns: MenuNs; items: MenuItem[]; rawItems: unknown; part: PartFn; overlay: string; emit: NativeProps[`emit`]; scope: string; path: string }) {
  const ctx = useSurfaceContext()
  const raws = objects<Record<string, unknown>>(rawItems)
  const N = ns as typeof MenuPrimitive
  return (
    <>
      {items.map((item, i) => {
        const kind = item.separator ? `separator` : (item.kind ?? `item`)
        const key = `${path}${i}`
        const icon = item.icon ? <IconGlyph icons={ctx.host.icons} name={item.icon} /> : null
        const shortcut = item.shortcut ? <span {...(part(`shortcut`) as Record<string, string>)}>{item.shortcut}</span> : null
        if (kind === `separator`) return <N.Separator key={key} {...(part(`separator`) as Record<string, string>)} />
        if (kind === `label`) return <N.Label key={key} {...(part(`label`) as Record<string, string>)}>{str(item.label)}</N.Label>
        if (kind === `submenu`) {
          return (
            <N.Sub key={key}>
              <N.SubTrigger {...(part(`item`, bool(item.disabled) && `disabled`) as Record<string, string>)} disabled={bool(item.disabled)}>
                {icon}
                <span className="xui-menu-label">{str(item.label)}</span>
                <span {...(part(`submenuIndicator`) as Record<string, string>)}>
                  <BuiltinIcon slot="Menu.submenuIndicator" />
                </span>
              </N.SubTrigger>
              <N.Portal container={ctx.portal ?? undefined}>
                <N.SubContent {...(part(`content`) as Record<string, string>)} sideOffset={2} collisionPadding={OVERLAY_PADDING} data-xui-overlay={`${overlay}.sub`}>
                  <MenuItems ns={ns} items={objects<MenuItem>(item.items)} rawItems={raws[i]?.items} part={part} overlay={overlay} emit={emit} scope={scope} path={`${key}.`} />
                </N.SubContent>
              </N.Portal>
            </N.Sub>
          )
        }
        if (kind === `checkbox`) {
          const raw = raws[i]?.checked
          return (
            <N.CheckboxItem
              key={key}
              {...(part(`item`, bool(item.disabled) && `disabled`, bool(item.checked) && `checked`) as Record<string, string>)}
              checked={bool(item.checked)}
              disabled={bool(item.disabled)}
              onCheckedChange={(next) => {
                const checked = next === true
                if (isBinding(raw)) ctx.setData(absolutePath(raw.path, scope), checked)
                void emit(`select`, { value: item.value ?? item.label, checked })
              }}
              onSelect={(e) => e.preventDefault()}
            >
              <span {...(part(`check`) as Record<string, string>)}>
                <N.ItemIndicator>
                  <BuiltinIcon slot="Menu.check" />
                </N.ItemIndicator>
              </span>
              <span className="xui-menu-label">{str(item.label)}</span>
              {shortcut}
            </N.CheckboxItem>
          )
        }
        return (
          <N.Item key={key} {...(part(`item`, bool(item.disabled) && `disabled`) as Record<string, string>)} disabled={bool(item.disabled)} data-destructive={item.destructive ? `true` : undefined} onSelect={() => void emit(`select`, { value: item.value ?? item.label })}>
            {icon}
            <span className="xui-menu-label">{str(item.label)}</span>
            {shortcut}
          </N.Item>
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
  const [open, setOpen] = useState(false)
  const child = node.children[0] ?? null
  return (
    <MenuPrimitive.Root open={open} onOpenChange={setOpen} dir={ctx.direction}>
      <div {...overlayRootProps(rootProps, true, child)}>
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
      </div>
      <MenuPrimitive.Portal container={ctx.portal ?? undefined}>
        <MenuPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} align="start" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Menu" data-open-on="press">
          <MenuItems ns={MenuPrimitive} items={items} rawItems={node.props.items} part={part} overlay="Menu" emit={emit} scope={scope} path="" />
        </MenuPrimitive.Content>
      </MenuPrimitive.Portal>
    </MenuPrimitive.Root>
  )
}

function ContextMenuOpening({ node, props, rootProps, emit, children, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const items = objects<MenuItem>(props.items)
  const [open, setOpen] = useState(false)
  return (
    <ContextPrimitive.Root onOpenChange={setOpen} dir={ctx.direction}>
      <ContextPrimitive.Trigger asChild>
        <div
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
      <ContextPrimitive.Portal container={ctx.portal ?? undefined}>
        <ContextPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Menu" data-open-on="contextmenu">
          <MenuItems ns={ContextPrimitive} items={items} rawItems={node.props.items} part={part} overlay="Menu" emit={emit} scope={scope} path="" />
        </ContextPrimitive.Content>
      </ContextPrimitive.Portal>
    </ContextPrimitive.Root>
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
    void emit(`dismiss`)
    void emit(`change`, { open: false })
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
    if (!open || duration === 0 || paused) return
    startedAt.current = Date.now()
    const t = setTimeout(close, remaining.current)
    return () => {
      clearTimeout(t)
      remaining.current = Math.max(0, remaining.current - (Date.now() - startedAt.current))
    }
  }, [open, duration, paused, close])
  if (!shown || !ctx.toastLayer) return null
  const title = str(props.title)
  const description = str(props.description)
  const actionLabel = str(props.actionLabel)
  const assertive = type === `error`
  return createPortal(
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
    </div>,
    ctx.toastLayer
  )
}
