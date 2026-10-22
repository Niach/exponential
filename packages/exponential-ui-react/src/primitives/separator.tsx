"use client"

import * as React from "react"
import { Separator as SeparatorPrimitive } from "radix-ui"
import { SEPARATOR_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function Separator({
  className,
  orientation = `horizontal`,
  decorative = true,
  ...props
}: React.ComponentProps<typeof SeparatorPrimitive.Root>) {
  return (
    <SeparatorPrimitive.Root
      data-slot="separator"
      decorative={decorative}
      orientation={orientation}
      className={cn(SEPARATOR_CLASSES.root, className)}
      {...props}
    />
  )
}

export { Separator }
