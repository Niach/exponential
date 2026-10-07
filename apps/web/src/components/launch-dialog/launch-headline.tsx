import { contract } from "@exp/domain-contract"

import { SubjectChips } from "@/components/launch-dialog/subject-chips"
import { IssueChip } from "@/components/issue-chip"
import { useIssueRefs } from "@/components/issue-ref-provider"
import type {
  FixConflictsView,
  LaunchComposerModel,
  LaunchSubject,
} from "@/hooks/use-launch-composer"
import { cn } from "@/lib/utils"

// EXP-1019: the launcher's HEADLINE — what this run is about, as the main
// element of the composer.
//
// The subject used to be a chip in the composer card's leading row, level
// with the text field's chrome, so a prefilled action read as decoration on
// a prompt box: people did not know the box was now optional and the thing
// they picked was already loaded. The subject leads instead — "Run <action>",
// "Implement <issues>" — and the field below it drops to "Additional
// instructions (optional)…".
//
// The verbs are the contract's (`contract.composerUi`, ×4): the same two
// words on the web dialog, the Agent page, the IDE's start-coding dialog and
// the two native composers. The CHIPS are each client's own; on the web they
// are the very same `SubjectChips` the card used to carry, ✕ and all, so
// removing the last one still turns the run back into a chat.
//
// EXP-1233: the Fix merge conflicts builtin with a PICKED pull request is
// the third verb — "Fix merge conflicts" leads, and the chips are the PR's
// own issue chips (the ones "Implement" draws), so a refused merge lands on
// a headline that names the work, not an action. Their ✕ clears the PICK
// (back to "Run Fix merge conflicts" with the picker), never the action.

const { composerUi } = contract

/** The verb in front of the chips — pure, so the three cases are a test.
 *  `fixConflictsPicked` = the builtin with a pull request picked. */
export function launchHeadlineVerb(
  subject: LaunchSubject,
  fixConflictsPicked = false
): string {
  if (subject === null) return composerUi.chatHeadline
  if (subject.kind === `action`) {
    return fixConflictsPicked
      ? composerUi.fixConflictsHeadline
      : composerUi.runHeadline
  }
  return composerUi.implementHeadline
}

/**
 * The headline as PLAIN TEXT — the dialog's accessible name, and what a test
 * can assert without walking chips. Falls back to the subject's shape while
 * its rows are still syncing.
 */
export function launchHeadlineText(model: LaunchComposerModel): string {
  const { subject, fixConflicts } = model
  const fixPicked = Boolean(fixConflicts?.pr)
  const verb = launchHeadlineVerb(subject, fixPicked)
  if (subject === null) return verb
  if (subject.kind === `action`) {
    if (fixConflicts?.pr) {
      const named = fixConflicts.pr.issues.map((issue) => issue.identifier)
      return `${verb} ${named.join(`, `)}`
    }
    return `${verb} ${model.selectedAction?.name ?? `action`}`
  }
  const named = model.checkedIssues.map((issue) => issue.identifier)
  const pending = subject.ids.length - named.length
  const parts = [...named, ...(pending > 0 ? [`${pending} more`] : [])]
  return parts.length > 0 ? `${verb} ${parts.join(`, `)}` : verb
}

/** The same line the dialog and the Agent page draw above the field. */
export function LaunchHeadline({
  model,
  className,
}: {
  model: LaunchComposerModel
  className?: string
}) {
  const { subject, busy, fixConflicts } = model
  // A chat has no subject to announce; the suggestion pills and the field's
  // own "Ask the agent…" already say what the box is for, and a heading over
  // an empty composer would just be chrome.
  if (subject === null) return null
  const checkedCount = subject.kind === `issues` ? subject.ids.length : 0
  const actionSubject = subject.kind === `action`
  const fixPr = fixConflicts?.pr ?? null
  return (
    <div
      className={cn(
        `flex flex-wrap items-center gap-x-2 gap-y-1.5 px-1 text-lg leading-tight font-semibold`,
        className
      )}
      data-testid="agent-composer-headline"
    >
      <span className="shrink-0">
        {launchHeadlineVerb(subject, fixPr !== null)}
      </span>
      {fixPr ? (
        <FixConflictsChips
          pr={fixPr}
          disabled={busy}
          onClearPr={() => model.setInput(`pr`, ``)}
        />
      ) : (
        <SubjectChips
          issues={model.checkedIssues}
          pendingIssueCount={checkedCount - model.checkedIssues.length}
          action={actionSubject ? model.selectedAction : null}
          actionPending={actionSubject && model.selectedAction === null}
          onRemoveIssue={model.toggleIssue}
          onClearAction={model.clearAction}
          disabled={busy}
        />
      )}
    </div>
  )
}

/** EXP-1233: the picked pull request's issues as the headline's chips — the
 *  same `IssueChip` the issues subject draws, test ids and all
 *  (`agent-composer-chip-issue-<IDENT>`), so a batch PR reads as its issues.
 *  The ✕ clears the PICK: the builtin stays, the card turns back into the
 *  picker. */
function FixConflictsChips({
  pr,
  disabled,
  onClearPr,
}: {
  pr: NonNullable<FixConflictsView[`pr`]>
  disabled?: boolean
  onClearPr: () => void
}) {
  const issueRefs = useIssueRefs()
  return (
    <>
      {pr.issues.map((issue) => (
        <IssueChip
          key={issue.id}
          issue={issue}
          className="max-w-[20rem]"
          preview={false}
          onClick={
            issueRefs ? () => issueRefs.open(issue.identifier) : undefined
          }
          onRemove={onClearPr}
          removeLabel="Clear the pull request"
          removeDisabled={disabled}
          testId={`agent-composer-chip-issue-${issue.identifier}`}
          removeTestId={`agent-composer-chip-issue-${issue.identifier}-remove`}
        />
      ))}
    </>
  )
}
