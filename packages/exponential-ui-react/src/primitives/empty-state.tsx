import type { ComponentProps } from "react"
import type { LucideIcon } from "lucide-react"
import { EMPTY_STATE_CLASSES, type EmptyStateSize } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"
import { IconDisc } from "./icon-disc"

export { EMPTY_STATE_SIZES } from "@exponential_app/eui/web/primitives"
export type { EmptyStateSize } from "@exponential_app/eui/web/primitives"

export function EmptyState({
  icon: Icon,
  title,
  description,
  size = `page`,
  className,
  children,
}: {
  icon?: LucideIcon
  title: string
  description?: string
  size?: EmptyStateSize
  className?: string
  children?: React.ReactNode
}) {
  if (size === `list`) {
    return (
      <div
        data-slot="empty-state"
        data-size="list"
        role="status"
        className={cn(EMPTY_STATE_CLASSES.list, className)}
      >
        {Icon && (
          <Icon
            aria-hidden="true"
            focusable="false"
            className={EMPTY_STATE_CLASSES.listIcon}
          />
        )}
        <span>{title}</span>
      </div>
    )
  }
  return (
    <div
      data-slot="empty-state"
      data-size="page"
      role="status"
      className={cn(EMPTY_STATE_CLASSES.page, className)}
    >
      {Icon && <IconDisc icon={Icon} />}
      <h2 className={EMPTY_STATE_CLASSES.pageTitle}>{title}</h2>
      {description && (
        <p className={EMPTY_STATE_CLASSES.pageDescription}>{description}</p>
      )}
      {children && (
        <div className={EMPTY_STATE_CLASSES.pageActions}>
          {children}
        </div>
      )}
    </div>
  )
}

export function ListEmpty({
  className,
  children,
  ...props
}: Omit<ComponentProps<`p`>, `title`>) {
  return (
    <p
      data-slot="list-empty"
      role="status"
      className={cn(EMPTY_STATE_CLASSES.listEmpty, className)}
      {...props}
    >
      {children}
    </p>
  )
}

export function EmptyCta({
  icon: Icon,
  title,
  description,
  className,
  type,
  ...props
}: Omit<ComponentProps<`button`>, `children` | `title`> & {
  icon: LucideIcon
  title: string
  description: string
}) {
  return (
    <button
      type={type ?? `button`}
      data-slot="empty-cta"
      className={cn(EMPTY_STATE_CLASSES.cta, className)}
      {...props}
    >
      <span className={EMPTY_STATE_CLASSES.ctaTitle}>
        <Icon className={EMPTY_STATE_CLASSES.ctaIcon} />
        {title}
      </span>
      <span className={EMPTY_STATE_CLASSES.ctaDescription}>{description}</span>
    </button>
  )
}
