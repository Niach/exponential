import { Fragment } from "react"
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
  BLOCKED_START_BODY_SUFFIX_STACKABLE,
  BLOCKED_START_TITLE,
  stackDisabledNote,
  type StackDisabledReason,
  stackPlanNote,
  STACKED_PR_LABEL,
  START_ANYWAY_LABEL,
} from "@/lib/blocked-start"

// EXP-980: starting a BLOCKED issue asks first over the transitive chain
// drawn as the mini-graph. A batch asks too, about the blockers outside it.
//
// SLOP-3: Cancel · Start anyway · Stacked PR. "Stacked PR" builds the whole
// dependency LINE bottom-up: it starts the line's first issue with the base
// instruction and the line paragraph in its prompt (`stackedStartPrompt`).
// It is never hidden: while `stackPlan` returns a reason it is disabled and
// the reason's note sits under the graph; enabled with 2+ issues to start, the
// plan note says which starts first. The copy is byte-locked ×4
// (`lib/blocked-start.ts`, fixture `blocked-start.json`).

export function BlockedStartDialog(props: {
  open: boolean
  teamId: string | undefined
  /** The checked issues, in pick order. */
  pickedIds: readonly string[]
  /** `openBlockersOfSet(...)`: never empty while the dialog is up. */
  blockers: readonly Issue[]
  /** `stackPlan(...)`'s reason; null = "Stacked PR" is enabled. */
  stackReason: StackDisabledReason | null
  /** The issue the reason's note names. */
  stackIdent?: string | null
  /** The plan's issues to start, bottom first (empty while disabled). */
  stackRun?: readonly string[]
  busy?: boolean
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
  stackReason,
  stackIdent = null,
  stackRun = [],
  busy = false,
  onOpenChange,
  onStartAnyway,
  onStartStacked,
}: {
  teamId: string
  pickedIds: readonly string[]
  blockers: readonly Issue[]
  stackReason: StackDisabledReason | null
  stackIdent?: string | null
  stackRun?: readonly string[]
  busy?: boolean
  onOpenChange: (open: boolean) => void
  onStartAnyway: () => void
  onStartStacked: () => void
}) {
  const planNote = stackReason === null ? stackPlanNote(stackRun) : null
  const batch = pickedIds.length > 1
  const suffix =
    stackReason === null
      ? BLOCKED_START_BODY_SUFFIX_STACKABLE
      : BLOCKED_START_BODY_SUFFIX
  const [, suffixGlue = ``, suffixRest = ``] = /^(\S*)(.*)$/s.exec(suffix) ?? []
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
            // The chips flow INLINE in the sentence (they are inline-flex,
            // `align-middle`), and the last one carries the suffix's leading
            // "." in a nowrap span, so a wrap never starts a line with it.
            <div className="leading-6" data-testid="blocked-start-sentence">
              {BLOCKED_START_BODY_PREFIX}
              {blockers.map((blocker, index) => {
                const chip = (
                  <IssueChip
                    issue={blocker}
                    testId={`blocked-start-chip-${blocker.identifier}`}
                  />
                )
                return index < blockers.length - 1 ? (
                  <Fragment key={blocker.id}>
                    {chip}
                    {`, `}
                  </Fragment>
                ) : (
                  <span key={blocker.id} className="whitespace-nowrap">
                    {chip}
                    {suffixGlue}
                  </span>
                )
              })}
              {suffixRest}
            </div>
          )}
        </DialogDescription>
      </DialogHeader>
      <TeamIssueGraph
        teamId={teamId}
        subjectIds={pickedIds}
        onNavigate={() => onOpenChange(false)}
      />
      {stackReason !== null ? (
        <div
          className="text-xs text-muted-foreground"
          data-testid="blocked-start-stack-note"
        >
          {stackDisabledNote(stackReason, stackIdent ?? ``)}
        </div>
      ) : planNote !== null ? (
        <div
          className="text-xs text-muted-foreground"
          data-testid="blocked-start-plan-note"
        >
          {planNote}
        </div>
      ) : null}
      <DialogFooter>
        <DialogCancel onClick={() => onOpenChange(false)} />
        <Button variant="outline" disabled={busy} onClick={onStartAnyway}>
          {START_ANYWAY_LABEL}
        </Button>
        <Button
          disabled={busy || stackReason !== null}
          onClick={onStartStacked}
          data-testid="blocked-start-stacked"
        >
          {STACKED_PR_LABEL}
        </Button>
      </DialogFooter>
    </DialogContent>
  )
}
