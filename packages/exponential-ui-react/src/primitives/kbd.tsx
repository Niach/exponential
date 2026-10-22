import * as React from "react"
import { KBD_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function Kbd({ className, ...props }: React.ComponentProps<`kbd`>) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(KBD_CLASSES.root, className)}
      {...props}
    />
  )
}

function KbdGroup({ className, ...props }: React.ComponentProps<`span`>) {
  return <span data-slot="kbd-group" className={cn(KBD_CLASSES.group, className)} {...props} />
}

export { Kbd, KbdGroup }
