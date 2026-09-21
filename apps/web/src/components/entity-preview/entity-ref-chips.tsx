import { useMemo, type ReactNode } from "react"
import { groupPreviewRefs } from "@exp/domain-contract/entity-preview"
import { cn } from "@/lib/utils"
import type { EntityRef } from "@/lib/mcp/preview"
import { EntityRefChip } from "./entity-ref-chip"

// EXP-920: the chip ROW under a settled Exponential tool row — one chip per
// group of `groupPreviewRefs` (a `list` ref absorbs the member refs behind
// it into one chip whose card lists them), wrapping. The transcript indents
// it under the caption exactly where the single issue chip used to sit.

export function EntityRefChips({
  refs,
  className,
}: {
  refs: readonly EntityRef[]
  className?: string
}): ReactNode {
  const groups = useMemo(() => groupPreviewRefs(refs), [refs])
  if (groups.length === 0) return null
  return (
    <div
      data-testid="entity-ref-chips"
      className={cn(`flex flex-wrap items-center gap-1`, className)}
    >
      {groups.map((group, index) => (
        <EntityRefChip
          key={`${group.ref.kind}:${group.ref.id}:${index}`}
          entityRef={group.ref}
          members={group.members}
        />
      ))}
    </div>
  )
}
