import { contract } from "@exp/domain-contract"

import { SubjectChips } from "@/components/launch-dialog/subject-chips"
import type {
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
// they picked was already loaded. The subject leads instead — "Implement
// <issues>", or (NEW-2) a picked prompt's own chip, its name — and the field
// below it drops to "Additional instructions (optional)…".
//
// The verbs are the contract's (`contract.composerUi`, ×4): the same two
// words on the web dialog, the Agent page, the IDE's start-coding dialog and
// the two native composers. The CHIPS are each client's own; on the web they
// are the very same `SubjectChips` the card used to carry, ✕ and all, so
// removing the last one still turns the run back into a chat.

const { composerUi } = contract

/** The verb in front of the chips: pure, so the cases are a test. A prompt
 *  needs none: its name IS the headline. */
export function launchHeadlineVerb(subject: LaunchSubject): string {
  if (subject === null) return composerUi.chatHeadline
  // A prompt's name, an agent's chip (NEW-3): the headline IS the chip.
  if (subject.kind === `prompt` || subject.kind === `agent`) return ``
  return composerUi.implementHeadline
}

/**
 * The headline as PLAIN TEXT — the dialog's accessible name, and what a test
 * can assert without walking chips. Falls back to the subject's shape while
 * its rows are still syncing.
 */
export function launchHeadlineText(model: LaunchComposerModel): string {
  const { subject } = model
  const verb = launchHeadlineVerb(subject)
  if (subject === null) return verb
  if (subject.kind === `prompt`) return model.selectedPrompt?.name ?? `Prompt`
  if (subject.kind === `agent`) return model.selectedAgent?.name ?? `Agent`
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
  const { subject, busy } = model
  // A chat has no subject to announce; the suggestion pills and the field's
  // own "Ask the agent…" already say what the box is for, and a heading over
  // an empty composer would just be chrome.
  if (subject === null) return null
  const checkedCount = subject.kind === `issues` ? subject.ids.length : 0
  const promptSubject = subject.kind === `prompt`
  const agentSubject = subject.kind === `agent`
  const verb = launchHeadlineVerb(subject)
  return (
    <div
      className={cn(
        `flex flex-wrap items-center gap-x-2 gap-y-1.5 px-1 text-lg leading-tight font-semibold`,
        className
      )}
      data-testid="agent-composer-headline"
    >
      {verb && <span className="shrink-0">{verb}</span>}
      <SubjectChips
        issues={model.checkedIssues}
        pendingIssueCount={checkedCount - model.checkedIssues.length}
        prompt={promptSubject ? model.selectedPrompt : null}
        promptPending={promptSubject && model.selectedPrompt === null}
        agent={agentSubject ? model.selectedAgent : null}
        agentPending={agentSubject && model.selectedAgent === null}
        onRemoveIssue={model.toggleIssue}
        onClearPrompt={model.clearPrompt}
        onClearAgent={model.clearAgent}
        disabled={busy}
      />
    </div>
  )
}
