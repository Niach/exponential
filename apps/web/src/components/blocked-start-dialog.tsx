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
import { useTeamIssueGraph } from "@/hooks/use-team-issue-graph"
import { blockGraph } from "@/lib/issue-graph"
import {
  BLOCKED_BATCH_BODY,
  BLOCKED_BATCH_TITLE,
  BLOCKED_START_BODY_PREFIX,
  BLOCKED_START_BODY_SUFFIX,
  BLOCKED_START_TITLE,
  stackDisabledNote,
  stackDisabledReason,
  START_ANYWAY_LABEL,
  STACKED_PR_LABEL,
} from "@/lib/stack-start"

// EXP-897: the third start mode. Starting a BLOCKED issue used to silently cut
// its branch from the board's base, so the run either waited on work that was
// not there or re-did it. The composer asks instead: start anyway, or start a
// STACKED pull request — cut from the blocker's branch, based on its PR, so
// the diff shows only this issue's own work and the run builds the foundation
// first if nobody has.
//
// EXP-980: the dialog shows the TRANSITIVE chain as the mini-graph (the direct
// blockers alone hid how deep a stack would go), asks for a BATCH too, and
// never hides "Stacked PR": it is disabled with the one reason that applies
// (`stackDisabledReason`: a cycle, a batch, a machine below `stacked-start`).
//
// The copy is byte-locked ×4 (`lib/stack-start.ts`, iOS `StackStart.swift`,
// Android `StackStart.kt`, desktop `chat_launch`).

export function BlockedStartDialog(props: {
  open: boolean
  teamId: string | undefined
  /** The checked issues, in pick order. */
  pickedIds: readonly string[]
  /** `openBlockersOfSet(...)` — never empty while the dialog is up. */
  blockers: readonly Issue[]
  busy?: boolean
  /** The target machine advertises `stacked-start`. */
  canStack?: boolean
  onOpenChange: (open: boolean) => void
  onStartAnyway: () => void
  onStartStacked: () => void
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
  canStack = true,
  onOpenChange,
  onStartAnyway,
  onStartStacked,
}: {
  teamId: string
  pickedIds: readonly string[]
  blockers: readonly Issue[]
  busy?: boolean
  canStack?: boolean
  onOpenChange: (open: boolean) => void
  onStartAnyway: () => void
  onStartStacked: () => void
}) {
  const batch = pickedIds.length > 1
  const { relations, issues } = useTeamIssueGraph(teamId)
  const hasCycle = blockGraph(pickedIds, relations, issues).hasCycle
  const reason = stackDisabledReason({
    pickedCount: pickedIds.length,
    canStack,
    hasCycle,
  })
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
                  preview={false}
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
      {reason && (
        <div
          className="text-xs text-muted-foreground"
          data-testid="blocked-start-stack-note"
        >
          {stackDisabledNote(reason)}
        </div>
      )}
      <DialogFooter>
        <DialogCancel onClick={() => onOpenChange(false)} />
        <Button variant="outline" disabled={busy} onClick={onStartAnyway}>
          {START_ANYWAY_LABEL}
        </Button>
        <Button disabled={busy || reason !== null} onClick={onStartStacked}>
          {STACKED_PR_LABEL}
        </Button>
      </DialogFooter>
    </DialogContent>
  )
}
