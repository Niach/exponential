// VAPP-87: the OVERLAYS — Dialog, Drawer, Popover, Tooltip, DropdownMenu on
// Radix portals. Every portal mounts into the SURFACE ROOT (outside the
// `xui` container's containment), so the theme's variables and scope class
// reach it and `position: fixed` still means the viewport. The placement
// contract the core mirrors: side + centre alignment, `OVERLAY_OFFSET` from
// the anchor, flip to the opposite side when the preferred one lacks room,
// shift to stay `OVERLAY_PADDING` inside the viewport
// (`@exponential-at/ui` `placeOverlay`, fixtures/overlay-geometry.json).
// A trigger slot is wrapped in an inline-flex span so Radix measures the
// slot's own box and the slot's painter keeps its element.

import { forwardRef, useEffect, useState, type HTMLAttributes, type ReactNode } from "react"
import { Dialog as DialogPrimitive, DropdownMenu as MenuPrimitive, Popover as PopoverPrimitive, Tooltip as TooltipPrimitive } from "radix-ui"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { CHROME, IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { arr, bool, str, useParts } from "./shared"

function useOpenState(external: boolean, emit: NativeProps[`emit`]) {
  const [open, setOpen] = useState(external)
  useEffect(() => setOpen(external), [external])
  const change = (next: boolean) => {
    setOpen(next)
    void emit(`change`, { open: next })
  }
  return [open, change] as const
}

/** The slot wrapper Radix anchors to and clicks through (forwardRef so the
 *  popper can measure it; the rest of the props are Radix's). */
const Trigger = forwardRef<HTMLSpanElement, HTMLAttributes<HTMLSpanElement> & { children?: ReactNode }>(function Trigger({ children, style, ...rest }, ref) {
  return (
    <span ref={ref} className="xui-trigger" style={{ display: `inline-flex`, ...style }} {...rest}>
      {children}
    </span>
  )
})

export function DialogNative(p: NativeProps) {
  return <ModalNative {...p} kind="Dialog" />
}
export function DrawerNative(p: NativeProps) {
  return <ModalNative {...p} kind="Drawer" />
}

function ModalNative({ node, props, rootProps, emit, children, slots, kind }: NativeProps & { kind: `Dialog` | `Drawer` }) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const [open, setOpen] = useOpenState(bool(props.open), emit)
  const dismissible = kind === `Drawer` || props.dismissible !== false
  const side = str(props.side, `bottom`)
  const title = str(props.title)
  const description = str(props.description)
  const Close = CHROME.close
  return (
    <DialogPrimitive.Root open={open} onOpenChange={(next) => (next || dismissible ? setOpen(next) : undefined)}>
      <div {...(rootProps as Record<string, unknown>)} style={{ display: slots.trigger ? undefined : `contents` }}>
        {slots.trigger ? (
          <DialogPrimitive.Trigger asChild>
            <Trigger>{slots.trigger}</Trigger>
          </DialogPrimitive.Trigger>
        ) : null}
      </div>
      <DialogPrimitive.Portal container={ctx.portal ?? undefined}>
        <DialogPrimitive.Overlay {...(part(`overlay`, open && `open`) as Record<string, string>)} />
        <DialogPrimitive.Content
          {...(part(`content`, open && `open`) as Record<string, string>)}
          data-side={kind === `Drawer` ? side : undefined}
          data-xui-overlay={kind}
          onEscapeKeyDown={dismissible ? undefined : (e) => e.preventDefault()}
          onPointerDownOutside={dismissible ? undefined : (e) => e.preventDefault()}
          aria-describedby={description ? undefined : undefined}
        >
          {kind === `Drawer` && (side === `bottom` || side === `top`) ? <div {...(part(`handle`) as Record<string, string>)} aria-hidden="true" /> : null}
          {title || description ? (
            <div className="xui-overlay-head">
              {title ? <DialogPrimitive.Title {...(part(`title`) as Record<string, string>)}>{title}</DialogPrimitive.Title> : <DialogPrimitive.Title className="xui-sr-only">{kind}</DialogPrimitive.Title>}
              {description ? <DialogPrimitive.Description {...(part(`description`) as Record<string, string>)}>{description}</DialogPrimitive.Description> : null}
            </div>
          ) : (
            <DialogPrimitive.Title className="xui-sr-only">{kind}</DialogPrimitive.Title>
          )}
          <div className="xui-overlay-body">{children}</div>
          {slots.footer ? <div {...(part(`footer`) as Record<string, string>)}>{slots.footer}</div> : null}
          {kind === `Dialog` && dismissible ? (
            <DialogPrimitive.Close {...(part(`close`) as Record<string, string>)} aria-label="Close">
              <Close aria-hidden="true" />
            </DialogPrimitive.Close>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}

export function PopoverNative({ node, props, rootProps, emit, children, slots }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const [open, setOpen] = useOpenState(bool(props.open), emit)
  const side = str(props.side, `bottom`) as `top` | `right` | `bottom` | `left`
  return (
    <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
      <div {...(rootProps as Record<string, unknown>)}>
        {slots.trigger ? (
          <PopoverPrimitive.Trigger asChild>
            <Trigger>{slots.trigger}</Trigger>
          </PopoverPrimitive.Trigger>
        ) : (
          <PopoverPrimitive.Anchor asChild>
            <span style={{ display: `inline-flex` }} />
          </PopoverPrimitive.Anchor>
        )}
      </div>
      <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
        <PopoverPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} side={side} align="center" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Popover">
          {children}
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  )
}

export function TooltipNative({ node, props, rootProps, children }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const side = str(props.side, `top`) as `top` | `right` | `bottom` | `left`
  const content = str(props.content) || str(props.text)
  return (
    <TooltipPrimitive.Provider delayDuration={300}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>
          <div {...(rootProps as Record<string, unknown>)} tabIndex={0}>
            {children}
          </div>
        </TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal container={ctx.portal ?? undefined}>
          <TooltipPrimitive.Content {...(part(`content`) as Record<string, string>)} side={side} sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Tooltip">
            {content}
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  )
}

interface MenuItem {
  label?: string
  value?: string
  icon?: string
  destructive?: boolean
  disabled?: boolean
  separator?: boolean
}

export function DropdownMenuNative({ node, props, rootProps, emit, slots }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const items = arr<MenuItem>(props.items)
  const label = str(props.label)
  const icon = str(props.icon)
  const [open, setOpen] = useState(false)
  const Chevron = CHROME.chevronDown
  return (
    <MenuPrimitive.Root open={open} onOpenChange={setOpen} dir={ctx.direction}>
      <div {...(rootProps as Record<string, unknown>)}>
        <MenuPrimitive.Trigger asChild>
          {slots.trigger ? (
            <Trigger>{slots.trigger}</Trigger>
          ) : (
            <button type="button" {...(part(`trigger`, open && `open`) as Record<string, string>)} aria-label={label || `Menu`}>
              {icon ? <IconGlyph icons={ctx.host.icons} name={icon} className="xui-icon" width={16} height={16} /> : null}
              {label ? <span>{label}</span> : null}
              <Chevron aria-hidden="true" width={16} height={16} />
            </button>
          )}
        </MenuPrimitive.Trigger>
      </div>
      <MenuPrimitive.Portal container={ctx.portal ?? undefined}>
        <MenuPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} align="start" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="DropdownMenu">
          {items.map((item, i) =>
            item.separator ? (
              <MenuPrimitive.Separator key={i} {...(part(`separator`) as Record<string, string>)} />
            ) : (
              <MenuPrimitive.Item
                key={item.value ?? i}
                {...(part(`item`, bool(item.disabled) && `disabled`) as Record<string, string>)}
                disabled={bool(item.disabled)}
                data-destructive={item.destructive ? `true` : undefined}
                onSelect={() => void emit(`select`, { value: item.value ?? item.label })}
              >
                {item.icon ? <IconGlyph icons={ctx.host.icons} name={item.icon} /> : null}
                <span>{str(item.label)}</span>
              </MenuPrimitive.Item>
            )
          )}
        </MenuPrimitive.Content>
      </MenuPrimitive.Portal>
    </MenuPrimitive.Root>
  )
}
