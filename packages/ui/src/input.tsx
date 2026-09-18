import * as React from "react"

import { cn } from "./cn"

// EXP-941 — "undress the field, the row is the chrome".
//
// Seven surfaces (the glass input/search rows here, the board form's title,
// the action editor's name, the MCP server rows, the support composer, the
// issue title field) strip an `Input` down to bare text so the row or card
// AROUND it draws the box. That was one long class string copy-pasted seven
// times; it is this constant now. Compose with `cn(BARE_FIELD_CLASS, …)` and
// add only what the surface itself changes (alignment, padding, colour).
export const BARE_FIELD_CLASS = `h-auto min-w-0 flex-1 rounded-none border-0 bg-transparent p-0 text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm`

// EXP-616: glass by default — the card fill + card hairline ARE the field, so
// callers never re-dress an Input locally. EXP-698: 36 tall, 12 radius, and
// focus is the STROKE stepping to active — the 3px halo ring is gone from
// every text field. A constant so a field that is not an `Input` (cmdk's own
// input in `CommandInput`'s `field` variant) wears the same chrome.
export const FIELD_CHROME_CLASS = `h-9 w-full min-w-0 rounded-lg border border-glass-stroke-card bg-glass-card px-3 py-1 text-base shadow-none transition-[color,border-color] duration-fast outline-none selection:bg-primary selection:text-primary-foreground file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-foreground/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 md:text-sm focus-visible:border-glass-stroke-active focus-visible:ring-0`

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<`input`>>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        ref={ref}
        type={type}
        data-slot="input"
        className={cn(
          FIELD_CHROME_CLASS,
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
