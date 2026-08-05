import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
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
// here first. Worded byte-identically to the desktop, iOS and Android
// clients.
export function MoveBoardConfirmDialog({
  board,
  issueIdentifier,
  onCancel,
  onConfirm,
}: MoveBoardConfirmDialogProps) {
  return (
    <AlertDialog
      open={board !== null}
      onOpenChange={(o) => {
        if (!o) onCancel()
      }}
    >
      <AlertDialogContent data-testid="issue-move-board-confirm">
        <AlertDialogHeader>
          <AlertDialogTitle>Move issue</AlertDialogTitle>
          <AlertDialogDescription>
            {`Move ${issueIdentifier ?? `this issue`} to "${board?.name ?? ``}"? The issue will get a new identifier in that board.`}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={() => {
              if (board) onConfirm(board)
            }}
          >
            Move
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
