import { conceptIcon, Prompt } from "@exp/ui"
import {
  MERGE_STACK_LABEL,
  STACK_CONFIRM_CANCEL_LABEL,
  type StackMergeConfirm,
} from "@/lib/pr-stack"

const PrMergedIcon = conceptIcon(`pr-merged`)

/** The `issues.mergePr` input a stack merge sends. */
export interface StackMergeInput {
  issueId: string
  mergeStack?: boolean
}

// EXP-1248: the ONE confirm a stack merge asks (Merge stack from the merge
// control, Merge through here from a stack-rail row). Copy =
// `stackMergeConfirm`, ×4 by the contract fixture `stack-merge-choice.json`
// (`confirm`). The caller runs the merge: it owns the pending state, the echo and
// the failure toast.
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
            // `busy` = the Prompt's own pending glyph in place of the mark.
            busy,
            leading: <PrMergedIcon />,
            onSelect: () => {
              if (confirm) onConfirm(confirm.input)
            },
          },
        ]}
      />
    </span>
  )
}
