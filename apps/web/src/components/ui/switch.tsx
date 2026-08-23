import * as React from "react"
import { Switch as SwitchPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

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
      className={cn(
        // EXP-616: the default track grows one notch toward the iOS toggle
        // (20×36 with a 16px thumb); size=sm stays untouched for dense rows.
        `peer group/switch inline-flex shrink-0 items-center rounded-full border border-transparent shadow-none transition-all outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 data-[size=default]:h-5 data-[size=default]:w-9 data-[size=sm]:h-3.5 data-[size=sm]:w-6 data-[state=checked]:bg-primary data-[state=unchecked]:bg-glass-active`,
        className
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        // The checked offset is (track width - 2px of border - thumb width),
        // written relative to the thumb's own width: sm is 24-2-12 = 10px =
        // calc(100% - 2px); the widened default is 36-2-16 = 18px =
        // calc(100% + 2px).
        className={cn(
          `pointer-events-none block rounded-full bg-background ring-0 transition-transform group-data-[size=default]/switch:size-4 group-data-[size=sm]/switch:size-3 data-[state=checked]:translate-x-[calc(100%-2px)] data-[state=unchecked]:translate-x-0 group-data-[size=default]/switch:data-[state=checked]:translate-x-[calc(100%+2px)] dark:data-[state=checked]:bg-primary-foreground dark:data-[state=unchecked]:bg-foreground`
        )}
      />
    </SwitchPrimitive.Root>
  )
}

export { Switch }
