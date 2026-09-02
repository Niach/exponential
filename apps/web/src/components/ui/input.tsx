import * as React from "react"

import { cn } from "@/lib/utils"

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<`input`>>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        ref={ref}
        type={type}
        data-slot="input"
        className={cn(
          // EXP-616: glass by default — the card fill + card hairline ARE the
          // field, so callers never re-dress an Input locally. EXP-698: 36
          // tall, 12 radius, and focus is the STROKE stepping to active — the
          // 3px halo ring is gone from every text field.
          `h-9 w-full min-w-0 rounded-lg border border-glass-stroke-card bg-glass-card px-3 py-1 text-base shadow-none transition-[color,border-color] duration-fast outline-none selection:bg-primary selection:text-primary-foreground file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-foreground/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 md:text-sm`,
          `focus-visible:border-glass-stroke-active focus-visible:ring-0`,
          `aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40`,
          className
        )}
        {...props}
      />
    )
  }
)

Input.displayName = `Input`

export { Input }
