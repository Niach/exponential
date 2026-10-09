import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { ListTodo } from "lucide-react"
import type { Board, Issue, IssueLabel, Team } from "@/db/schema"
import { formatDateForMutation, type IssueEstimation } from "@/lib/domain"
import { issueCollection, issueLabelCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { deleteIssuePrompt, promptActions } from "@/lib/prompts"
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
  Menu,
  Prompt,
  issueMenuEntries,
  type IssueMenuCondition,
  type MenuTarget,
} from "@exp/ui"
import type { IssueMenuSelection } from "./selection"
import { dueDateSlot } from "./due-date-presets"
import {
  assigneeSlot,
  boardSlot,
  estimateSlot,
  labelsSlot,
  prioritySlot,
  statusSlot,
} from "./submenus"

// The rows, their order and their glyphs are `@exp/ui` ISSUE_MENU_LAYOUT's:
// this session only fills each row's slot (the verb, the current value, the
// picker body) from the live issue, and `Menu` draws them — at the pointer
// on a pointer device, as a bottom sheet on a phone.

const NO_ISSUE_LABELS: IssueLabel[] = []

export function IssueMenuSession({
  target,
  team,
  open,
  onOpenChange,
  selection,
}: {
  target: MenuTarget
  team: Team | null
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The list the row belongs to, when it takes a selection (phones). */
  selection?: IssueMenuSelection
}) {
  const issueId = target.id
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
  const { resolve: resolveStatus, options: statusOptions } =
    useTeamStatusesContext()
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
  // EXP-1215: Delete confirms in the shared `delete-issue` prompt, the card
  // the header's `…` menu and the phone menu raise too.
  const [deleteOpen, setDeleteOpen] = useState(false)
  const deleteCopy = deleteIssuePrompt(issue?.identifier ?? ``)

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

  const conditions = new Set<IssueMenuCondition>()
  if (selection) conditions.add(`phone`)
  if (issue?.duplicateOfId) conditions.add(`duplicate`)
  if (estimation !== `none`) conditions.add(`estimation`)
  if (boards.length > 1) conditions.add(`boards`)

  const entries =
    issue && statusOption
      ? issueMenuEntries({
          header: { identifier: issue.identifier, title: issue.title },
          conditions,
          slots: {
            open: {
              onSelect: () =>
                issueRefs?.open(issue.identifier, { from: target.from }),
            },
            // Convenience toggle — deliberately an ENUM write (EXP-314): it
            // lands on the team's builtin Done / Backlog rows via the anchor
            // derivation, like the native swipe actions. The LABEL keys off
            // the resolved category, so a custom completed status still reads
            // "Move to backlog".
            "toggle-done": {
              ...(isCompleted
                ? { label: `Move to backlog`, icon: ListTodo }
                : {}),
              onSelect: () => {
                void updateIssue({ status: isCompleted ? `backlog` : `done` })
              },
            },
            "copy-id": { onSelect: () => void copyText(issue.identifier) },
            // Mobile multi-select entry (FEED-12): the long-press already
            // opens this menu, so selection mode starts from a row; md+
            // selects via the row checkboxes.
            ...(selection
              ? {
                  select: {
                    label: selection.isSelected(issue.id) ? `Deselect` : `Select`,
                    phoneOnly: true,
                    onSelect: () => selection.toggle(issue.id),
                  },
                }
              : {}),
            // Server restores 'backlog' and clears the link atomically.
            "unmark-duplicate": {
              onSelect: () => void updateIssue({ duplicateOfId: null }),
            },
            status: statusSlot({
              status: statusOption,
              options: statusOptions,
              onSelect: handleStatusChange,
            }),
            assignee: assigneeSlot({
              assigneeId: issue.assigneeId,
              orderedUsers,
              selectedAssignee,
              onSelect: (userId) => void updateIssue({ assigneeId: userId }),
            }),
            priority: prioritySlot({
              priority: issue.priority,
              onSelect: (priority) => void updateIssue({ priority }),
            }),
            labels: labelsSlot({
              labels,
              selectedLabelIds: new Set(issueLabels.map((label) => label.id)),
              onToggle: (labelId) => void toggleLabel(labelId),
            }),
            estimate: estimateSlot({
              estimate: issue.estimate,
              estimation,
              onSelect: (estimate) => void updateIssue({ estimate }),
            }),
            "due-date": dueDateSlot({
              dueDate: issue.dueDate,
              onApplyDueDate: (date) =>
                void updateIssue({ dueDate: formatDateForMutation(date) }),
            }),
            "move-board": boardSlot({
              boardId: issue.boardId,
              boards,
              onSelect: (boardId) => {
                const next = boards.find((board) => board.id === boardId)
                // Defer past the menu close + focus restore so the dialog's
                // focus trap doesn't fight Radix.
                if (next) setTimeout(() => setPendingBoard(next), 0)
              },
            }),
            // EXP-760: the same six sides the issue header's `…` offers.
            "add-relation": {
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
            // EXP-1215: confirms in the shared prompt, opened deferred past
            // the menu close + focus restore, like the move confirm.
            delete: { onSelect: () => setTimeout(() => setDeleteOpen(true), 0) },
          },
        })
      : []

  return (
    <>
      <Menu
        mode="pointer"
        anchor={target.anchor}
        open={open && issue !== null && statusOption !== null}
        onOpenChange={onOpenChange}
        entries={entries}
        aria-label="Issue actions"
        title={issue?.identifier}
        contentClassName="w-(--menu-max-width)"
        returnFocus={target.origin}
        data-testid="issue-context-menu"
      />

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

      <Prompt
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        data-testid="issue-delete-confirm"
        title={deleteCopy.title}
        body={deleteCopy.body}
        actions={promptActions(deleteCopy, {
          delete: {
            onSelect: async () => {
              await trpc.issues.delete.mutate({ id: issueId })
              setDeleteOpen(false)
            },
          },
        })}
      />
    </>
  )
}
