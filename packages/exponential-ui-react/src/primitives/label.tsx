import * as React from "react"
import { Label as LabelPrimitive } from "radix-ui"
import { LABEL_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function Label({
  className,
  ...props
}: React.ComponentProps<typeof LabelPrimitive.Root>) {
  return (
    <LabelPrimitive.Root
      data-slot="label"
      className={cn(LABEL_CLASSES.root, className)}
      {...props}
    />
  )
}

export { Label }
