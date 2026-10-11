// EXP-1274: ONE empty state, by id. `<Empty id="inbox" />` resolves the ×4
// copy (`lib/empty-states.ts`) and the concept icon into `@exp/ui`'s
// `EmptyState`; `children` = the page's actions row (a `page` state's
// `action` label is what its primary button says). `size` may override the
// fixture's for a page state drawn inside a column (never the other way).
import { conceptIcon, EmptyState, type EmptyStateSize } from "@exp/ui"
import { emptyStateCopy, type EmptyStateId } from "@/lib/empty-states"

export function Empty({
  id,
  size,
  className,
  children,
}: {
  id: EmptyStateId
  size?: EmptyStateSize
  className?: string
  children?: React.ReactNode
}) {
  const copy = emptyStateCopy(id)
  return (
    <EmptyState
      icon={conceptIcon(copy.icon as Parameters<typeof conceptIcon>[0])}
      title={copy.title}
      description={copy.description}
      size={size ?? copy.size}
      className={className}
    >
      {children}
    </EmptyState>
  )
}
