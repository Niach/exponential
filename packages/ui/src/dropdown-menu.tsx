"use client"

import * as React from "react"
import { CheckIcon, ChevronRightIcon, CircleIcon } from "lucide-react"
import { DropdownMenu as DropdownMenuPrimitive } from "radix-ui"

import { cn } from "./cn"
import {
  MENU_CHECK_ITEM_CLASS,
  MENU_CHEVRON_CLASS,
  MENU_CONTENT_CLASS,
  MENU_ITEM_CLASS,
  MENU_LABEL_CLASS,
  MENU_SEPARATOR_CLASS,
  MENU_SHORTCUT_CLASS,
  MENU_SUB_TRIGGER_CLASS,
  MENU_SURFACE_CLASS,
} from "./menu-surface"

function DropdownMenu({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Root>) {
  return <DropdownMenuPrimitive.Root data-slot="dropdown-menu" {...props} />
}

function DropdownMenuPortal({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Portal>) {
  return (
    <DropdownMenuPrimitive.Portal data-slot="dropdown-menu-portal" {...props} />
  )
}

function DropdownMenuTrigger({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Trigger>) {
  return (
    <DropdownMenuPrimitive.Trigger
      data-slot="dropdown-menu-trigger"
      {...props}
    />
  )
}

function DropdownMenuContent({
  className,
  sideOffset = 4,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Content>) {
  return (
    <DropdownMenuPrimitive.Portal>
      <DropdownMenuPrimitive.Content
        data-slot="dropdown-menu-content"
        sideOffset={sideOffset}
        className={cn(
          // EXP-698 menu recipe: opaque card composite, 12 radius, NO blur and
          // NO shadow — see `bg-glass-card-opaque` in styles.css.
          MENU_SURFACE_CLASS,
          MENU_CONTENT_CLASS,
          `max-h-(--radix-dropdown-menu-content-available-height) origin-(--radix-dropdown-menu-content-transform-origin) overflow-x-hidden overflow-y-auto`,
          className
        )}
        {...props}
      />
    </DropdownMenuPrimitive.Portal>
  )
}

function DropdownMenuGroup({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Group>) {
  return (
    <DropdownMenuPrimitive.Group data-slot="dropdown-menu-group" {...props} />
  )
}

function DropdownMenuItem({
  className,
  inset,
  variant = `default`,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Item> & {
  inset?: boolean
  variant?: `default` | `destructive`
}) {
  return (
    <DropdownMenuPrimitive.Item
      data-slot="dropdown-menu-item"
      data-inset={inset}
      data-variant={variant}
      className={cn(
        MENU_ITEM_CLASS,
        className
      )}
      {...props}
    />
  )
}

function DropdownMenuCheckboxItem({
  className,
  children,
  checked,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.CheckboxItem>) {
  return (
    <DropdownMenuPrimitive.CheckboxItem
      data-slot="dropdown-menu-checkbox-item"
      className={cn(
        MENU_CHECK_ITEM_CLASS,
        className
      )}
      checked={checked}
      {...props}
    >
      <span className="pointer-events-none absolute left-2 flex size-3.5 items-center justify-center">
        <DropdownMenuPrimitive.ItemIndicator>
          <CheckIcon className="size-4" />
        </DropdownMenuPrimitive.ItemIndicator>
      </span>
      {children}
    </DropdownMenuPrimitive.CheckboxItem>
  )
}

function DropdownMenuRadioGroup({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.RadioGroup>) {
  return (
    <DropdownMenuPrimitive.RadioGroup
      data-slot="dropdown-menu-radio-group"
      {...props}
    />
  )
}

function DropdownMenuRadioItem({
  className,
  children,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.RadioItem>) {
  return (
    <DropdownMenuPrimitive.RadioItem
      data-slot="dropdown-menu-radio-item"
      className={cn(
        MENU_CHECK_ITEM_CLASS,
        className
      )}
      {...props}
    >
      <span className="pointer-events-none absolute left-2 flex size-3.5 items-center justify-center">
        <DropdownMenuPrimitive.ItemIndicator>
          <CircleIcon className="size-2 fill-current" />
        </DropdownMenuPrimitive.ItemIndicator>
      </span>
      {children}
    </DropdownMenuPrimitive.RadioItem>
  )
}

function DropdownMenuLabel({
  className,
  inset,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Label> & {
  inset?: boolean
}) {
  return (
    <DropdownMenuPrimitive.Label
      data-slot="dropdown-menu-label"
      data-inset={inset}
      className={cn(
        MENU_LABEL_CLASS,
        className
      )}
      {...props}
    />
  )
}

function DropdownMenuSeparator({
  className,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Separator>) {
  return (
    <DropdownMenuPrimitive.Separator
      data-slot="dropdown-menu-separator"
      className={cn(MENU_SEPARATOR_CLASS, className)}
      {...props}
    />
  )
}

function DropdownMenuShortcut({
  className,
  ...props
}: React.ComponentProps<`span`>) {
  return (
    <span
      data-slot="dropdown-menu-shortcut"
      className={cn(
        MENU_SHORTCUT_CLASS,
        className
      )}
      {...props}
    />
  )
}

function DropdownMenuSub({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Sub>) {
  return <DropdownMenuPrimitive.Sub data-slot="dropdown-menu-sub" {...props} />
}

function DropdownMenuSubTrigger({
  className,
  inset,
  children,
  // EXP-687: a submenu that opens a destructive action reads red like the
  // item it leads to — same tokens as DropdownMenuItem's destructive variant.
  variant = `default`,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.SubTrigger> & {
  inset?: boolean
  variant?: `default` | `destructive`
}) {
  return (
    <DropdownMenuPrimitive.SubTrigger
      data-slot="dropdown-menu-sub-trigger"
      data-inset={inset}
      data-variant={variant}
      className={cn(
        MENU_SUB_TRIGGER_CLASS,
        className
      )}
      {...props}
    >
      {children}
      <ChevronRightIcon className={MENU_CHEVRON_CLASS} />
    </DropdownMenuPrimitive.SubTrigger>
  )
}

function DropdownMenuSubContent({
  className,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.SubContent>) {
  return (
    // Portaled: un-portaled sub content lives inside the parent content's DOM,
    // and the parent's `glass-panel` backdrop-filter makes it a containing
    // block whose overflow-x-hidden clips the fixed-positioned submenu to
    // invisibility in Safari/WebKit (EXP-305).
    <DropdownMenuPrimitive.Portal>
      <DropdownMenuPrimitive.SubContent
        data-slot="dropdown-menu-sub-content"
        className={cn(
          MENU_SURFACE_CLASS,
          MENU_CONTENT_CLASS,
          `origin-(--radix-dropdown-menu-content-transform-origin) overflow-hidden`,
          className
        )}
        {...props}
      />
    </DropdownMenuPrimitive.Portal>
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
