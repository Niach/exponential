import * as React from "react"
import { HoverCard as HoverCardPrimitive } from "radix-ui"
import { HOVER_CARD_CLASSES, HOVER_CARD_DELAYS } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function HoverCard({
  openDelay = HOVER_CARD_DELAYS.openDelay,
  closeDelay = HOVER_CARD_DELAYS.closeDelay,
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
        className={cn(HOVER_CARD_CLASSES.content, className)}
        {...props}
      />
    </HoverCardPrimitive.Portal>
  )
}

export { HoverCard, HoverCardTrigger, HoverCardContent }
