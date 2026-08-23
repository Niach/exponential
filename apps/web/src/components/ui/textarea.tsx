import * as React from "react"

import { cn } from "@/lib/utils"

function Textarea({ className, ...props }: React.ComponentProps<`textarea`>) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        // EXP-616: glass by default, matching Input.
        `flex field-sizing-content min-h-16 w-full rounded-md border border-glass-stroke bg-glass-row px-3 py-2 text-base shadow-none transition-[color,box-shadow] outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 md:text-sm dark:aria-invalid:ring-destructive/40`,
        className
      )}
      {...props}
    />
  )
}

export { Textarea }
