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
import type { Issue } from "@/db/schema"
import {
  BLOCKED_START_BODY_PREFIX,
  BLOCKED_START_BODY_SUFFIX,
  BLOCKED_START_TITLE,
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
// The copy is byte-locked ×4 (`lib/stack-start.ts`, iOS `StackStart.swift`,
// Android `StackStart.kt`, desktop `chat_launch::stack_label`); the natives'
// alerts cannot host chips and say `#IDENT` instead (documented divergence).

export function BlockedStartDialog({
  open,
  blockers,
  busy = false,
  onOpenChange,
  onStartAnyway,
  onStartStacked,
}: {
  open: boolean
  /** `openBlockers(...)` — never empty while the dialog is up. */
  blockers: readonly Issue[]
  busy?: boolean
  onOpenChange: (open: boolean) => void
  onStartAnyway: () => void
  onStartStacked: () => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent mobile="alert" data-testid="blocked-start-dialog">
        <DialogHeader>
          <DialogTitle>{BLOCKED_START_TITLE}</DialogTitle>
          <DialogDescription asChild>
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
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogCancel onClick={() => onOpenChange(false)} />
          <Button variant="outline" disabled={busy} onClick={onStartAnyway}>
            {START_ANYWAY_LABEL}
          </Button>
          <Button disabled={busy} onClick={onStartStacked}>
            {STACKED_PR_LABEL}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
