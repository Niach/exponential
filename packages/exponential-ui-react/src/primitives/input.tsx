import * as React from "react"
import { INPUT_CLASSES } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

export { BARE_FIELD_CLASS, FIELD_CHROME_CLASS } from "@exponential_app/eui/web/primitives"

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<`input`>>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        ref={ref}
        type={type}
        data-slot="input"
        className={cn(INPUT_CLASSES.root, className)}
        {...props}
      />
    )
  }
)

Input.displayName = `Input`

export { Input }
