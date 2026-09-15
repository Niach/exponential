import type { LucideIcon } from "lucide-react"

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
