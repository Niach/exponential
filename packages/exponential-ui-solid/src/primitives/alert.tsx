import { splitProps, type ComponentProps } from "solid-js"
import { ALERT_CLASSES, alertVariants, type AlertVariantProps } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

function Alert(props: ComponentProps<`div`> & AlertVariantProps) {
  const [local, rest] = splitProps(props, [`class`, `variant`])
  return <div data-slot="alert" role="alert" class={cn(alertVariants({ variant: local.variant }), local.class)} {...rest} />
}

function AlertTitle(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="alert-title" class={cn(ALERT_CLASSES.title, local.class)} {...rest} />
}

function AlertDescription(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="alert-description" class={cn(ALERT_CLASSES.description, local.class)} {...rest} />
}

export { Alert, AlertTitle, AlertDescription, alertVariants }
