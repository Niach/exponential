import * as React from "react"
import { Switch as SwitchPrimitive } from "radix-ui"
import { SWITCH_CLASSES } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

function Switch({
  className,
  size = `default`,
  ...props
}: React.ComponentProps<typeof SwitchPrimitive.Root> & {
  size?: `sm` | `default`
}) {
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      data-size={size}
      className={cn(SWITCH_CLASSES.root, className)}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className={cn(SWITCH_CLASSES.thumb)}
      />
    </SwitchPrimitive.Root>
  )
}

export { Switch }
