import { conceptIcon, Prompt } from "@exp/ui"
import {
  MERGE_STACK_LABEL,
  MERGE_THIS_PR_LABEL,
  STACK_MERGE_CANCEL_LABEL,
  STACK_MERGE_CHOICE_TITLE,
  type StackMergeChoice,
} from "@/lib/pr-stack"

const PrMergedIcon = conceptIcon(`pr-merged`)
const UiLoadingIcon = conceptIcon(`ui-loading`)

/** The `issues.mergePr` input a button of the stack dialog sends. */
export interface StackMergeInput {
  issueId: string
  mergeStack?: boolean
}

/** "Merge stack": the whole open chain, from its TOP member. */
export function mergeStackInput(choice: StackMergeChoice): StackMergeInput {
  return { issueId: choice.topIssueId, mergeStack: true }
}

/** "Merge this pull request": the bottom member merges plainly; any other
 *  member merges the chain bottom-up THROUGH itself (`thisSentence`), which
 *  the server does for `mergeStack` on that member. */
export function mergeThisInput(
  choice: StackMergeChoice,
  issueId: string
): StackMergeInput {
  return choice.position === 1 ? { issueId } : { issueId, mergeStack: true }
}

// EXP-1145: the ONE dialog a Merge control on a member of an open PR stack
// opens instead of its plain confirm (`SessionMergeButton` and the Reviews
// rows). Copy = `stackMergeChoice`, byte-locked ×4 by the contract fixture
// `stack-merge-choice.json`. The caller runs the merges: it owns the spinner,
// the echo and the failure caption.
export function StackMergeChoiceDialog({
  choice,
  issueId,
  busy = false,
  onCancel,
  onMerge,
}: {
  /** null = closed. */
  choice: StackMergeChoice | null
  /** The issue whose Merge was pressed. */
  issueId: string
  busy?: boolean
  onCancel: () => void
  /** `stack` = Merge stack, `this` = Merge this pull request. */
  onMerge: (input: StackMergeInput, which: `stack` | `this`) => void
}) {
  return (
    // The card is portalled, but React still bubbles its clicks through this
    // tree: the span keeps them off the list row the Merge sits in.
    <span className="contents" onClick={(e) => e.stopPropagation()}>
      <Prompt
        open={choice !== null}
        onOpenChange={(next) => {
          if (!next && !busy) onCancel()
        }}
        busy={busy}
        className="sm:max-w-lg"
        data-testid="stack-merge-choice-dialog"
        title={STACK_MERGE_CHOICE_TITLE}
        body={
          choice ? (
            <div className="whitespace-pre-line">{choice.body}</div>
          ) : undefined
        }
        actions={[
          { label: STACK_MERGE_CANCEL_LABEL, role: `cancel` },
          {
            label: MERGE_THIS_PR_LABEL,
            onSelect: () => {
              if (choice) onMerge(mergeThisInput(choice, issueId), `this`)
            },
          },
          {
            label: MERGE_STACK_LABEL,
            role: `primary`,
            leading: busy ? (
              <UiLoadingIcon className="animate-spin" />
            ) : (
              <PrMergedIcon />
            ),
            onSelect: () => {
              if (choice) onMerge(mergeStackInput(choice), `stack`)
            },
          },
        ]}
      />
    </span>
  )
}
