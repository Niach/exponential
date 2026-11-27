import type { ComponentType, ReactNode } from "react"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"

// #EXP-2169 ("one look"): THE frame and heading of a wide entity page (a
// device, the CI page, a repository): one column, the 24px gutter, the
// page's glyph + its bold title, then its meta and trailing controls. Phones
// replace the heading with the platform's page header.

/** The page column. `wide` = a page whose body needs the room (a run's log). */
export function pageFrameClass(wide = false): string {
  return `mx-auto w-full ${wide ? `max-w-6xl` : `max-w-4xl`} space-y-6 p-4 sm:p-6 ${PAGE_END_CLEARANCE}`
}

export function PageHeading({
  icon: Icon,
  title,
  leading,
  meta,
  trailing,
}: {
  icon?: ComponentType<{ className?: string; "aria-hidden"?: boolean }>
  title: ReactNode
  leading?: ReactNode
  /** Muted bits after the title (a version, a status line). */
  meta?: ReactNode
  trailing?: ReactNode
}) {
  return (
    <div className="flex min-w-0 items-center gap-3" data-testid="page-heading">
      {leading}
      {Icon && <Icon aria-hidden className="size-6 shrink-0 text-muted-foreground" />}
      <h1 className="min-w-0 truncate text-2xl font-bold">{title}</h1>
      {meta}
      {trailing && <div className="ml-auto flex shrink-0 items-center gap-2">{trailing}</div>}
    </div>
  )
}
