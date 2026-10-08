import * as React from "react"
import { LoaderCircleIcon } from "lucide-react"

import { cn } from "./cn"

// VAPP-87: shadcn's spinner: the loader circle, spinning, announced as a
// status. Under reduced motion it stands still.

function Spinner({ className, ...props }: React.ComponentProps<`svg`>) {
  return (
    <LoaderCircleIcon
      data-slot="spinner"
      role="status"
      aria-label="Loading"
      className={cn(`size-4 motion-safe:animate-spin`, className)}
      {...props}
    />
  )
}

export { Spinner }
