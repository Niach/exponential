import { useEffect, useRef, useState, type ReactElement } from "react"
import {
  conceptIcon,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  useIsMobile,
} from "@exp/ui"
import { blocksBadgeLabel } from "@/lib/issue-graph"
import { TeamIssueGraph } from "@/components/issue-graph"

// EXP-980: the ONE relations badge of a list row. Counts come from the list
// (`useTeamIssueGraph`, once per list); the graph behind it mounts only while
// the popover is open, so a board of 500 rows still runs two queries.
//
// Blocked-by reads red (something is in this issue's way), blocking muted (it
// is in something else's way, which is information, not an alarm).
//
// EXP-998: at md+ the web list draws the blocks RAIL instead (`issue-rail.tsx`)
// and the badge stays the phone's affordance — the same pill the natives
// draw. Both open the same popover, `IssueBlocksPopover`.
//
// EXP-1057: hover opens it on BOTH (the rail's dot no longer reveals arrows),
// and leaving closes it after a short grace, so the pointer can travel from
// the trigger into the graph and follow a chip.

/** How long the popover lingers after the pointer leaves trigger or graph. */
const HOVER_CLOSE_DELAY_MS = 150

const BlockedByIcon = conceptIcon(`relation-blocked-by`)
const BlocksIcon = conceptIcon(`relation-blocks`)

/**
 * The mini-graph popover behind a row's blocks affordance: hover opens it on
 * a pointer device, tap opens the sheet on a phone. `trigger` is the
 * element it hangs off; it receives the open-on-hover handler through Radix's
 * `asChild`, so it must forward props.
 */
export function IssueBlocksPopover({
  issueId,
  teamId,
  label,
  trigger,
  openOnHover = true,
}: {
  issueId: string
  teamId: string
  /** The overlay's sheet title on a phone. */
  label: string
  trigger: ReactElement
  /** Pointer hover opens it; `false` = click only. */
  openOnHover?: boolean
}) {
  const isMobile = useIsMobile()
  const [open, setOpen] = useState(false)
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const cancelClose = () => {
    if (closeTimer.current) clearTimeout(closeTimer.current)
    closeTimer.current = null
  }
  const closeSoon = () => {
    if (isMobile || !openOnHover) return
    cancelClose()
    closeTimer.current = setTimeout(() => setOpen(false), HOVER_CLOSE_DELAY_MS)
  }
  useEffect(() => cancelClose, [])

  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      <MobilePopoverTrigger
        asChild
        onMouseEnter={() => {
          if (isMobile || !openOnHover) return
          cancelClose()
          setOpen(true)
        }}
        onMouseLeave={closeSoon}
      >
        {trigger}
      </MobilePopoverTrigger>
      <MobilePopoverContent
        align="start"
        collisionPadding={12}
        mobileTitle={label}
        className="w-auto max-w-[min(36rem,90vw)] p-3"
        data-testid="issue-blocks-overlay"
        onClick={(event) => event.stopPropagation()}
        onMouseEnter={cancelClose}
        onMouseLeave={closeSoon}
      >
        {open && (
          <TeamIssueGraph
            teamId={teamId}
            subjectIds={[issueId]}
            onNavigate={() => setOpen(false)}
          />
        )}
      </MobilePopoverContent>
    </MobilePopover>
  )
}

export function IssueBlocksBadge({
  issueId,
  teamId,
  blockedBy,
  blocking,
  className,
}: {
  issueId: string
  teamId: string
  blockedBy: number
  blocking: number
  className?: string
}) {
  const label = blocksBadgeLabel({ blockedBy, blocking })

  return (
    <IssueBlocksPopover
      issueId={issueId}
      teamId={teamId}
      label={label}
      trigger={
        <button
          type="button"
          aria-label={label}
          title={label}
          data-testid="issue-blocks-badge"
          className={`flex h-5 shrink-0 items-center gap-1.5 rounded-full border border-glass-stroke bg-glass-row px-1.5 text-[11px] tabular-nums text-muted-foreground hover:border-glass-stroke-strong ${className ?? ``}`}
          // The Radix trigger owns the toggle; this only keeps the click off
          // the row underneath (which would open the issue).
          onClick={(event) => event.stopPropagation()}
        >
          {blockedBy > 0 && (
            <span className="flex items-center gap-0.5 text-destructive">
              <BlockedByIcon className="size-3" />
              {blockedBy}
            </span>
          )}
          {blocking > 0 && (
            <span className="flex items-center gap-0.5">
              <BlocksIcon className="size-3" />
              {blocking}
            </span>
          )}
        </button>
      }
    />
  )
}
