import { useState } from "react"
import { deleteIssuePrompt, promptActions } from "@/lib/prompts"
import { contract } from "@exp/domain-contract"
import {
  conceptIcon,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Prompt,
  toast,
} from "@exp/ui"
import { BoardPicker } from "@/components/issue-properties/board-picker"

const UiMoreIcon = conceptIcon(`ui-more`)
const UiShareIcon = conceptIcon(`ui-share`)
const UiCopyIcon = conceptIcon(`ui-copy`)
const NavBoardsIcon = conceptIcon(`nav-boards`)
const UiUndoIcon = conceptIcon(`ui-undo`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const PrClosedIcon = conceptIcon(`pr-closed`)

interface IssueDetailMobileMenuProps {
  issueTitle: string
  // The canonical issue URL — the same one the desktop copy-link button uses.
  issueUrl: string
  teamId: string
  boardId: string
  issueIdentifier: string
  duplicateOfId: string | null
  readOnly?: boolean
  onDelete: () => void | Promise<void>
  onMoveBoard: (boardId: string) => void | Promise<void>
  onUnmarkDuplicate: () => void
  /** EXP-1154: Close PR without merging (`useClosePr`), above Delete; absent
   *  unless a member is looking at an open PR. */
  closePr?: { onSelect: () => void; disabled?: boolean }
}

// The phone issue-detail overflow menu (EXP-687). The desktop breadcrumb keeps
// its row of icon buttons; on a phone every one of them — copy link, unmark
// duplicate, delete — collapses into this ONE `…`, matching the iOS and
// Android toolbar menus. Only the prev/next switcher stays outside it, because
// it is navigation rather than an action.
export function IssueDetailMobileMenu({
  issueTitle,
  issueUrl,
  teamId,
  boardId,
  issueIdentifier,
  duplicateOfId,
  readOnly = false,
  onDelete,
  onMoveBoard,
  onUnmarkDuplicate,
  closePr,
}: IssueDetailMobileMenuProps) {
  const [moveOpen, setMoveOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [canShare] = useState(
    () => typeof navigator !== `undefined` && typeof navigator.share === `function`
  )

  const share = () => {
    if (canShare) {
      void navigator.share({ title: issueTitle, url: issueUrl }).catch(() => {
        // Cancelled or denied — nothing to report.
      })
      return
    }
    if (typeof navigator === `undefined` || !navigator.clipboard) {
      return
    }
    navigator.clipboard.writeText(issueUrl).then(
      () => toast.success(`Link copied`),
      () => {
        // Clipboard denied (permissions/insecure context).
      }
    )
  }

  const deleteCopy = deleteIssuePrompt(issueIdentifier)

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Issue actions"
          >
            <UiMoreIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-[14rem]">
          <DropdownMenuItem onSelect={share}>
            {canShare ? <UiShareIcon /> : <UiCopyIcon />}
            {canShare ? `Share` : `Copy link`}
          </DropdownMenuItem>
          {!readOnly && (
            <DropdownMenuItem
              onSelect={() => {
                // Defer past the menu close + focus restore so the sheet's
                // focus trap doesn't fight Radix.
                setTimeout(() => setMoveOpen(true), 0)
              }}
            >
              <NavBoardsIcon />
              Move to board
            </DropdownMenuItem>
          )}
          {!readOnly && duplicateOfId && (
            <DropdownMenuItem onSelect={onUnmarkDuplicate}>
              <UiUndoIcon />
              Unmark duplicate
            </DropdownMenuItem>
          )}
          {/* No separator above a destructive item (EXP-687). */}
          {closePr && (
            <DropdownMenuItem
              variant="destructive"
              disabled={closePr.disabled}
              data-testid="issue-close-pr"
              onSelect={closePr.onSelect}
            >
              <PrClosedIcon />
              {contract.diffUi.closePr}
            </DropdownMenuItem>
          )}
          {!readOnly && (
            <DropdownMenuItem
              variant="destructive"
              onSelect={() => {
                setTimeout(() => setDeleteOpen(true), 0)
              }}
            >
              <UiDeleteIcon />
              Delete issue
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Rendered trigger-less: the menu item above owns the open state, and
          the picker still routes the pick through MoveBoardConfirmDialog. */}
      {!readOnly && (
        <BoardPicker
          teamId={teamId}
          selectedBoardId={boardId}
          issueIdentifier={issueIdentifier}
          onSelect={onMoveBoard}
          open={moveOpen}
          onOpenChange={setMoveOpen}
          hideTrigger
        />
      )}

      <Prompt
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        data-testid="issue-delete-confirm"
        title={deleteCopy.title}
        body={deleteCopy.body}
        actions={promptActions(deleteCopy, {
          delete: {
            onSelect: () => {
              setDeleteOpen(false)
              void onDelete()
            },
          },
        })}
      />
    </>
  )
}
