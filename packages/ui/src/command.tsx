"use client"

import * as React from "react"
import { Command as CommandPrimitive } from "cmdk"
import { SearchIcon } from "lucide-react"

import { Button } from "./button"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { FIELD_CHROME_CLASS } from "./input"
import {
  SEARCH_FIELD_CLEAR_CLASS,
  SEARCH_FIELD_GLYPH_CLASS,
  SEARCH_FIELD_INPUT_CLASS,
} from "./search-field"

const SearchGlyph = conceptIcon(`nav-search`)
const ClearGlyph = conceptIcon(`ui-clear`)

function Command({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive>) {
  return (
    <CommandPrimitive
      data-slot="command"
      className={cn(
        // Transparent (EXP-269): the hosts — filter popovers, the search
        // sheet — provide the glass surface; an opaque fill would break it.
        `flex h-full w-full flex-col overflow-hidden rounded-md bg-transparent text-popover-foreground`,
        className
      )}
      {...props}
    />
  )
}

function CommandInput({
  className,
  leading,
  variant = `inline`,
  clearLabel,
  ref,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Input> & {
  /** Rendered INSIDE the field row, before the search glyph — a host's back
   *  arrow (the phone search sheet) that must share the row without leaving
   *  the cmdk root, which owns the keyboard. */
  leading?: React.ReactNode
  /** `inline` (default) = the bare popover row: glyph, text, a hairline
   *  under. `field` = the `SearchField` look around cmdk's own input — the
   *  glass box with the glyph inside and a trailing clear once there is
   *  something to clear (which puts the caret back) — for a host that is a
   *  page (the phone search sheet) rather than a popover. */
  variant?: `inline` | `field`
  /** The `field` clear button's accessible name. */
  clearLabel?: string
}) {
  const innerRef = React.useRef<HTMLInputElement | null>(null)
  const setRef = (node: HTMLInputElement | null) => {
    innerRef.current = node
    if (typeof ref === `function`) {
      ref(node)
    } else if (ref) {
      ref.current = node
    }
  }

  if (variant === `field`) {
    const showClear = (props.value ?? ``) !== ``
    return (
      <div
        data-slot="command-input-wrapper"
        data-variant="field"
        className="flex items-center gap-2 border-b border-glass-stroke px-3 py-2"
      >
        {leading}
        <div data-slot="search-field" className="relative w-full">
          <SearchGlyph aria-hidden className={SEARCH_FIELD_GLYPH_CLASS} />
          <CommandPrimitive.Input
            ref={setRef}
            data-slot="command-input"
            className={cn(
              FIELD_CHROME_CLASS,
              SEARCH_FIELD_INPUT_CLASS,
              showClear && `pr-8`,
              className
            )}
            {...props}
          />
          {showClear && (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              data-slot="search-field-clear"
              aria-label={clearLabel ?? `Clear search`}
              className={SEARCH_FIELD_CLEAR_CLASS}
              onClick={() => {
                props.onValueChange?.(``)
                innerRef.current?.focus()
              }}
            >
              <ClearGlyph aria-hidden />
            </Button>
          )}
        </div>
      </div>
    )
  }

  return (
    <div
      data-slot="command-input-wrapper"
      className="flex h-9 items-center gap-2 border-b border-glass-stroke px-3"
    >
      {leading}
      <SearchIcon className="size-4 shrink-0 opacity-50" />
      <CommandPrimitive.Input
        ref={setRef}
        data-slot="command-input"
        className={cn(
          `flex h-10 w-full rounded-md bg-transparent py-3 text-sm outline-hidden placeholder:text-foreground/50 disabled:cursor-not-allowed disabled:opacity-50`,
          className
        )}
        {...props}
      />
    </div>
  )
}

function CommandList({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.List>) {
  return (
    <CommandPrimitive.List
      data-slot="command-list"
      className={cn(
        `max-h-[18.75rem] scroll-py-1 overflow-x-hidden overflow-y-auto`,
        className
      )}
      {...props}
    />
  )
}

function CommandEmpty({
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Empty>) {
  return (
    <CommandPrimitive.Empty
      data-slot="command-empty"
      className="py-6 text-center text-sm"
      {...props}
    />
  )
}

function CommandGroup({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Group>) {
  return (
    <CommandPrimitive.Group
      data-slot="command-group"
      className={cn(
        `overflow-hidden p-(--menu-surface-padding) text-foreground [&_[cmdk-group-heading]]:px-(--menu-item-padding-x) [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-muted-foreground`,
        className
      )}
      {...props}
    />
  )
}

function CommandSeparator({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Separator>) {
  return (
    <CommandPrimitive.Separator
      data-slot="command-separator"
      className={cn(`-mx-(--menu-surface-padding) my-(--menu-surface-padding) h-px bg-glass-stroke`, className)}
      {...props}
    />
  )
}

function CommandItem({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Item>) {
  return (
    <CommandPrimitive.Item
      data-slot="command-item"
      className={cn(
        `relative flex min-h-(--menu-item-height) cursor-default items-center gap-(--menu-item-gap) rounded-sm px-(--menu-item-padding-x) py-1 text-sm text-foreground/90 outline-hidden select-none data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 data-[selected=true]:bg-glass-active data-[selected=true]:text-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-(--menu-icon-size) [&_svg:not([class*='text-'])]:text-muted-foreground`,
        className
      )}
      {...props}
    />
  )
}

function CommandShortcut({
  className,
  ...props
}: React.ComponentProps<`span`>) {
  return (
    <span
      data-slot="command-shortcut"
      className={cn(
        `ml-auto text-xs tracking-widest text-muted-foreground`,
        className
      )}
      {...props}
    />
  )
}

export {
  Command,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandShortcut,
  CommandSeparator,
}
