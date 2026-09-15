import * as React from "react"
import { HoverCard as HoverCardPrimitive } from "radix-ui"

import { cn } from "./cn"
import { MENU_SURFACE_CLASS } from "./menu-surface"

// EXP-760: the shadcn HoverCard, wired to the same opaque card composite the
// Popover uses (EXP-698 menu recipe — 12 radius, no blur, no shadow).
//
// The delays are the product's own, not Radix's defaults: 400 ms to open so a
// pointer crossing a dense list of `#IDENT` chips never flashes a card, 100 ms
// to close so the pointer can travel from the trigger into the card. The
// desktop IDE's `IssuePreviewHost` uses the same pair.
//
// Hover is a POINTER affordance: Radix only opens on `pointerenter` from a
// mouse-like device, so a touch tap on a wrapped trigger still just activates
// the trigger. Callers gate the mount on `useIsMobile` anyway.

function HoverCard({
  openDelay = 400,
  closeDelay = 100,
  ...props
}: React.ComponentProps<typeof HoverCardPrimitive.Root>) {
  return (
    <HoverCardPrimitive.Root
      data-slot="hover-card"
      openDelay={openDelay}
      closeDelay={closeDelay}
      {...props}
    />
  )
}

function HoverCardTrigger({
  ...props
}: React.ComponentProps<typeof HoverCardPrimitive.Trigger>) {
  return (
    <HoverCardPrimitive.Trigger data-slot="hover-card-trigger" {...props} />
  )
}

function HoverCardContent({
  className,
  align = `start`,
  sideOffset = 6,
  ...props
}: React.ComponentProps<typeof HoverCardPrimitive.Content>) {
  return (
    <HoverCardPrimitive.Portal>
      <HoverCardPrimitive.Content
        data-slot="hover-card-content"
        align={align}
        sideOffset={sideOffset}
        className={cn(
          MENU_SURFACE_CLASS,
          `w-80 origin-(--radix-hover-card-content-transform-origin) p-3 outline-hidden`,
          className
        )}
        {...props}
      />
    </HoverCardPrimitive.Portal>
  )
}

export { HoverCard, HoverCardTrigger, HoverCardContent }
