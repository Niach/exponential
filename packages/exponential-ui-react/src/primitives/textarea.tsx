import * as React from "react"
import { TEXTAREA_CLASSES } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

function Textarea({ className, ...props }: React.ComponentProps<`textarea`>) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(TEXTAREA_CLASSES.root, className)}
      {...props}
    />
  )
}

export { Textarea }
