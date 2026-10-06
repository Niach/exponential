import { Prompt } from "@exp/ui"
import { moveIssuePrompt, promptActions } from "@/lib/prompts"
import type { Board } from "@/db/schema"

interface MoveBoardConfirmDialogProps {
  // The picked target board; null keeps the dialog closed.
  board: Board | null
  // Named in the confirmation copy; absent reads as "this issue".
  issueIdentifier?: string | null
  onCancel: () => void
  onConfirm: (board: Board) => void
}

// The ONE web copy of the move-to-board confirmation (EXP-426/EXP-428):
// the server renumbers the issue in the target board (EXP-42 → ABC-17), so
// every move surface (detail sidebar picker, issue row context menu) lands
// here first. EXP-1215: one question (`Move X to "Board"?`) + the renumber
// fact as the body.
export function MoveBoardConfirmDialog({
  board,
  issueIdentifier,
  onCancel,
  onConfirm,
}: MoveBoardConfirmDialogProps) {
  const copy = moveIssuePrompt(
    issueIdentifier ?? `this issue`,
    board?.name ?? ``
  )

  return (
    <Prompt
      open={board !== null}
      onOpenChange={(o) => {
        if (!o) onCancel()
      }}
      data-testid="issue-move-board-confirm"
      title={copy.title}
      body={copy.body}
      actions={promptActions(copy, {
        move: {
          onSelect: () => {
            if (board) onConfirm(board)
            // The callers close on `onCancel` (the old dialog's close path).
            onCancel()
          },
        },
      })}
    />
  )
}
