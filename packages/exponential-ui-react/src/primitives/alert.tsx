import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "./cn"

// EXP-941 — the inline banner. The admin console carried two byte-identical
// copies of the destructive one (`rounded-md border border-destructive/50
// bg-destructive/10 px-3 py-2 text-sm text-destructive`); this is that recipe
// as the shadcn `Alert`, so the next error strip is a component and not a
// third copy. The grid template is shadcn's: a leading glyph gets its own
// column only when one is present, and the title/description stack in the
// second.
const alertVariants = cva(
  `relative grid w-full grid-cols-[0_1fr] items-start gap-y-0.5 rounded-md border px-3 py-2 text-sm has-[>svg]:grid-cols-[calc(var(--spacing)*4)_1fr] has-[>svg]:gap-x-3 [&>svg]:size-4 [&>svg]:translate-y-0.5 [&>svg]:text-current`,
  {
    variants: {
      variant: {
        default: `border-glass-stroke-card bg-glass-card text-foreground`,
        destructive: `border-destructive/50 bg-destructive/10 text-destructive`,
      },
    },
    defaultVariants: {
      variant: `default`,
    },
  }
)

function Alert({
  className,
  variant,
  ...props
}: React.ComponentProps<`div`> & VariantProps<typeof alertVariants>) {
  return (
    <div
      data-slot="alert"
      role="alert"
      className={cn(alertVariants({ variant }), className)}
      {...props}
    />
  )
}

function AlertTitle({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="alert-title"
      className={cn(
        `col-start-2 line-clamp-1 min-h-4 font-medium tracking-tight`,
        className
      )}
      {...props}
    />
  )
}

function AlertDescription({
  className,
  ...props
}: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="alert-description"
      className={cn(
        `col-start-2 grid justify-items-start gap-1 text-sm [&_p]:leading-relaxed`,
        className
      )}
      {...props}
    />
  )
}

export { Alert, AlertTitle, AlertDescription, alertVariants }
