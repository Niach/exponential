import type * as React from "react"
import { LoaderCircle } from "lucide-react"
import { SPINNER_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function Spinner({
  size = 10,
  label = `Loading`,
  className,
  ...props
}: Omit<React.ComponentProps<`svg`>, `children`> & {
  size?: number
  /** The accessible name; the glyph itself never carries words. */
  label?: string
  className?: string
}) {
  return (
    <LoaderCircle
      data-slot="spinner"
      data-testid="feedback-spinner"
      aria-label={label}
      width={size}
      height={size}
      style={{ width: size, height: size }}
      className={cn(SPINNER_CLASSES.root, className)}
      {...props}
    />
  )
}

export { Spinner }
