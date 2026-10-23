import { children, splitProps, type ComponentProps, type ValidComponent } from "solid-js"
import { Dynamic } from "solid-js/web"
import { ChevronRight, Ellipsis } from "lucide-solid"
import { BREADCRUMB_CLASSES } from "@exponential_app/ui/web/primitives"

import { PaintIcon } from "../icons"
import { cn } from "./cn"

function Breadcrumb(props: ComponentProps<`nav`>) {
  return <nav aria-label="breadcrumb" data-slot="breadcrumb" {...props} />
}

function BreadcrumbList(props: ComponentProps<`ol`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <ol data-slot="breadcrumb-list" class={cn(BREADCRUMB_CLASSES.list, local.class)} {...rest} />
}

function BreadcrumbItem(props: ComponentProps<`li`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <li data-slot="breadcrumb-item" class={cn(BREADCRUMB_CLASSES.item, local.class)} {...rest} />
}

/** React's `asChild` is `as` (a tag or a component). */
function BreadcrumbLink(props: ComponentProps<`a`> & { as?: ValidComponent }) {
  const [local, rest] = splitProps(props, [`class`, `as`])
  return <Dynamic component={local.as ?? `a`} data-slot="breadcrumb-link" class={cn(BREADCRUMB_CLASSES.link, local.class)} {...rest} />
}

function BreadcrumbPage(props: ComponentProps<`span`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <span data-slot="breadcrumb-page" role="link" aria-disabled="true" aria-current="page" class={cn(BREADCRUMB_CLASSES.page, local.class)} {...rest} />
}

function BreadcrumbSeparator(props: ComponentProps<`li`>) {
  const [local, rest] = splitProps(props, [`class`, `children`])
  const resolved = children(() => local.children)
  return (
    <li data-slot="breadcrumb-separator" role="presentation" aria-hidden="true" class={cn(BREADCRUMB_CLASSES.separator, local.class)} {...rest}>
      {resolved() ?? <PaintIcon icon={ChevronRight} />}
    </li>
  )
}

function BreadcrumbEllipsis(props: ComponentProps<`span`>) {
  const [local, rest] = splitProps(props, [`class`])
  return (
    <span data-slot="breadcrumb-ellipsis" role="presentation" aria-hidden="true" class={cn(BREADCRUMB_CLASSES.ellipsis, local.class)} {...rest}>
      <PaintIcon icon={Ellipsis} class={BREADCRUMB_CLASSES.ellipsisIcon} />
      <span class={BREADCRUMB_CLASSES.srOnly}>More</span>
    </span>
  )
}

export { Breadcrumb, BreadcrumbList, BreadcrumbItem, BreadcrumbLink, BreadcrumbPage, BreadcrumbSeparator, BreadcrumbEllipsis }
