import * as React from "react"
import { GLASS_CARD_CLASSES } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

export { GLASS_CARD_CLASS, GLASS_CARD_CLASSES } from "@exponential_app/eui/web/primitives"

export function GlassCard({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="glass-card"
      className={cn(GLASS_CARD_CLASSES.root, className)}
      {...props}
    />
  )
}
