import { conceptIcon, Prompt } from "@exp/ui"
import {
  MERGE_STACK_LABEL,
  MERGE_THIS_PR_LABEL,
  STACK_CONFIRM_CANCEL_LABEL,
  STACK_MERGE_CANCEL_LABEL,
  STACK_MERGE_CHOICE_TITLE,
  type StackMergeChoice,
  type StackMergeConfirm,
} from "@/lib/pr-stack"

const PrMergedIcon = conceptIcon(`pr-merged`)
const UiLoadingIcon = conceptIcon(`ui-loading`)

/** The `issues.mergePr` input a stack merge sends. */
export interface StackMergeInput {
  issueId: string
  mergeStack?: boolean
}

// EXP-1248: the ONE confirm a stack merge asks (Merge stack from the merge
// control, Merge through here from a stack-rail row). Copy =
// `stackMergeConfirm`, ×4 by the contract fixture `stack-merge-choice.json`
// (`confirm`). The caller runs the merge: it owns the spinner, the echo and
// the failure caption.
export function StackMergeConfirmDialog({
  confirm,
  busy = false,
  onCancel,
  onConfirm,
}: {
  /** null = closed. */
  confirm: StackMergeConfirm | null
  busy?: boolean
  onCancel: () => void
  onConfirm: (input: StackMergeInput) => void
}) {
  return (
    // The card is portalled, but React still bubbles its clicks through this
    // tree: the span keeps them off the list row the Merge sits in.
    <span className="contents" onClick={(e) => e.stopPropagation()}>
      <Prompt
        open={confirm !== null}
        onOpenChange={(next) => {
          if (!next && !busy) onCancel()
        }}
        busy={busy}
        className="sm:max-w-lg"
        data-testid="stack-merge-confirm-dialog"
        title={confirm?.title ?? MERGE_STACK_LABEL}
        body={confirm?.body}
        actions={[
          { label: STACK_CONFIRM_CANCEL_LABEL, role: `cancel` },
          {
            label: confirm?.title ?? MERGE_STACK_LABEL,
            role: `primary`,
            leading: busy ? (
              <UiLoadingIcon className="animate-spin" />
            ) : (
              <PrMergedIcon />
            ),
            onSelect: () => {
              if (confirm) onConfirm(confirm.input)
            },
          },
        ]}
      />
    </span>
  )
}

/** @deprecated EXP-1248: "Merge stack" off the old choice (the Reviews page
 *  until its wave-B rewrite moves to `stackMergeConfirm`). */
export function mergeStackInput(choice: StackMergeChoice): StackMergeInput {
  return { issueId: choice.topIssueId, mergeStack: true }
}

/** @deprecated EXP-1248: "Merge this pull request" = a merge THROUGH this
 *  member, the bottom included (a plain merge of any open-stack member is
 *  refused by the server now). */
export function mergeThisInput(
  _choice: StackMergeChoice,
  issueId: string
): StackMergeInput {
  return { issueId, mergeStack: true }
}

/**
 * @deprecated EXP-1248: EXP-1145's 3-way dialog, kept ONLY for the Reviews
 * page until its wave-B rewrite moves to `StackMergeConfirmDialog`.
 */
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
