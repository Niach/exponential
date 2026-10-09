import { useState } from "react"
import { contract } from "@exp/domain-contract"
import { useNavigate } from "@tanstack/react-router"
import type { Board, Issue } from "@/db/schema"
import {
  conceptIcon,
  Button,
  IconTooltip,
  Menu,
  Prompt,
  toast,
  type MenuEntry,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { deleteIssuePrompt, promptActions } from "@/lib/prompts"
import {
  RELATION_SIDES,
  pickLabel,
  useAddRelation,
} from "@/components/issue-relations-card"
import { useClosePr } from "@/components/close-pr-dialog"
import { BoardPicker } from "@/components/issue-properties/board-picker"

const UiMoreIcon = conceptIcon(`ui-more`)
const UiShareIcon = conceptIcon(`ui-share`)
const UiCopyIcon = conceptIcon(`ui-copy`)
const RelationSectionIcon = conceptIcon(`relation-section`)
const NavBoardsIcon = conceptIcon(`nav-boards`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const UiUndoIcon = conceptIcon(`ui-undo`)
const PrClosedIcon = conceptIcon(`pr-closed`)

/** The issue's canonical URL, for Copy link and the phone's share sheet. */
export function issueUrlFor(
  teamSlug: string,
  boardSlug: string,
  issueIdentifier: string
): string {
  const origin = typeof window === `undefined` ? `` : window.location.origin
  return `${origin}/t/${teamSlug}/boards/${boardSlug}/issues/${issueIdentifier}`
}

// EXP-760 → the UI cleanup batch: THE issue `…` — ONE menu for the md+ work
// header, the session route's issue face and the phone header (the old
// `IssueDetailMobileMenu` folded in): Copy link (Share where the browser has
// a share sheet) · Add relation ▸ · Move to board (where the host can move
// it) · Unmark duplicate · Close PR (EXP-1154, an open PR) · Delete issue
// (EXP-1215, the shared `delete-issue` prompt). It is the shared `Menu`, so a
// phone gets the same rows as a bottom sheet.
export function IssueActionsMenu({
  issue,
  board,
  teamSlug,
  teamId,
  readOnly = false,
  onMoveBoard,
  onUnmarkDuplicate,
}: {
  issue: Issue
  board: Pick<Board, `slug`>
  teamSlug: string
  /** With `onMoveBoard`, the menu offers "Move to board" (the phone header). */
  teamId?: string
  readOnly?: boolean
  onMoveBoard?: (boardId: string) => void | Promise<void>
  /** Defaults to the plain `duplicateOfId: null` write. */
  onUnmarkDuplicate?: () => void
}) {
  const navigate = useNavigate()
  // The "Add relation" flow behind this menu and the list row's context menu
  // — one hook, one picker (issue-relations-card.tsx).
  const addRelation = useAddRelation(issue.id)
  const issueUrl = issueUrlFor(teamSlug, board.slug, issue.identifier)
  const closePr = useClosePr(issue, { readOnly })
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [moveOpen, setMoveOpen] = useState(false)
  const deleteCopy = deleteIssuePrompt(issue.identifier)
  const [canShare] = useState(
    () =>
      typeof navigator !== `undefined` &&
      typeof navigator.share === `function` &&
      typeof window !== `undefined` &&
      window.matchMedia?.(`(pointer: coarse)`).matches === true
  )

  const copyLink = () => {
    if (canShare) {
      void navigator.share({ title: issue.title, url: issueUrl }).catch(() => {
        // Cancelled or denied — nothing to report.
      })
      return
    }
    if (typeof navigator === `undefined` || !navigator.clipboard) return
    navigator.clipboard.writeText(issueUrl).then(
      () => toast.success(`Link copied`),
      () => {
        // Clipboard denied (permissions/insecure context) — the toast would
        // be a lie, so say nothing.
      }
    )
  }

  // Delete is a hard delete (issues.delete cleans up attachments
  // server-side); once it commits, land back on the board.
  const handleDeleteIssue = async () => {
    await trpc.issues.delete.mutate({ id: issue.id })
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug, boardSlug: board.slug },
      search: {},
    })
  }

  const canMove = !readOnly && onMoveBoard !== undefined && teamId !== undefined
  const entries: MenuEntry[] = [
    {
      kind: `item`,
      id: `copy-link`,
      label: canShare ? `Share` : `Copy link`,
      icon: canShare ? UiShareIcon : UiCopyIcon,
      onSelect: copyLink,
    },
    ...(readOnly
      ? []
      : [
          {
            kind: `submenu` as const,
            id: `add-relation`,
            label: `Add relation`,
            icon: RelationSectionIcon,
            contentClassName: `w-[12rem]`,
            entries: RELATION_SIDES.filter((entry) => entry.pickable).map(
              (entry) => ({
                kind: `item` as const,
                id: entry.side,
                label: pickLabel(entry.type, entry.direction),
                icon: entry.icon,
                onSelect: () => addRelation.pick(entry),
              })
            ),
          },
        ]),
    ...(canMove
      ? [
          {
            kind: `item` as const,
            id: `move-board`,
            label: `Move to board`,
            icon: NavBoardsIcon,
            // Deferred past the menu close + focus restore so the picker's
            // focus trap does not fight Radix.
            onSelect: () => setTimeout(() => setMoveOpen(true), 0),
          },
        ]
      : []),
    ...(!readOnly && issue.duplicateOfId
      ? [
          {
            kind: `item` as const,
            id: `unmark-duplicate`,
            label: `Unmark duplicate`,
            icon: UiUndoIcon,
            onSelect:
              onUnmarkDuplicate ??
              (() => {
                void trpc.issues.update.mutate({
                  id: issue.id,
                  duplicateOfId: null,
                })
              }),
          },
        ]
      : []),
    // No separator above a destructive row (EXP-687): the red is the
    // divider, on every client. Close PR confirms in a dialog.
    ...(closePr.canClose
      ? [
          {
            kind: `item` as const,
            id: `close-pr`,
            label: contract.diffUi.closePr,
            icon: PrClosedIcon,
            destructive: true,
            disabled: closePr.closing,
            "data-testid": `issue-close-pr`,
            onSelect: closePr.request,
          },
        ]
      : []),
    ...(readOnly
      ? []
      : [
          {
            kind: `item` as const,
            id: `delete`,
            label: `Delete issue`,
            icon: UiDeleteIcon,
            destructive: true,
            "data-testid": `issue-delete`,
            onSelect: () => setTimeout(() => setDeleteOpen(true), 0),
          },
        ]),
  ]

  return (
    <>
      <IconTooltip label="More actions">
        <Menu
          align="end"
          aria-label="Issue actions"
          title={issue.identifier}
          contentClassName="w-[14rem]"
          entries={entries}
          trigger={
            <Button variant="ghost" size="icon-sm" aria-label="Issue actions">
              <UiMoreIcon />
            </Button>
          }
        />
      </IconTooltip>
      {addRelation.dialog}
      {closePr.dialog}
      {/* Rendered trigger-less: the row above owns the open state, and the
          picker still routes the pick through MoveBoardConfirmDialog. */}
      {canMove && (
        <BoardPicker
          teamId={teamId}
          selectedBoardId={issue.boardId}
          issueIdentifier={issue.identifier}
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
          // Async: the pill spins until the delete lands and the page leaves.
          delete: { onSelect: handleDeleteIssue },
        })}
      />
    </>
  )
}
