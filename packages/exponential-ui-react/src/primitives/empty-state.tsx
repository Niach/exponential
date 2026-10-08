import type { ComponentProps } from "react"
import type { LucideIcon } from "lucide-react"

import { cn } from "./cn"
import { IconDisc } from "./icon-disc"

// A centered teaching empty state: icon + title + description + an actions slot.
export function EmptyState({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: LucideIcon
  title: string
  description: string
  children?: React.ReactNode
}) {
  return (
    <div className="mx-auto flex max-w-md flex-col items-center gap-3 px-6 py-12 text-center">
      <IconDisc icon={Icon} />
      <h2 className="text-lg font-semibold">{title}</h2>
      <p className="text-sm text-muted-foreground">{description}</p>
      {children && (
        <div className="mt-2 flex flex-wrap items-center justify-center gap-2">
          {children}
        </div>
      )}
    </div>
  )
}

// The compact in-list empty line — the same line `CommandEmpty` draws inside a
// `Command`, for the lists that have no Command around them (the emoji
// picker's grid). `EmptyState` above is the page-sized one.
export function ListEmpty({ className, children, ...props }: ComponentProps<`p`>) {
  return (
    <p
      data-slot="list-empty"
      className={cn(
        `px-1 py-6 text-center text-sm text-muted-foreground`,
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
// teaches a PAGE, `ListEmpty` reports a filtered list, this one starts the
// list. Desktop `actions_view` draws the same dashed strip.
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
