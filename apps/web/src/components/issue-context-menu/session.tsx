import { useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  CheckCheck,
  Copy,
  ListTodo,
  SquareCheckBig,
  SquarePen,
  Undo2,
} from "lucide-react"
import type { Board, Issue, IssueLabel, Team } from "@/db/schema"
import { formatDateForMutation, type IssueEstimation } from "@/lib/domain"
import { issueCollection, issueLabelCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import {
  useTeamBoards,
  useTeamLabels,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { useDuplicateInterception } from "@/hooks/use-duplicate-interception"
import { statusUpdatePayload } from "@/lib/team-statuses"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { MoveBoardConfirmDialog } from "@/components/issue-properties/move-board-confirm"
import {
  RELATION_SIDES,
  pickLabel,
  useAddRelation,
} from "@/components/issue-relations-card"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
  conceptIcon,
} from "@exp/ui"
import type { IssueMenuTarget } from "./gestures"
import type { IssueMenuSelection } from "./selection"
import { DueDateSubmenu } from "./due-date-presets"
import {
  AssigneeSubmenu,
  BoardSubmenu,
  EstimateSubmenu,
  LabelsSubmenu,
  PrioritySubmenu,
  StatusSubmenu,
} from "./submenus"

const UiDeleteIcon = conceptIcon(`ui-delete`)
const RelationSectionIcon = conceptIcon(`relation-section`)

const TOP_LEVEL_VALUE_CLASS = `w-[5.75rem] shrink-0 text-right normal-case tracking-normal truncate`

/** A right-click opens the menu on the mouse DOWN (macOS, and the keyboard
 *  key), beside the cursor; a Radix item selects on a pointer-UP it did not
 *  see the down of. So a release a few px into the menu used to fire the
 *  item under it — "opens and instantly closes again". A pointer-up this
 *  soon after opening is the opening gesture's own release, never a pick. */
const POINTER_UP_GRACE_MS = 300

const NO_ISSUE_LABELS: IssueLabel[] = []

export function IssueMenuSession({
  target,
  team,
  open,
  onOpenChange,
  selection,
}: {
  target: IssueMenuTarget
  team: Team | null
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The list the row belongs to, when it takes a selection (phones). */
  selection?: IssueMenuSelection
}) {
  const issueId = target.issueId
  const { data: issueRows, isReady: issueReady } = useLiveQuery(
    (query) =>
      query
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.id, issueId)),
    [issueId]
  )
  const issue = (issueRows?.[0] ?? null) as Issue | null
  const { data: issueLabelRows } = useLiveQuery(
    (query) =>
      query
        .from({ issueLabels: issueLabelCollection })
        .where(({ issueLabels }) => eq(issueLabels.issueId, issueId)),
    [issueId]
  )
  const labels = useTeamLabels(team?.id)
  const issueLabels = useMemo(() => {
    const ids = new Set(
      ((issueLabelRows ?? NO_ISSUE_LABELS) as IssueLabel[]).map(
        (row) => row.labelId
      )
    )
    return labels.filter((label) => ids.has(label.id))
  }, [issueLabelRows, labels])
  const boards = useTeamBoards(team?.id)
  const { users, userMap } = useTeamUsers(team?.id)
  const { resolve: resolveStatus } = useTeamStatusesContext()
  const issueRefs = useIssueRefs()
  const estimation = (team?.estimationType ?? `none`) as IssueEstimation

  // The issue left the synced set under the menu (deleted, moved out of
  // reach): nothing to act on.
  useEffect(() => {
    if (open && issueReady && !issue) onOpenChange(false)
  }, [open, issueReady, issue, onOpenChange])

  const updateIssue = async (updates: {
    assigneeId?: Issue[`assigneeId`]
    dueDate?: Issue[`dueDate`]
    duplicateOfId?: Issue[`duplicateOfId`]
    estimate?: Issue[`estimate`]
    priority?: Issue[`priority`]
    status?: Issue[`status`]
    statusId?: string
  }) => {
    await trpc.issues.update.mutate({ id: issueId, ...updates })
  }

  const { handleStatusChange, duplicatePicker } = useDuplicateInterception({
    issueId,
    onStatusChange: (next) => updateIssue(statusUpdatePayload(next)),
  })

  // EXP-760: the same six sides the issue header's `…` offers, from the one
  // hook that owns the picker and the duplicate dual-write.
  const addRelation = useAddRelation(issueId)

  // The pick lands in the shared confirmation dialog first, because the
  // server renumbers the issue in the target board (EXP-428 — same flow as
  // the detail sidebar's BoardPicker).
  const [pendingBoard, setPendingBoard] = useState<Board | null>(null)

  const openedAt = useRef(performance.now())

  const toggleLabel = async (labelId: string) => {
    const has = issueLabels.some((label) => label.id === labelId)
    if (has) {
      await trpc.issueLabels.remove.mutate({ issueId, labelId })
      return
    }
    await trpc.issueLabels.add.mutate({ issueId, labelId })
  }

  const copyText = async (value: string) => {
    if (typeof navigator === `undefined` || !navigator.clipboard) return
    await navigator.clipboard.writeText(value)
  }

  const statusOption = issue ? resolveStatus(issue) : null
  const isCompleted = statusOption?.category === `completed`
  const selectedAssignee =
    issue?.assigneeId ? (userMap.get(issue.assigneeId) ?? null) : null
  const orderedUsers = useMemo(
    () =>
      [...users].sort((left, right) => {
        if (left.id === issue?.assigneeId) return -1
        if (right.id === issue?.assigneeId) return 1
        return left.name.localeCompare(right.name)
      }),
    [users, issue?.assigneeId]
  )
  const { anchor } = target

  return (
    <>
      <DropdownMenu open={open && issue !== null} onOpenChange={onOpenChange}>
        {/* The anchor: nothing to see, nothing to focus — a 0×0 point at the
            pointer (the element's box for a keyboard-invoked menu), which is
            what Radix's own ContextMenu anchors to. */}
        <DropdownMenuTrigger asChild>
          <span
            aria-hidden
            tabIndex={-1}
            data-testid="issue-context-menu-anchor"
            style={{
              position: `fixed`,
              left: anchor.x,
              top: anchor.y,
              width: anchor.width,
              height: anchor.height,
              pointerEvents: `none`,
            }}
          />
        </DropdownMenuTrigger>
        {issue && statusOption && (
          <DropdownMenuContent
            aria-label="Issue actions"
            side={anchor.width > 0 ? `bottom` : `right`}
            align="start"
            sideOffset={2}
            collisionPadding={12}
            className="w-[17.5rem] p-1.5"
            onCloseAutoFocus={(event) => {
              // Never the invisible anchor: keyboard users go back to the
              // row (a sidebar link, a chip); a plain div takes no focus.
              event.preventDefault()
              target.origin?.focus?.({ preventScroll: true })
            }}
            onPointerUpCapture={(event) => {
              if (performance.now() - openedAt.current < POINTER_UP_GRACE_MS) {
                event.stopPropagation()
              }
            }}
          >
            <DropdownMenuLabel className="rounded-lg bg-accent/40 px-3 py-2.5">
              <div className="min-w-0">
                <div className="truncate font-mono text-xs text-foreground/50">
                  {issue.identifier}
                </div>
                <div className="truncate text-sm font-medium text-foreground">
                  {issue.title}
                </div>
              </div>
            </DropdownMenuLabel>

            <DropdownMenuSeparator />

            <DropdownMenuItem
              onSelect={() =>
                issueRefs?.open(issue.identifier, { from: target.from })
              }
            >
              <SquarePen className="size-4" />
              Open issue
            </DropdownMenuItem>

            {/* Convenience toggle — deliberately an ENUM write (EXP-314): it
                always lands on the team's builtin Done / Backlog rows via the
                trigger's anchor derivation, exactly like the native swipe
                actions and the coding launcher's parking write. The LABEL keys
                off the resolved category so a custom completed status still
                reads "Move to backlog" (Backlog since EXP-685 retired Todo). */}
            <DropdownMenuItem
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
            </DropdownMenuItem>

            <DropdownMenuItem
              onSelect={() => {
                void copyText(issue.identifier)
              }}
            >
              <Copy className="size-4" />
              Copy issue ID
            </DropdownMenuItem>

            {/* Mobile multi-select entry (FEED-12): the long-press already
                opens this menu, so selection mode starts from an item; md+
                selects via the row checkboxes. */}
            {selection && (
              <DropdownMenuItem
                className="md:hidden"
                onSelect={() => selection.toggle(issue.id)}
              >
                <SquareCheckBig className="size-4" />
                {selection.isSelected(issue.id) ? `Deselect` : `Select`}
              </DropdownMenuItem>
            )}

            {issue.duplicateOfId && (
              <DropdownMenuItem
                onSelect={() => {
                  // Server restores 'backlog' and clears the link atomically.
                  void updateIssue({ duplicateOfId: null })
                }}
              >
                <Undo2 className="size-4" />
                Unmark duplicate
              </DropdownMenuItem>
            )}

            <DropdownMenuSeparator />

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
              selectedLabelIds={new Set(issueLabels.map((label) => label.id))}
              topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
              onToggle={(labelId) => void toggleLabel(labelId)}
            />

            <EstimateSubmenu
              estimate={issue.estimate}
              estimation={estimation}
              topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
              onSelect={(estimate) => void updateIssue({ estimate })}
            />

            <DueDateSubmenu
              dueDate={issue.dueDate}
              topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
              onApplyDueDate={(date) =>
                void updateIssue({ dueDate: formatDateForMutation(date) })
              }
            />

            {boards.length > 1 && (
              <BoardSubmenu
                boardId={issue.boardId}
                boards={boards}
                topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
                onSelect={(boardId) => {
                  const next = boards.find((board) => board.id === boardId)
                  if (!next) return
                  // Defer past the menu close + focus restore so the dialog's
                  // focus trap doesn't fight Radix.
                  setTimeout(() => setPendingBoard(next), 0)
                }}
              />
            )}

            <DropdownMenuSub>
              <DropdownMenuSubTrigger>
                <RelationSectionIcon className="size-4" />
                Add relation
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="w-[12rem]">
                {RELATION_SIDES.filter((entry) => entry.pickable).map(
                  (entry) => {
                    const Icon = entry.icon
                    return (
                      <DropdownMenuItem
                        key={entry.side}
                        onSelect={() => addRelation.pick(entry)}
                      >
                        <Icon className="size-4" />
                        {pickLabel(entry.type, entry.direction)}
                      </DropdownMenuItem>
                    )
                  }
                )}
              </DropdownMenuSubContent>
            </DropdownMenuSub>

            {/* No separator above a destructive item (EXP-687): the red is
                the divider, on every client. */}
            <DropdownMenuSub>
              <DropdownMenuSubTrigger variant="destructive">
                <UiDeleteIcon className="size-4" />
                Delete issue
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="w-[14rem]">
                <DropdownMenuItem
                  variant="destructive"
                  onSelect={() => {
                    void trpc.issues.delete.mutate({ id: issueId })
                  }}
                >
                  <UiDeleteIcon className="size-4" />
                  Confirm delete
                </DropdownMenuItem>
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          </DropdownMenuContent>
        )}
      </DropdownMenu>

      {duplicatePicker}
      {addRelation.dialog}

      <MoveBoardConfirmDialog
        board={pendingBoard}
        issueIdentifier={issue?.identifier ?? ``}
        onCancel={() => setPendingBoard(null)}
        onConfirm={(board) => {
          if (board.id === issue?.boardId) return
          void trpc.issues.move.mutate({ id: issueId, boardId: board.id })
        }}
      />
    </>
  )
}
