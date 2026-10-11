import type { ComponentProps } from "react"
import type { LucideIcon } from "lucide-react"

import { cn } from "./cn"
import { IconDisc } from "./icon-disc"

// EXP-1274 — ONE empty state, two sizes (the catalog `EmptyState` macro's
// `size`; the ×4 contract is `@exp/domain-contract/fixtures/empty-state.json`).
//
// `page`: what a whole screen or pane says when it has nothing — the 48px
// icon disc, a semibold title, an optional muted sentence that TEACHES the
// next step, and an optional actions row, centred on a column of at most
// 28rem.
//
// `list`: what a list inside a section, a sidebar or a picker says — a bare
// 20px muted glyph over ONE muted line, centred, 24px of vertical padding.
// It never carries a description or a button: the section band's trailing
// pill (or the picker's own field) is the call to action. Without an icon it
// is the line `CommandEmpty` draws inside a `Command`.
export const EMPTY_STATE_SIZES = [`page`, `list`] as const
export type EmptyStateSize = (typeof EMPTY_STATE_SIZES)[number]

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
        className={cn(
          `flex w-full flex-col items-center gap-1.5 px-3 py-6 text-center text-sm text-muted-foreground`,
          className
        )}
      >
        {Icon && (
          <Icon
            aria-hidden="true"
            focusable="false"
            className="size-5 shrink-0 text-muted-foreground/60"
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
      className={cn(
        `mx-auto flex max-w-md flex-col items-center gap-3 px-6 py-12 text-center`,
        className
      )}
    >
      {Icon && <IconDisc icon={Icon} />}
      <h2 className="text-lg font-semibold">{title}</h2>
      {description && (
        <p className="text-sm text-muted-foreground">{description}</p>
      )}
      {children && (
        <div className="mt-2 flex flex-wrap items-center justify-center gap-2">
          {children}
        </div>
      )}
    </div>
  )
}

// The compact in-list empty line = `EmptyState size="list"` without a glyph
// (the emoji picker's grid, a filtered admin table). Kept as its own name
// because a `children` string reads better than `title` at those call sites.
export function ListEmpty({
  className,
  children,
  ...props
}: Omit<ComponentProps<`p`>, `title`>) {
  return (
    <p
      data-slot="list-empty"
      role="status"
      className={cn(
        `px-3 py-6 text-center text-sm text-muted-foreground`,
        className
      )}
      {...props}
    >
      {children}
    </p>
  )
}

// EXP-962: the CLICKABLE empty state — a dashed, full-width nudge standing
// where the first item will go, that IS the call to action (the actions
// panel's "No custom actions yet / Describe one and your agent will build
// it"). Dashed because the box is a placeholder for the row it invites; a
// button because the shortest path to the row is the box itself. `EmptyState`
// teaches a PAGE or reports a list, this one starts the list. Desktop
// `actions_view` draws the same dashed strip.
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
      className={cn(
        `flex w-full cursor-pointer flex-col items-start gap-1 rounded-md border border-dashed border-glass-stroke-strong p-3 text-left text-sm text-muted-foreground transition-colors duration-fast outline-none hover:bg-glass-row hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50`,
        className
      )}
      {...props}
    >
      <span className="flex items-center gap-2">
        <Icon className="size-4 shrink-0" />
        {title}
      </span>
      <span className="text-xs">{description}</span>
    </button>
  )
}
