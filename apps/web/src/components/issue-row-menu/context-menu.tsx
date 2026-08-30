import { useState, type ReactNode } from "react"
import type { Issue, Label, Board, User } from "@/db/schema"
import { formatDateForMutation } from "@/lib/domain"
import { trpc } from "@/lib/trpc-client"
import { useDuplicateInterception } from "@/hooks/use-duplicate-interception"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { statusUpdatePayload } from "@/lib/team-statuses"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  CheckCheck,
  Copy,
  ListTodo,
  SquareCheckBig,
  SquarePen,
  Undo2,
} from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { MoveBoardConfirmDialog } from "@/components/issue-properties/move-board-confirm"
import { DueDateSubmenu } from "./due-date-presets"
import {
  AssigneeSubmenu,
  LabelsSubmenu,
  PrioritySubmenu,
  BoardSubmenu,
  StatusSubmenu,
} from "./submenus"

const UiDeleteIcon = conceptIcon(`ui-delete`)

interface IssueRowContextMenuProps {
  children: ReactNode
  issue: Issue
  issueLabels: Label[]
  labels: Label[]
  onOpenIssue: () => void
  // Team boards (sorted) for the move-to-board submenu (EXP-57);
  // undefined hides the submenu (caller has no team scope).
  boards?: Board[]
  userMap: Map<string, User>
  users: User[]
  // Mobile multi-select entry (FEED-12): Radix already owns the touch
  // long-press gesture for this menu, so selection mode starts from a menu
  // item instead of a competing recognizer. Undefined hides the item
  // (bulk select off for the caller); the item is md:hidden — desktop
  // selects via the row checkboxes.
  onToggleSelect?: () => void
  isSelected?: boolean
}

const TOP_LEVEL_VALUE_CLASS = `w-[5.75rem] shrink-0 text-right normal-case tracking-normal truncate`

