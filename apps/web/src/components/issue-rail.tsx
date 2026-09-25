import { forwardRef, type ComponentProps } from "react"
import { cn } from "@/lib/utils"
import { blocksBadgeLabel } from "@/lib/issue-graph"
import { railNode, RAIL_NODE_WIDTH, type RailNode } from "@/lib/issue-rail"
import { IssueBlocksPopover } from "@/components/issue-blocks-badge"

// EXP-998 → EXP-1057: the blocks rail, DRAWN. One absolutely positioned dot
// per row at the row's right edge (the grid reserves the column,
// `--issue-rail`): a ring in red when something open is in the issue's way, a
// filled dot when the issue only blocks others. Hovering (or focusing) the
// dot opens THE mini-graph — the same popover as the phone's badge — so the
// relations read as a graph, not as arrows between far-apart rows.

/** The node dot, the popover's trigger: forwards Radix's props onto the
 *  button. */
const RailNodeButton = forwardRef<
  HTMLButtonElement,
  { kind: RailNode; label: string } & Omit<ComponentProps<`button`>, `children`>
>(function RailNodeButton({ kind, label, ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      title={label}
      data-testid="issue-rail-node"
      data-kind={kind}
      className="group/node pointer-events-auto absolute inset-y-0 right-0 flex items-center justify-center outline-none"
      style={{ width: RAIL_NODE_WIDTH }}
      // The Radix trigger owns the toggle; this keeps the click off the row
      // underneath (which would open the issue).
      onClick={(event) => event.stopPropagation()}
      {...rest}
    >
      {/* Hover and focus grow a soft halo. */}
      <span
        className={cn(
          `block size-2.5 rounded-full border-2 transition-[box-shadow,transform,border-color,background-color] duration-fast group-hover/node:scale-110 group-hover/node:shadow-[0_0_0_3px] group-focus-visible/node:shadow-[0_0_0_3px] group-data-[state=open]/node:shadow-[0_0_0_3px]`,
          kind === `blocked`
            ? `border-destructive bg-background shadow-destructive/25`
            : `border-muted-foreground bg-muted-foreground shadow-foreground/15 group-hover/node:border-foreground group-hover/node:bg-foreground`
        )}
      />
    </button>
  )
})

/**
 * A ROW's rail dot. The caller positions it (`className`) over the reserved
 * column; `width` = the rail's width (0 = no rail in this list).
 */
export function IssueRailLayer({
  blockedBy,
  blocking,
  width,
  issueId,
  teamId,
  className,
}: {
  blockedBy: number
  blocking: number
  width: number
  issueId: string
  /** The popover's graph scope; absent = an inert dot. */
  teamId: string | undefined
  className?: string
}) {
  const counts = { blockedBy, blocking }
  const kind = railNode(counts)
  if (kind === null || width === 0) return null
  const label = blocksBadgeLabel(counts)
  return (
    <div
      className={cn(`pointer-events-none absolute inset-y-0`, className)}
      style={{ width }}
      data-testid="issue-rail"
    >
      {teamId ? (
        <IssueBlocksPopover
          issueId={issueId}
          teamId={teamId}
          label={label}
          trigger={<RailNodeButton kind={kind} label={label} />}
        />
      ) : (
        <RailNodeButton kind={kind} label={label} />
      )}
    </div>
  )
}
