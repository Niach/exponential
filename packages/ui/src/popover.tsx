import * as React from "react"
import { Popover as PopoverPrimitive } from "radix-ui"

import { cn } from "./cn"
import { MENU_SURFACE_CLASS } from "./menu-surface"

function Popover({
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Root>) {
  return <PopoverPrimitive.Root data-slot="popover" {...props} />
}

function PopoverTrigger({
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Trigger>) {
  return <PopoverPrimitive.Trigger data-slot="popover-trigger" {...props} />
}

function PopoverContent({
  className,
  align = `center`,
  sideOffset = 4,
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Content>) {
  return (
    <PopoverPrimitive.Portal>
      <PopoverPrimitive.Content
        data-slot="popover-content"
        align={align}
        sideOffset={sideOffset}
        className={cn(
          // EXP-698 menu recipe: opaque card composite, 12 radius, NO blur and
          // NO shadow. A popover keeps its `p-4` form padding; menu-shaped
          // hosts (Command lists) pass `p-0` as they always have.
          MENU_SURFACE_CLASS,
          `w-72 origin-(--radix-popover-content-transform-origin) p-4 outline-hidden`,
          className
        )}
        {...props}
      />
    </PopoverPrimitive.Portal>
  )
}

function PopoverAnchor({
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Anchor>) {
  return <PopoverPrimitive.Anchor data-slot="popover-anchor" {...props} />
}

export { Popover, PopoverTrigger, PopoverContent, PopoverAnchor }
