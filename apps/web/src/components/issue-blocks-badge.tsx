import { useState } from "react"
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

const BlockedByIcon = conceptIcon(`relation-blocked-by`)
const BlocksIcon = conceptIcon(`relation-blocks`)

export function IssueBlocksBadge({
  issueId,
  teamId,
  blockedBy,
  blocking,
}: {
  issueId: string
  teamId: string
  blockedBy: number
  blocking: number
}) {
  const isMobile = useIsMobile()
  const [open, setOpen] = useState(false)
  const label = blocksBadgeLabel({ blockedBy, blocking })

  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      <MobilePopoverTrigger asChild>
        <button
          type="button"
          aria-label={label}
          title={label}
          data-testid="issue-blocks-badge"
          className="flex h-5 shrink-0 items-center gap-1.5 rounded-full border border-glass-stroke bg-glass-row px-1.5 text-[11px] tabular-nums text-muted-foreground hover:border-glass-stroke-strong"
          // The Radix trigger owns the toggle; this only keeps the click off
          // the row underneath (which would open the issue).
          onClick={(event) => event.stopPropagation()}
          onMouseEnter={() => {
            if (!isMobile) setOpen(true)
          }}
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
      </MobilePopoverTrigger>
      <MobilePopoverContent
        align="start"
        collisionPadding={12}
        mobileTitle={label}
        className="w-auto max-w-[min(36rem,90vw)] p-3"
        data-testid="issue-blocks-overlay"
        onClick={(event) => event.stopPropagation()}
        onMouseLeave={() => {
          if (!isMobile) setOpen(false)
        }}
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
