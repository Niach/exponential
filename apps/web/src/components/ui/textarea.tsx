import * as React from "react"

import { cn } from "@/lib/utils"

function Textarea({ className, ...props }: React.ComponentProps<`textarea`>) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        // EXP-616: glass by default, matching Input. EXP-698: the same 12
        // radius / card fill / card hairline recipe, focus on the stroke and
        // no ring. `resize-none` is part of it — every field here auto-grows
        // through `field-sizing-content`, so the native grip only ever landed
        // mid-card over a tool row. Browsers without `field-sizing`
        // (Firefox, Safari) keep a vertical grip so the field can still grow.
        `flex field-sizing-content min-h-16 w-full resize-y [@supports(field-sizing:content)]:resize-none rounded-lg border border-glass-stroke-card bg-glass-card px-3 py-2 text-base shadow-none transition-[color,border-color] duration-fast outline-none placeholder:text-foreground/50 focus-visible:border-glass-stroke-active focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 md:text-sm dark:aria-invalid:ring-destructive/40`,
        className
      )}
      {...props}
    />
  )
}

export { Textarea }