export function IssueRowContextMenu({
  children,
  issue,
  issueLabels,
  labels,
  onOpenIssue,
  boards,
  userMap,
  users,
  onToggleSelect,
  isSelected,
}: IssueRowContextMenuProps) {
  const selectedAssignee = issue.assigneeId
    ? (userMap.get(issue.assigneeId) ?? null)
    : null
  const selectedLabelIds = new Set(issueLabels.map((label) => label.id))
  const orderedUsers = [...users].sort((left, right) => {
    if (left.id === issue.assigneeId) {
      return -1
    }

    if (right.id === issue.assigneeId) {
      return 1
    }

    return left.name.localeCompare(right.name)
  })

  const updateIssue = async (updates: {
    assigneeId?: Issue[`assigneeId`]
    dueDate?: Issue[`dueDate`]
    duplicateOfId?: Issue[`duplicateOfId`]
    priority?: Issue[`priority`]
    status?: Issue[`status`]
    statusId?: string
  }) => {
    await trpc.issues.update.mutate({
      id: issue.id,
      ...updates,
    })
  }

  const { resolve: resolveStatus } = useTeamStatusesContext()
  const statusOption = resolveStatus(issue)
  const isCompleted = statusOption.category === `completed`

  const { handleStatusChange, duplicatePicker } = useDuplicateInterception({
    issueId: issue.id,
    onStatusChange: (next) => updateIssue(statusUpdatePayload(next)),
  })

  const applyDueDate = (date: Date | null | undefined) => {
    void updateIssue({
      dueDate: formatDateForMutation(date),
    })
  }

  const toggleLabel = async (labelId: string) => {
    if (selectedLabelIds.has(labelId)) {
      await trpc.issueLabels.remove.mutate({
        issueId: issue.id,
        labelId,
      })
      return
    }

    await trpc.issueLabels.add.mutate({
      issueId: issue.id,
      labelId,
    })
  }

  // The pick lands in the shared confirmation dialog first, because the
  // server renumbers the issue in the target board (EXP-428 — same flow as
  // the detail sidebar's BoardPicker).
  const [pendingBoard, setPendingBoard] = useState<Board | null>(null)

  const moveToBoard = async (boardId: string) => {
    if (boardId === issue.boardId) return
    await trpc.issues.move.mutate({
      id: issue.id,
      boardId,
    })
  }

  const deleteIssue = async () => {
    await trpc.issues.delete.mutate({ id: issue.id })
  }

  const copyText = async (value: string) => {
    if (typeof navigator === `undefined` || !navigator.clipboard) {
      return
    }

    await navigator.clipboard.writeText(value)
  }

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent
          className="w-[17.5rem] p-1.5"
          collisionPadding={12}
        >
          <ContextMenuLabel className="rounded-lg bg-accent/40 px-3 py-2.5">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="truncate text-xs font-mono uppercase tracking-[0.24em] text-muted-foreground">
                  {issue.identifier}
                </div>
                <div className="truncate text-sm font-medium text-foreground">
                  {issue.title}
                </div>
              </div>
            </div>
          </ContextMenuLabel>

          <ContextMenuSeparator />

          <ContextMenuItem onSelect={onOpenIssue}>
            <SquarePen className="size-4" />
            Open issue
          </ContextMenuItem>

          {/* Convenience toggle — deliberately an ENUM write (EXP-314): it
              always lands on the team's builtin Done / Backlog rows via the
              trigger's anchor derivation, exactly like the native swipe
              actions and the coding launcher's parking write. The LABEL keys
              off the resolved category so a custom completed status still
              reads "Move to backlog" (Backlog since EXP-685 retired Todo). */}
          <ContextMenuItem
            onSelect={() => {
              void updateIssue({ status: isCompleted ? `backlog` : `done` })
            }}
          >
            {isCompleted ? (
              <ListTodo className="size-4" />
            ) : (
              <CheckCheck className="size-4" />
            )}
            {isCompleted ? `Move to backlog` : `Mark as done`}
          </ContextMenuItem>

          <ContextMenuItem
            onSelect={() => {
              void copyText(issue.identifier)
            }}
          >
            <Copy className="size-4" />
            Copy issue ID
          </ContextMenuItem>

          {onToggleSelect && (
            <ContextMenuItem
              className="md:hidden"
              onSelect={onToggleSelect}
            >
              <SquareCheckBig className="size-4" />
              {isSelected ? `Deselect` : `Select`}
            </ContextMenuItem>
          )}

          {issue.duplicateOfId && (
            <ContextMenuItem
              onSelect={() => {
                // Server restores 'backlog' and clears the link atomically.
                void updateIssue({ duplicateOfId: null })
              }}
            >
              <Undo2 className="size-4" />
              Unmark duplicate
            </ContextMenuItem>
          )}

          <ContextMenuSeparator />

          <StatusSubmenu
            status={statusOption}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onSelect={handleStatusChange}
          />

          <AssigneeSubmenu
            assigneeId={issue.assigneeId}
            orderedUsers={orderedUsers}
            selectedAssignee={selectedAssignee}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onSelect={(userId) => void updateIssue({ assigneeId: userId })}
          />

          <PrioritySubmenu
            priority={issue.priority}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onSelect={(priority) => void updateIssue({ priority })}
          />

          <LabelsSubmenu
            labels={labels}
            selectedLabelIds={selectedLabelIds}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onToggle={(labelId) => void toggleLabel(labelId)}
          />

          <DueDateSubmenu
            dueDate={issue.dueDate}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onApplyDueDate={applyDueDate}
          />

          {boards && boards.length > 1 && (
            <BoardSubmenu
              boardId={issue.boardId}
              boards={boards}
              topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
              onSelect={(boardId) => {
                const target = boards.find((board) => board.id === boardId)
                if (!target) return
                // Defer past the menu close + focus restore so the dialog's
                // focus trap doesn't fight Radix.
                setTimeout(() => setPendingBoard(target), 0)
              }}
            />
          )}

          {/* No separator above a destructive item (EXP-687): the red is the
              divider, on every client. */}
          <ContextMenuSub>
            <ContextMenuSubTrigger variant="destructive">
              <UiDeleteIcon className="size-4" />
              Delete issue
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className="w-[14rem]">
              <ContextMenuItem
                variant="destructive"
                onSelect={() => {
                  void deleteIssue()
                }}
              >
                <UiDeleteIcon className="size-4" />
                Confirm delete
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
        </ContextMenuContent>
      </ContextMenu>

      {duplicatePicker}

      <MoveBoardConfirmDialog
        board={pendingBoard}
        issueIdentifier={issue.identifier}
        onCancel={() => setPendingBoard(null)}
        onConfirm={(board) => void moveToBoard(board.id)}
      />
    </>
  )
}
