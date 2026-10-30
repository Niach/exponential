import * as React from "react"
import { SKELETON_CLASSES } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

function Skeleton({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="skeleton"
      className={cn(SKELETON_CLASSES.root, className)}
      {...props}
    />
  )
}

export { Skeleton }
