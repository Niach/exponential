import * as React from "react"
import type { VariantProps } from "class-variance-authority"
import { ALERT_CLASSES, alertVariants } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

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
      className={cn(ALERT_CLASSES.title, className)}
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
      className={cn(ALERT_CLASSES.description, className)}
      {...props}
    />
  )
}

export { Alert, AlertTitle, AlertDescription, alertVariants }
