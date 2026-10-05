import { Fragment } from "react"
import { Prompt } from "@exp/ui"
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

export function BlockedStartDialog({
  open,
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
  const planNote = stackReason === null ? stackPlanNote(stackRun) : null
  const batch = pickedIds.length > 1
  const suffix =
    stackReason === null
      ? BLOCKED_START_BODY_SUFFIX_STACKABLE
      : BLOCKED_START_BODY_SUFFIX
  const [, suffixGlue = ``, suffixRest = ``] = /^(\S*)(.*)$/s.exec(suffix) ?? []
  return (
    // EXP-1215: the ONE Prompt card. The sentence-with-chips is the body,
    // the graph + note the content slot (the graph queries the team, and the
    // card's content mounts only while open).
    <Prompt
      open={open && teamId !== undefined}
      onOpenChange={onOpenChange}
      busy={busy}
      data-testid="blocked-start-dialog"
      title={batch ? BLOCKED_BATCH_TITLE : BLOCKED_START_TITLE}
      body={
        batch ? (
          BLOCKED_BATCH_BODY
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
        )
      }
      actions={[
        { label: `Cancel` },
        { label: START_ANYWAY_LABEL, onSelect: onStartAnyway },
        {
          label: STACKED_PR_LABEL,
          role: `primary`,
          disabled: stackReason !== null,
          onSelect: onStartStacked,
          testId: `blocked-start-stacked`,
        },
      ]}
    >
      {teamId !== undefined ? (
        <div className="flex flex-col gap-2">
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
        </div>
      ) : null}
    </Prompt>
  )
}
