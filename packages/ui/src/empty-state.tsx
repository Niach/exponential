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
