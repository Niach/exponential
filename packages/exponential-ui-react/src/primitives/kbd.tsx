import * as React from "react"

import { cn } from "./cn"

// Round 1: shadcn's Kbd (the key-cap hint the catalog's Kbd macro paints the
// same way) — a single key, or `KbdGroup` for a chord.

function Kbd({ className, ...props }: React.ComponentProps<`kbd`>) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(
        `pointer-events-none inline-flex h-5 w-fit min-w-5 items-center justify-center gap-1 rounded-sm bg-muted px-1 font-sans text-xs font-medium text-muted-foreground select-none [&_svg:not([class*='size-'])]:size-3`,
        className
      )}
      {...props}
    />
  )
}

function KbdGroup({ className, ...props }: React.ComponentProps<`span`>) {
  return <span data-slot="kbd-group" className={cn(`inline-flex items-center gap-1`, className)} {...props} />
}

export { Kbd, KbdGroup }
