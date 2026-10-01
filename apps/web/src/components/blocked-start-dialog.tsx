import {
  Button,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@exp/ui"
import { IssueChip } from "@/components/issue-chip"
import { TeamIssueGraph } from "@/components/issue-graph"
import type { Issue } from "@/db/schema"
import {
  BLOCKED_BATCH_BODY,
  BLOCKED_BATCH_TITLE,
  BLOCKED_START_BODY_PREFIX,
  BLOCKED_START_BODY_SUFFIX,
  BLOCKED_START_TITLE,
  START_ANYWAY_LABEL,
} from "@/lib/issue-graph"

// EXP-980: starting a BLOCKED issue asks first — Cancel or Start anyway —
// over the transitive chain drawn as the mini-graph. A batch asks too, about
// the blockers outside it. The copy is byte-locked ×4 (`lib/issue-graph.ts`).

export function BlockedStartDialog(props: {
  open: boolean
  teamId: string | undefined
  /** The checked issues, in pick order. */
  pickedIds: readonly string[]
  /** `openBlockersOfSet(...)` — never empty while the dialog is up. */
  blockers: readonly Issue[]
  busy?: boolean
  onOpenChange: (open: boolean) => void
  onStartAnyway: () => void
}) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      {/* The body queries the team graph, so it mounts only while open. */}
      {props.open && props.teamId ? (
        <BlockedStartBody {...props} teamId={props.teamId} />
      ) : null}
    </Dialog>
  )
}

function BlockedStartBody({
  teamId,
  pickedIds,
  blockers,
  busy = false,
  onOpenChange,
  onStartAnyway,
}: {
  teamId: string
  pickedIds: readonly string[]
  blockers: readonly Issue[]
  busy?: boolean
  onOpenChange: (open: boolean) => void
  onStartAnyway: () => void
}) {
  const batch = pickedIds.length > 1
  return (
    <DialogContent mobile="alert" data-testid="blocked-start-dialog">
      <DialogHeader>
        <DialogTitle>
          {batch ? BLOCKED_BATCH_TITLE : BLOCKED_START_TITLE}
        </DialogTitle>
        <DialogDescription asChild>
          {batch ? (
            <div>{BLOCKED_BATCH_BODY}</div>
          ) : (
            <div className="flex flex-wrap items-center gap-1.5">
              <span>{BLOCKED_START_BODY_PREFIX.trimEnd()}</span>
              {blockers.map((blocker) => (
                <IssueChip
                  key={blocker.id}
                  issue={blocker}
                  testId={`blocked-start-chip-${blocker.identifier}`}
                />
              ))}
              <span>{BLOCKED_START_BODY_SUFFIX.trimStart()}</span>
            </div>
          )}
        </DialogDescription>
      </DialogHeader>
      <TeamIssueGraph
        teamId={teamId}
        subjectIds={pickedIds}
        onNavigate={() => onOpenChange(false)}
      />
      <DialogFooter>
        <DialogCancel onClick={() => onOpenChange(false)} />
        <Button disabled={busy} onClick={onStartAnyway}>
          {START_ANYWAY_LABEL}
        </Button>
      </DialogFooter>
    </DialogContent>
  )
}
