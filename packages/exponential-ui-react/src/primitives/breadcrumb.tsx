import * as React from "react"
import { ChevronRight, Ellipsis } from "lucide-react"
import { Slot } from "radix-ui"
import { BREADCRUMB_CLASSES } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

function Breadcrumb({ ...props }: React.ComponentProps<`nav`>) {
  return <nav aria-label="breadcrumb" data-slot="breadcrumb" {...props} />
}

function BreadcrumbList({ className, ...props }: React.ComponentProps<`ol`>) {
  return <ol data-slot="breadcrumb-list" className={cn(BREADCRUMB_CLASSES.list, className)} {...props} />
}

function BreadcrumbItem({ className, ...props }: React.ComponentProps<`li`>) {
  return <li data-slot="breadcrumb-item" className={cn(BREADCRUMB_CLASSES.item, className)} {...props} />
}

function BreadcrumbLink({ asChild, className, ...props }: React.ComponentProps<`a`> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : `a`
  return <Comp data-slot="breadcrumb-link" className={cn(BREADCRUMB_CLASSES.link, className)} {...props} />
}

function BreadcrumbPage({ className, ...props }: React.ComponentProps<`span`>) {
  return <span data-slot="breadcrumb-page" role="link" aria-disabled="true" aria-current="page" className={cn(BREADCRUMB_CLASSES.page, className)} {...props} />
}

function BreadcrumbSeparator({ children, className, ...props }: React.ComponentProps<`li`>) {
  return (
    <li data-slot="breadcrumb-separator" role="presentation" aria-hidden="true" className={cn(BREADCRUMB_CLASSES.separator, className)} {...props}>
      {children ?? <ChevronRight />}
    </li>
  )
}

function BreadcrumbEllipsis({ className, ...props }: React.ComponentProps<`span`>) {
  return (
    <span data-slot="breadcrumb-ellipsis" role="presentation" aria-hidden="true" className={cn(BREADCRUMB_CLASSES.ellipsis, className)} {...props}>
      <Ellipsis className={BREADCRUMB_CLASSES.ellipsisIcon} />
      <span className={BREADCRUMB_CLASSES.srOnly}>More</span>
    </span>
  )
}

export { Breadcrumb, BreadcrumbList, BreadcrumbItem, BreadcrumbLink, BreadcrumbPage, BreadcrumbSeparator, BreadcrumbEllipsis }
